use anyhow::{Context, Result};
use solana_sdk::signature::Keypair;
use std::env;

pub struct Config {
    pub rpc_url:           String,
    pub rpc_host:          String,
    pub rpc_ws:            String,
    pub min_spread_bps:    u32,
    pub trade_lamports:    u64,
    pub reserve_lamports:  u64,
    pub priority_fee:      u64,
    pub jito_tip_lamports: u64,
    /// Percentage of gross profit to pay as Jito tip (0-100). Default 65.
    /// Set higher (e.g. 90) to be more competitive at the cost of less net profit.
    pub tip_pct:           u64,
    /// Minimum net profit in lamports required to submit a bundle. Default 1.
    /// Set to a negative value (e.g. -50000) to allow small losses for testing
    /// whether bundles land at all.
    pub min_net_lamports:  i64,
    pub jito_url:          String,
    pub jupiter_api_key:   Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            rpc_url:           env::var("RPC_URL").context("RPC_URL")?,
            rpc_host:          env::var("RPC_HOST").unwrap_or_default(),
            rpc_ws:            env::var("RPC_WS_URL").context("RPC_WS_URL")?,
            min_spread_bps:    env::var("MIN_PROFIT_BPS").unwrap_or("5".into()).parse()?,
            trade_lamports:    (env::var("TRADE_SOL").unwrap_or("2".into()).parse::<f64>()? * 1e9) as u64,
            reserve_lamports:  20_000_000, // 0.02 SOL
            priority_fee:      env::var("PRIORITY_FEE").unwrap_or("200000".into()).parse()?,
            jito_tip_lamports: env::var("JITO_TIP_LAMPORTS").unwrap_or("10000".into()).parse()?,
            tip_pct:           env::var("TIP_PCT").unwrap_or("65".into()).parse()?,
            min_net_lamports:  env::var("MIN_NET_LAMPORTS").unwrap_or("1".into()).parse()?,
            jito_url:          env::var("JITO_URL")
                .unwrap_or("https://mainnet.block-engine.jito.wtf/api/v1/bundles".into()),
            jupiter_api_key:   env::var("JUPITER_API_KEY").ok(),
        })
    }
}

pub fn load_wallet() -> Result<Keypair> {
    let key_str = env::var("WALLET_PRIVATE_KEY").context("WALLET_PRIVATE_KEY")?;
    let bytes   = bs58::decode(key_str).into_vec().context("invalid base58 key")?;
    Keypair::from_bytes(&bytes).context("invalid keypair bytes")
}
