use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Accumulates latency samples per named key.
/// Call `record` from any task; call `print_report` on shutdown.
pub struct LatencyStats {
    samples: Mutex<BTreeMap<&'static str, Vec<u64>>>,
}

impl LatencyStats {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            samples: Mutex::new(BTreeMap::new()),
        })
    }

    pub fn record(&self, key: &'static str, ms: u64) {
        self.samples.lock().entry(key).or_default().push(ms);
    }

    pub fn print_report(&self) {
        let mut guard = self.samples.lock();
        if guard.is_empty() {
            println!("\n[Stats] No latency samples collected.");
            return;
        }
        println!("\n╔══ Latency report (ms) ══════════════════════════════════════════════╗");
        for (key, samples) in guard.iter_mut() {
            if samples.is_empty() {
                continue;
            }
            samples.sort_unstable();
            let n     = samples.len();
            let med   = samples[n / 2];
            let p75   = samples[n * 75 / 100];
            let p95   = samples[(n * 95 / 100).min(n - 1)];
            let p99   = samples[(n * 99 / 100).min(n - 1)];
            let mean  = samples.iter().sum::<u64>() / n as u64;
            let min   = samples[0];
            let max   = *samples.last().unwrap();
            println!(
                "║  {:<12}  n={:<5}  min={:>4}  med={:>4}  mean={:>4}  p75={:>4}  p95={:>4}  p99={:>4}  max={:>5}",
                key, n, min, med, mean, p75, p95, p99, max
            );
        }
        println!("╚══════════════════════════════════════════════════════════════════════╝");
    }
}
