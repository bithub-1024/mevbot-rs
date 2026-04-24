use anyhow::{bail, Result};
use base64::Engine;
use futures_util::future::select_ok;
use rand::seq::SliceRandom;
use reqwest::Client;
use serde_json::{json, Value};
use solana_sdk::{
    hash::Hash,
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    system_instruction,
    transaction::VersionedTransaction,
    message::{v0, VersionedMessage},
};
use std::{pin::Pin, str::FromStr, time::Duration};

const JITO_ENDPOINTS: &[&str] = &[
    "https://mainnet.block-engine.jito.wtf/api/v1/bundles",
    "https://ny.mainnet.block-engine.jito.wtf/api/v1/bundles",
    "https://amsterdam.mainnet.block-engine.jito.wtf/api/v1/bundles",
    "https://frankfurt.block-engine.jito.wtf/api/v1/bundles",
    "https://tokyo.mainnet.block-engine.jito.wtf/api/v1/bundles",
];

const TIP_ACCOUNTS: &[&str] = &[
    "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
    "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
    "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
    "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
    "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
    "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
];

fn build_tip_tx(wallet: &Keypair, blockhash: Hash, tip_lamports: u64) -> Result<VersionedTransaction> {
    let tip_account = Pubkey::from_str(
        TIP_ACCOUNTS.choose(&mut rand::thread_rng()).unwrap()
    )?;
    let ix  = system_instruction::transfer(&wallet.pubkey(), &tip_account, tip_lamports);
    let msg = v0::Message::try_compile(&wallet.pubkey(), &[ix], &[], blockhash)?;
    let tx  = VersionedTransaction::try_new(VersionedMessage::V0(msg), &[wallet])?;
    Ok(tx)
}

fn encode_tx(tx: &VersionedTransaction) -> Result<String> {
    let bytes = bincode::serialize(tx)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// Sign all txs with the provided blockhash, append tip tx, then submit the bundle
/// to all Jito endpoints in parallel — returns on first success.
pub async fn send_bundle(
    client:       &Client,
    wallet:       &Keypair,
    jito_url:     &str,
    mut txs:      Vec<VersionedTransaction>,
    tip_lamports: u64,
    blockhash:    Hash,
) -> Result<(String, String)> {
    // Re-sign all transactions with provided blockhash
    for tx in &mut txs {
        match &mut tx.message {
            VersionedMessage::V0(m)     => m.recent_blockhash = blockhash,
            VersionedMessage::Legacy(m) => m.recent_blockhash = blockhash,
        }
        let msg_bytes = tx.message.serialize();
        let sig = wallet.sign_message(&msg_bytes);
        if tx.signatures.is_empty() {
            tx.signatures.push(sig);
        } else {
            tx.signatures[0] = sig;
        }
    }

    let tip_tx = build_tip_tx(wallet, blockhash, tip_lamports)?;
    txs.push(tip_tx);

    let encoded: Vec<String> = txs.iter().map(encode_tx).collect::<Result<_>>()?;

    let body = json!({
        "jsonrpc": "2.0",
        "id":      1,
        "method":  "sendBundle",
        "params":  [encoded, { "encoding": "base64" }],
    });

    // Deduplicate endpoints, preferred URL first
    let all_endpoints: Vec<&str> = std::iter::once(jito_url)
        .chain(JITO_ENDPOINTS.iter().copied().filter(|&u| u != jito_url))
        .collect();

    // Fire all endpoints simultaneously — first success wins
    type Fut = Pin<Box<dyn std::future::Future<Output = Result<(String, String)>> + Send>>;
    let futs: Vec<Fut> = all_endpoints
        .iter()
        .map(|&url| -> Fut {
            let url = url.to_string();
            let c   = client.clone();
            let b   = body.clone();
            Box::pin(async move {
                let r = c.post(&url).json(&b).timeout(Duration::from_secs(5)).send().await?;
                let data: Value = r.json().await?;
                if let Some(err) = data.get("error") {
                    bail!("Jito error: {err}");
                }
                let bundle_id = data["result"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("no bundle id"))?
                    .to_string();
                Ok((bundle_id, url))
            })
        })
        .collect();

    let ((bundle_id, endpoint), _) = select_ok(futs).await
        .map_err(|e| anyhow::anyhow!("All Jito endpoints failed: {e}"))?;

    Ok((bundle_id, endpoint))
}

pub async fn wait_for_bundle(
    client:    &Client,
    endpoint:  &str,
    bundle_id: &str,
    timeout:   Duration,
) -> Result<()> {
    let body = json!({
        "jsonrpc": "2.0",
        "id":      1,
        "method":  "getBundleStatuses",
        "params":  [[bundle_id]],
    });

    // Use the generic status endpoint (not the regional one which may differ)
    let status_url = if endpoint.contains("mainnet.block-engine.jito.wtf") {
        endpoint.to_string()
    } else {
        "https://mainnet.block-engine.jito.wtf/api/v1/bundles".to_string()
    };

    let deadline = tokio::time::Instant::now() + timeout;

    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(2)).await;

        let resp = client
            .post(&status_url)
            .json(&body)
            .timeout(Duration::from_secs(8))
            .send()
            .await;
        let data: Value = match resp {
            Err(_) => continue,
            Ok(r)  => r.json().await.unwrap_or(Value::Null),
        };

        let status = &data["result"]["value"][0];
        if status.is_null() { continue; }

        if !status["err"].is_null() {
            bail!("Bundle failed on-chain: {}", status["err"]);
        }
        let s = status["confirmation_status"].as_str().unwrap_or("");
        if s == "confirmed" || s == "finalized" {
            return Ok(());
        }
    }

    bail!("Bundle confirmation timeout")
}
