pub const SOL:  &str = "So11111111111111111111111111111111111111112";
pub const USDC: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
pub const BONK: &str = "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263";
pub const WIF:  &str = "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm";

#[derive(Clone, Debug)]
pub enum PriceMethod {
    /// Read SPL vault balances; vaultA / vaultB addresses required
    Vaults {
        vault_a: &'static str,
        vault_b: &'static str,
    },
    /// Read sqrtPriceX64 u128-LE at a fixed byte offset in the pool account
    SqrtPrice {
        pool_address:      &'static str,
        sqrt_price_offset: usize,
    },
}

#[derive(Clone, Debug)]
pub struct Pool {
    pub id:          &'static str,
    pub name:        &'static str,
    pub jupiter_dex: &'static str,
    pub token_a:     &'static str,
    pub decimals_a:  i32,
    pub token_b:     &'static str,
    pub decimals_b:  i32,
    pub fee_bps:     u32,
    pub method:      PriceMethod,
}

pub static POOLS: &[Pool] = &[
    // ── SOL/USDC ─────────────────────────────────────────────────────────────
    Pool {
        id: "58oQChx4yWmvKdwLLZzBi4ChoCc2fqCUWBkwMihLYQo2",
        name: "Raydium AMM SOL/USDC", jupiter_dex: "Raydium",
        token_a: SOL, decimals_a: 9, token_b: USDC, decimals_b: 6, fee_bps: 25,
        method: PriceMethod::Vaults {
            vault_a: "DQyrAcCrDXQ7NeoqGgDCZwBvWDcYmFCjSb9JtteuvPpz",
            vault_b: "HLmqeL62xR1QoZ1HKKbXRrdN1p3phKpxRMb2VVopvBBz",
        },
    },
    Pool {
        id: "HJPjoWUrhoZzkNfRpHuieeFk9WcZWjwy6PBjZ81ngndJ",
        name: "Orca Whirlpool SOL/USDC", jupiter_dex: "Orca V2",
        token_a: SOL, decimals_a: 9, token_b: USDC, decimals_b: 6, fee_bps: 5,
        method: PriceMethod::SqrtPrice {
            pool_address:      "HJPjoWUrhoZzkNfRpHuieeFk9WcZWjwy6PBjZ81ngndJ",
            sqrt_price_offset: 65,
        },
    },
    Pool {
        id: "CYbD9RaToYMtWKA7QZyoLahnHdWq553Vm62Lh6qWtuxq",
        name: "Raydium CLMM SOL/USDC", jupiter_dex: "Raydium CLMM",
        token_a: SOL, decimals_a: 9, token_b: USDC, decimals_b: 6, fee_bps: 1,
        method: PriceMethod::SqrtPrice {
            pool_address:      "CYbD9RaToYMtWKA7QZyoLahnHdWq553Vm62Lh6qWtuxq",
            sqrt_price_offset: 253,
        },
    },

    // ── BONK/SOL ─────────────────────────────────────────────────────────────
    Pool {
        id: "HVNwzt7Pxfu76KHCMQPTLuTCLTm6WnQ1esLv4eizseSv",
        name: "Raydium AMM BONK/SOL", jupiter_dex: "Raydium",
        token_a: BONK, decimals_a: 5, token_b: SOL, decimals_b: 9, fee_bps: 25,
        method: PriceMethod::Vaults {
            vault_a: "7KFdXKA5WkZBspxwqd9kSrDGTg9WhiX5TptUB3yRwEaE",
            vault_b: "GehmCo7EgzkB4xxyviW6xdUhm1Ed2nN98QcfcRWQCfA9",
        },
    },
    Pool {
        // Verified on-chain: token_0=SOL, token_1=BONK
        id: "GtKKKs3yaPdHbQd2aZS4SfWhy8zQ988BJGnKNndLxYsN",
        name: "Raydium CLMM BONK/SOL", jupiter_dex: "Raydium CLMM",
        token_a: SOL, decimals_a: 9, token_b: BONK, decimals_b: 5, fee_bps: 1,
        method: PriceMethod::SqrtPrice {
            pool_address:      "GtKKKs3yaPdHbQd2aZS4SfWhy8zQ988BJGnKNndLxYsN",
            sqrt_price_offset: 253,
        },
    },

    // ── WIF/SOL ──────────────────────────────────────────────────────────────
    Pool {
        id: "EP2ib6dYdEeqD8MfE2ezHCxX3kP3K2eLKkirfPm5eyMx",
        name: "Raydium AMM WIF/SOL", jupiter_dex: "Raydium",
        token_a: WIF, decimals_a: 6, token_b: SOL, decimals_b: 9, fee_bps: 25,
        method: PriceMethod::Vaults {
            vault_a: "7UYZ4vX13mmGiopayLZAduo8aie77yZ3o8FMzTeAX8uJ",
            vault_b: "7e9ExBAvDvuJP3GE6eKL5aSMi4RfXv3LkQaiNZBPmffR",
        },
    },
    Pool {
        // Verified on-chain: token_0=SOL, token_1=WIF
        id: "4mMDQ5kG9fFrBSQeedErsUoTBhY5KKnsKWGvenXRTwSy",
        name: "Raydium CLMM WIF/SOL", jupiter_dex: "Raydium CLMM",
        token_a: SOL, decimals_a: 9, token_b: WIF, decimals_b: 6, fee_bps: 1,
        method: PriceMethod::SqrtPrice {
            pool_address:      "4mMDQ5kG9fFrBSQeedErsUoTBhY5KKnsKWGvenXRTwSy",
            sqrt_price_offset: 253,
        },
    },
];
