use anyhow::Result;
use reqwest::Client;
use solana_sdk::{signature::Keypair, signer::Signer};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tracing::{info, warn, error};

use crate::{
    arbitrage::detector::Opportunity,
    config::Config,
    utils::{jito, jupiter, rpc::{self, BlockhashCache}},
};

pub const SOL_MINT: &str = "So11111111111111111111111111111111111111112";

pub async fn execute(
    client:    &Client,
    wallet:    &Keypair,
    config:    &Config,
    opp:       &Opportunity,
    bh_cache:  &Arc<BlockhashCache>,
) -> Result<bool> {
    let t0         = Instant::now();
    let pubkey     = wallet.pubkey();
    let pubkey_str = pubkey.to_string();
    let api_key    = config.jupiter_api_key.as_deref();

    let sol_is_a = opp.sell_pool.token_a == SOL_MINT;
    let mid_mint = if sol_is_a { opp.sell_pool.token_b } else { opp.sell_pool.token_a };

    // Phase 1: balance check + quote1 + blockhash in parallel
    let bh_future = {
        let client   = client.clone();
        let url      = config.rpc_url.clone();
        let rpc_host = config.rpc_host.clone();
        let cache    = Arc::clone(bh_cache);
        async move {
            if let Some(h) = cache.get_fresh() { return Ok::<_, anyhow::Error>(h); }
            rpc::get_latest_blockhash(&client, &url, &rpc_host).await
        }
    };
    let (lamports, q1, blockhash) = tokio::try_join!(
        rpc::get_balance(client, &config.rpc_url, &config.rpc_host, &pubkey),
        jupiter::get_quote(client, SOL_MINT, mid_mint, config.trade_lamports, 50, api_key),
        bh_future,
    )?;
    let ms_q1 = t0.elapsed().as_millis();

    if lamports < config.trade_lamports + config.reserve_lamports {
        warn!(
            have = lamports as f64 / 1e9,
            need = (config.trade_lamports + config.reserve_lamports) as f64 / 1e9,
            "Insufficient balance"
        );
        return Ok(false);
    }

    // Phase 2: quote2 (sequential — needs q1.out_amount)
    let q2 = jupiter::get_quote(client, mid_mint, SOL_MINT, q1.out_amount, 50, api_key).await?;
    let ms_q2 = t0.elapsed().as_millis();

    let gross      = q2.out_amount as i64 - config.trade_lamports as i64;
    // Pay 65% of gross as tip — competitive on high-contention pairs.
    // Floor from config ensures we don't bid zero on tiny spreads.
    let tip        = (config.jito_tip_lamports as i64).max(gross * 65 / 100);
    let net_profit = gross - tip;

    info!(
        pair         = format!("{} vs {}", opp.sell_pool.name, opp.buy_pool.name),
        spread_bps   = opp.spread_bps,
        gross_sol    = gross as f64 / 1e9,
        tip_lamports = tip,
        net_sol      = net_profit as f64 / 1e9,
        q1_ms        = ms_q1,
        q2_ms        = ms_q2 - ms_q1,
        "Quote check"
    );

    if net_profit <= 0 {
        info!(elapsed_ms = ms_q2, "No profit after Jito tip, skipping");
        return Ok(false);
    }

    // Phase 3: build both swap txs in parallel — blockhash already fetched in phase 1
    let (tx1, tx2) = tokio::try_join!(
        jupiter::build_swap_tx(client, &q1.raw, &pubkey_str, config.priority_fee, api_key, blockhash),
        jupiter::build_swap_tx(client, &q2.raw, &pubkey_str, config.priority_fee, api_key, blockhash),
    )?;
    let ms_build = t0.elapsed().as_millis();

    info!(
        net_sol             = net_profit as f64 / 1e9,
        tip_lamports        = tip,
        build_ms            = ms_build - ms_q2,
        total_pre_submit_ms = ms_build,
        "Submitting Jito bundle"
    );

    // Phase 4: sign + submit
    let t_submit = Instant::now();
    let (bundle_id, endpoint) = jito::send_bundle(
        client, wallet,
        &config.jito_url,
        vec![tx1, tx2],
        tip as u64,
        blockhash,
    ).await?;
    let ms_submit = t_submit.elapsed().as_millis();
    let ms_total  = t0.elapsed().as_millis();

    info!(
        bundle_id = %bundle_id,
        endpoint  = %endpoint,
        submit_ms = ms_submit,
        total_ms  = ms_total,
        "Bundle sent"
    );

    match jito::wait_for_bundle(client, &endpoint, &bundle_id, Duration::from_secs(60)).await {
        Ok(()) => {
            println!(
                "\x1b[1m\x1b[32m✓ Bundle confirmed | net_sol={:.6} | bundle={} | total_ms={}\x1b[0m",
                net_profit as f64 / 1e9, bundle_id, ms_total
            );
            Ok(true)
        }
        Err(e) if e.to_string().contains("timeout") => {
            warn!(bundle_id = %bundle_id, total_ms = ms_total,
                  "Bundle expired (outbid or stale blockhash) — no tokens lost");
            Ok(false)
        }
        Err(e) => {
            error!(bundle_id = %bundle_id, error = %e, total_ms = ms_total,
                   "Bundle failed on-chain — no tokens lost (atomic)");
            Ok(false)
        }
    }
}
