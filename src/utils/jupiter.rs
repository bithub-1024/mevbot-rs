use anyhow::{bail, Result};
use base64::Engine;
use reqwest::Client;
use serde_json::{json, Value};
use solana_sdk::transaction::VersionedTransaction;
use std::time::Duration;

const BASE: &str = "https://api.jup.ag/swap/v1";

#[derive(Debug, Clone)]
pub struct Quote {
    pub in_amount:  u64,
    pub out_amount: u64,
    pub raw:        Value,
}

fn auth_header(key: Option<&str>) -> Option<(&'static str, String)> {
    key.map(|k| ("Authorization", format!("Bearer {k}")))
}

pub async fn get_quote(
    client:       &Client,
    input_mint:   &str,
    output_mint:  &str,
    amount:       u64,
    slippage_bps: u32,
    api_key:      Option<&str>,
) -> Result<Quote> {
    let url = format!(
        "{BASE}/quote?inputMint={input_mint}&outputMint={output_mint}&amount={amount}&slippageBps={slippage_bps}"
    );

    for attempt in 0..4u32 {
        let mut req = client.get(&url).timeout(Duration::from_secs(6));
        if let Some((k, v)) = auth_header(api_key) { req = req.header(k, v); }
        match req.send().await {
            Err(e) => {
                if attempt == 3 { bail!("Jupiter quote error: {e}"); }
                tokio::time::sleep(Duration::from_millis(100 * 2u64.pow(attempt))).await;
            }
            Ok(r) if r.status() == 429 => {
                if attempt == 3 { bail!("Jupiter 429 max retries"); }
                tokio::time::sleep(Duration::from_millis(100 * 2u64.pow(attempt))).await;
            }
            Ok(r) => {
                let data: Value = r.json().await?;
                if let Some(err) = data.get("error") {
                    bail!("Jupiter quote: {err}");
                }
                let in_amount  = data["inAmount"].as_str().unwrap_or("0").parse()?;
                let out_amount = data["outAmount"].as_str().unwrap_or("0").parse()?;
                return Ok(Quote { in_amount, out_amount, raw: data });
            }
        }
    }
    bail!("Jupiter quote exhausted retries")
}

pub async fn build_swap_tx(
    client:       &Client,
    quote:        &Value,
    user_pubkey:  &str,
    priority_fee: u64,
    api_key:      Option<&str>,
) -> Result<VersionedTransaction> {
    let body = json!({
        "quoteResponse":             quote,
        "userPublicKey":             user_pubkey,
        "wrapAndUnwrapSol":          true,
        "dynamicComputeUnitLimit":   true,
        "prioritizationFeeLamports": priority_fee,
        // skip on-chain account checks — saves 200-500ms per call
        "skipUserAccountsRpcCalls":  true,
    });

    for attempt in 0..4u32 {
        let mut req = client
            .post(format!("{BASE}/swap"))
            .json(&body)
            .timeout(Duration::from_secs(8));
        if let Some((k, v)) = auth_header(api_key) { req = req.header(k, v); }
        match req.send().await {
            Err(e) => {
                if attempt == 3 { bail!("Jupiter swap error: {e}"); }
                tokio::time::sleep(Duration::from_millis(100 * 2u64.pow(attempt))).await;
            }
            Ok(r) if r.status() == 429 => {
                if attempt == 3 { bail!("Jupiter swap 429"); }
                tokio::time::sleep(Duration::from_millis(100 * 2u64.pow(attempt))).await;
            }
            Ok(r) => {
                let data: Value = r.json().await?;
                if let Some(err) = data.get("error") {
                    bail!("Jupiter swap: {err}");
                }
                let b64 = data["swapTransaction"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("no swapTransaction"))?;
                let bytes = base64::engine::general_purpose::STANDARD.decode(b64)?;
                let tx: VersionedTransaction = bincode::deserialize(&bytes)
                    .map_err(|e| anyhow::anyhow!("deserialize tx: {e}"))?;
                return Ok(tx);
            }
        }
    }
    bail!("Jupiter swap exhausted retries")
}
