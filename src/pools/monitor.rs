use std::{collections::HashMap, str::FromStr, time::Duration};

use anyhow::Result;
use base64::Engine;
use reqwest::Client;
use serde_json::{json, Value};
use solana_sdk::pubkey::Pubkey;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::pools::registry::{Pool, PriceMethod, POOLS};

#[derive(Debug, Clone)]
pub struct PriceEvent {
    pub pool:  &'static Pool,
    /// price = tokenB per tokenA (human decimals)
    pub price: f64,
}

fn parse_spl_amount(data: &[u8]) -> Option<u64> {
    data.get(64..72).map(|b| u64::from_le_bytes(b.try_into().unwrap()))
}

fn parse_sqrt_price(data: &[u8], offset: usize, dec_a: i32, dec_b: i32) -> Option<f64> {
    let lo = u64::from_le_bytes(data.get(offset..offset + 8)?.try_into().ok()?);
    let hi = u64::from_le_bytes(data.get(offset + 8..offset + 16)?.try_into().ok()?);
    let sqrt = lo as f64 + hi as f64 * (u64::MAX as f64 + 1.0);
    let price = (sqrt / 2_f64.powi(64)).powi(2) * 10_f64.powi(dec_a - dec_b);
    if price > 0.0 { Some(price) } else { None }
}

/// Build the ordered account list and role map once.
struct AccountLayout {
    keys:  Vec<String>,
    roles: Vec<Role>,
}

enum Role {
    VaultA(usize),
    VaultB(usize),
    SqrtPrice(usize),
}

impl AccountLayout {
    fn build() -> Self {
        let mut keys  = Vec::new();
        let mut roles = Vec::new();
        for (i, pool) in POOLS.iter().enumerate() {
            match &pool.method {
                PriceMethod::Vaults { vault_a, vault_b } => {
                    keys.push(vault_a.to_string());  roles.push(Role::VaultA(i));
                    keys.push(vault_b.to_string());  roles.push(Role::VaultB(i));
                }
                PriceMethod::SqrtPrice { pool_address, .. } => {
                    keys.push(pool_address.to_string());
                    roles.push(Role::SqrtPrice(i));
                }
            }
        }
        Self { keys, roles }
    }
}

/// Seed initial prices — same logic as first poll cycle, exposed for main.rs.
pub async fn seed_initial_prices(
    rpc_url:  &str,
    rpc_host: &str,
    client:   &Client,
    tx:       &mpsc::Sender<PriceEvent>,
) -> Result<()> {
    let layout = AccountLayout::build();
    let _ = poll_once(rpc_url, rpc_host, client, &layout, tx).await;
    Ok(())
}

/// Decode one account's base64 data from the RPC response value.
fn decode_account(value: &Value) -> Option<Vec<u8>> {
    let b64 = value["data"].as_array()?.first()?.as_str()?;
    base64::engine::general_purpose::STANDARD.decode(b64).ok()
}

/// One getMultipleAccounts poll → emit PriceEvents for changed/new prices.
async fn poll_once(
    rpc_url:  &str,
    rpc_host: &str,
    client:   &Client,
    layout:   &AccountLayout,
    tx:       &mpsc::Sender<PriceEvent>,
) -> anyhow::Result<()> {
    let req = client
        .post(rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getMultipleAccounts",
            "params": [layout.keys, {"encoding": "base64", "commitment": "processed"}]
        }))
        .timeout(Duration::from_secs(5));
    let req = if rpc_host.is_empty() { req } else { req.header("host", rpc_host) };
    let resp: Value = req.send().await?.json().await?;

    let accounts = match resp["result"]["value"].as_array() {
        Some(a) => a,
        None    => {
            warn!("getMultipleAccounts: unexpected response");
            return Ok(());
        }
    };

    // Collect vault amounts per pool before computing price
    let mut vault_bals: HashMap<usize, (Option<u64>, Option<u64>)> = HashMap::new();

    for (account_val, role) in accounts.iter().zip(layout.roles.iter()) {
        let data = match decode_account(account_val) {
            Some(d) => d,
            None    => continue,
        };
        match role {
            Role::VaultA(idx) => {
                if let Some(amt) = parse_spl_amount(&data) {
                    vault_bals.entry(*idx).or_default().0 = Some(amt);
                }
            }
            Role::VaultB(idx) => {
                if let Some(amt) = parse_spl_amount(&data) {
                    vault_bals.entry(*idx).or_default().1 = Some(amt);
                }
            }
            Role::SqrtPrice(idx) => {
                let pool = &POOLS[*idx];
                if let PriceMethod::SqrtPrice { sqrt_price_offset, .. } = pool.method {
                    if let Some(price) = parse_sqrt_price(
                        &data, sqrt_price_offset, pool.decimals_a, pool.decimals_b,
                    ) {
                        let _ = tx.send(PriceEvent { pool, price }).await;
                    }
                }
            }
        }
    }

    for (idx, (ba_opt, bb_opt)) in vault_bals {
        if let (Some(ba), Some(bb)) = (ba_opt, bb_opt) {
            if ba > 0 && bb > 0 {
                let pool  = &POOLS[idx];
                let price = (bb as f64 / 10_f64.powi(pool.decimals_b))
                          / (ba as f64 / 10_f64.powi(pool.decimals_a));
                let _ = tx.send(PriceEvent { pool, price }).await;
            }
        }
    }
    Ok(())
}

/// Poll all pool accounts every POLL_MS milliseconds.
/// Uses the same reqwest::Client as the rest of the bot (danger_accept_invalid_certs already set).
pub async fn run(rpc_url: &str, rpc_host: &str, client: Client, tx: mpsc::Sender<PriceEvent>) -> Result<()> {
    const POLL_MS: u64 = 200;

    let layout = AccountLayout::build();
    info!("HTTP polling {} accounts every {}ms", layout.keys.len(), POLL_MS);

    loop {
        if let Err(e) = poll_once(rpc_url, rpc_host, &client, &layout, &tx).await {
            warn!("Poll error: {e}");
        }
        tokio::time::sleep(Duration::from_millis(POLL_MS)).await;
    }
}
