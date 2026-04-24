mod config;
mod pools;
mod arbitrage;
mod utils;

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::Result;
use parking_lot::Mutex;
use reqwest::Client;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use solana_sdk::signer::Signer;
use tracing_subscriber::EnvFilter;

use arbitrage::{detector::Detector, executor};
use config::{load_wallet, Config};
use pools::monitor;
use utils::rpc::{self, BlockhashCache};

struct ExecGuard {
    executing: bool,
    last_exec: Instant,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("mevbot=info"))
        )
        .init();

    let config = Arc::new(Config::from_env()?);
    let wallet = Arc::new(load_wallet()?);

    let client = Arc::new(
        Client::builder()
            .timeout(Duration::from_secs(15))
            .tcp_nodelay(true)
            .pool_max_idle_per_host(20)
            .pool_idle_timeout(Duration::from_secs(90))
            .danger_accept_invalid_certs(true)
            .build()?
    );

    let balance = rpc::get_balance(&client, &config.rpc_url, &config.rpc_host, &wallet.pubkey()).await?;
    info!(
        wallet     = %wallet.pubkey(),
        sol        = balance as f64 / 1e9,
        trade_sol  = config.trade_lamports as f64 / 1e9,
        min_spread = config.min_spread_bps,
        "Bot started"
    );

    let (price_tx, mut price_rx) = mpsc::channel(2048);

    // Seed initial prices immediately via batched getMultipleAccounts
    if let Err(e) = monitor::seed_initial_prices(&config.rpc_url, &config.rpc_host, &client, &price_tx).await {
        warn!("Initial price seed failed (non-fatal): {e}");
    }

    // HTTP polling monitor
    {
        let rpc_url  = config.rpc_url.clone();
        let rpc_host = config.rpc_host.clone();
        let client2  = (*client).clone();
        let price_tx = price_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = monitor::run(&rpc_url, &rpc_host, client2, price_tx).await {
                error!("Monitor exited: {e}");
            }
        });
    }

    // Background blockhash refresher — keeps cache warm every 400ms
    let bh_cache = BlockhashCache::new();
    {
        let client2  = Arc::clone(&client);
        let url      = config.rpc_url.clone();
        let rpc_host = config.rpc_host.clone();
        let cache    = Arc::clone(&bh_cache);
        tokio::spawn(rpc::run_blockhash_refresher(client2, url, rpc_host, cache));
    }

    let exec_guard = Arc::new(Mutex::new(ExecGuard {
        executing: false,
        last_exec: Instant::now() - Duration::from_secs(60),
    }));
    let detector = Arc::new(Mutex::new(Detector::new()));

    let stat_executed = Arc::new(AtomicU64::new(0));
    let stat_failed   = Arc::new(AtomicU64::new(0));

    let mut opps       = 0u64;
    let mut cooldown   = 0u64;
    let mut checked    = 0u64;
    let mut last_stats = Instant::now();

    const COOLDOWN: Duration = Duration::from_millis(500);

    while let Some(event) = price_rx.recv().await {
        if last_stats.elapsed() >= Duration::from_secs(60) {
            info!(
                opps     = opps,
                cooldown = cooldown,
                checked  = checked,
                executed = stat_executed.load(Ordering::Relaxed),
                failed   = stat_failed.load(Ordering::Relaxed),
                "Stats"
            );
            detector.lock().log_prices();
            last_stats = Instant::now();
        }

        let opp = {
            let mut det = detector.lock();
            det.on_price(&event)
        };

        let opp = match opp {
            None => continue,
            Some(o) if o.spread_bps < config.min_spread_bps => continue,
            Some(o) => o,
        };

        opps += 1;

        {
            let guard = exec_guard.lock();
            if guard.executing || guard.last_exec.elapsed() < COOLDOWN {
                cooldown += 1;
                continue;
            }
        }

        checked += 1;
        info!(
            sell = opp.sell_pool.name,
            buy  = opp.buy_pool.name,
            bps  = opp.spread_bps,
            "Opportunity → checking Jupiter"
        );

        {
            let config   = Arc::clone(&config);
            let wallet   = Arc::clone(&wallet);
            let client   = Arc::clone(&client);
            let guard    = Arc::clone(&exec_guard);
            let executed = Arc::clone(&stat_executed);
            let failed   = Arc::clone(&stat_failed);
            let bh_cache = Arc::clone(&bh_cache);

            exec_guard.lock().executing = true;

            tokio::spawn(async move {
                let result = executor::execute(&client, &wallet, &config, &opp, &bh_cache).await;
                {
                    let mut g = guard.lock();
                    g.executing = false;
                    g.last_exec = Instant::now();
                }
                match result {
                    Ok(true)  => { executed.fetch_add(1, Ordering::Relaxed); }
                    Ok(false) => {}
                    Err(e)    => {
                        failed.fetch_add(1, Ordering::Relaxed);
                        error!("Execute error: {e}");
                    }
                }
            });
        }
    }

    Ok(())
}
