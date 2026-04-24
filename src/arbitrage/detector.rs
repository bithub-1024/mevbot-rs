use std::collections::HashMap;

use crate::pools::{
    monitor::PriceEvent,
    registry::{Pool, POOLS},
};

#[derive(Debug, Clone)]
pub struct Opportunity {
    pub buy_pool:   &'static Pool,
    pub sell_pool:  &'static Pool,
    pub spread_bps: u32,
}

pub struct Detector {
    /// pool.id → raw stored price (tokenB per tokenA)
    prices: HashMap<&'static str, f64>,
}

impl Detector {
    pub fn new() -> Self {
        Self { prices: HashMap::new() }
    }

    pub fn on_price(&mut self, ev: &PriceEvent) -> Option<Opportunity> {
        self.prices.insert(ev.pool.id, ev.price);
        self.scan(ev.pool.token_a, ev.pool.token_b)
    }

    fn scan(&self, mint_a: &str, mint_b: &str) -> Option<Opportunity> {
        let pools: Vec<&Pool> = POOLS
            .iter()
            .filter(|p| {
                (p.token_a == mint_a && p.token_b == mint_b)
                    || (p.token_a == mint_b && p.token_b == mint_a)
            })
            .filter(|p| self.prices.contains_key(p.id))
            .collect();

        if pools.len() < 2 {
            return None;
        }

        let mut best: Option<Opportunity> = None;

        for sell in &pools {
            for buy in &pools {
                if sell.id == buy.id {
                    continue;
                }
                let sell_price = self.normalized(*sell, mint_a);
                let buy_price  = self.normalized(*buy,  mint_a);
                if sell_price <= buy_price || buy_price <= 0.0 {
                    continue;
                }
                let spread_bps = ((sell_price - buy_price) / buy_price * 10_000.0) as u32;
                if best.as_ref().map_or(true, |b| spread_bps > b.spread_bps) {
                    best = Some(Opportunity { buy_pool: buy, sell_pool: sell, spread_bps });
                }
            }
        }

        best
    }

    /// Normalize stored price so it's always expressed as tokenB per mintA,
    /// regardless of how the pool stores token_a/token_b.
    fn normalized(&self, pool: &Pool, mint_a: &str) -> f64 {
        let p = self.prices.get(pool.id).copied().unwrap_or(0.0);
        if pool.token_a == mint_a { p } else if p > 0.0 { 1.0 / p } else { 0.0 }
    }

    pub fn log_prices(&self) {
        let mut seen = std::collections::HashSet::new();
        let mut lines = Vec::new();

        for pool in POOLS {
            let pair_key = {
                let mut k = [pool.token_a, pool.token_b];
                k.sort();
                k.join(":")
            };
            if seen.contains(&pair_key) {
                continue;
            }
            seen.insert(pair_key.clone());

            let pair_pools: Vec<&Pool> = POOLS
                .iter()
                .filter(|p| {
                    let mut k = [p.token_a, p.token_b];
                    k.sort();
                    k.join(":") == pair_key
                })
                .collect();

            let mint_a = pair_key.split(':').next().unwrap();
            let parts: Vec<String> = pair_pools
                .iter()
                .map(|p| {
                    let price = self.normalized(p, mint_a);
                    let fmt = if price == 0.0 {
                        "n/a".to_string()
                    } else if price < 0.001 {
                        format!("{:.3e}", price)
                    } else {
                        format!("{:.6}", price)
                    };
                    format!("{}: {}", p.name, fmt)
                })
                .collect();
            lines.push(parts.join("  |  "));
        }

        tracing::info!("Prices:\n  {}", lines.join("\n  "));
    }
}
