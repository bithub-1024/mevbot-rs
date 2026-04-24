use anyhow::{bail, Result};
use parking_lot::Mutex;
use reqwest::Client;
use serde_json::{json, Value};
use solana_sdk::{hash::Hash, pubkey::Pubkey};
use std::{
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(5);

fn apply_host(req: reqwest::RequestBuilder, rpc_host: &str) -> reqwest::RequestBuilder {
    if rpc_host.is_empty() { req } else { req.header("host", rpc_host) }
}

pub async fn get_balance(client: &Client, url: &str, rpc_host: &str, pubkey: &Pubkey) -> Result<u64> {
    let req = client
        .post(url)
        .json(&json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "getBalance",
            "params": [pubkey.to_string(), {"commitment": "processed"}]
        }))
        .timeout(TIMEOUT);
    let resp: Value = apply_host(req, rpc_host).send().await?.json().await?;
    if let Some(err) = resp.get("error") {
        bail!("getBalance: {err}");
    }
    Ok(resp["result"]["value"].as_u64().unwrap_or(0))
}

pub async fn get_latest_blockhash(client: &Client, url: &str, rpc_host: &str) -> Result<Hash> {
    let req = client
        .post(url)
        .json(&json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "getLatestBlockhash",
            "params": [{"commitment": "processed"}]
        }))
        .timeout(TIMEOUT);
    let resp: Value = apply_host(req, rpc_host).send().await?.json().await?;
    if let Some(err) = resp.get("error") {
        bail!("getLatestBlockhash: {err}");
    }
    let hash_str = resp["result"]["value"]["blockhash"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing blockhash"))?;
    Hash::from_str(hash_str).map_err(|e| anyhow::anyhow!("invalid hash: {e}"))
}

/// Cached blockhash refreshed in background every ~400ms.
/// Executor reads from cache; no RPC call on the critical path.
pub struct BlockhashCache {
    inner: Mutex<(Hash, Instant)>,
}

impl BlockhashCache {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new((Hash::default(), Instant::now() - Duration::from_secs(60))),
        })
    }

    /// Returns cached hash if it is less than 600ms old, else None.
    pub fn get_fresh(&self) -> Option<Hash> {
        let (hash, ts) = *self.inner.lock();
        if ts.elapsed() < Duration::from_millis(600) { Some(hash) } else { None }
    }

    pub fn update(&self, hash: Hash) {
        *self.inner.lock() = (hash, Instant::now());
    }
}

/// Background task: refresh blockhash every 400ms and store in cache.
pub async fn run_blockhash_refresher(
    client:   Arc<Client>,
    url:      String,
    rpc_host: String,
    cache:    Arc<BlockhashCache>,
) {
    loop {
        if let Ok(hash) = get_latest_blockhash(&client, &url, &rpc_host).await {
            cache.update(hash);
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
}
