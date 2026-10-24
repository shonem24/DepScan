use std::time::Instant;

use anyhow::Result;

pub struct Profiler {
    started_at: Instant,
    phases: Vec<Phase>,
}

struct Phase {
    name: &'static str,
    ms: f64,
}

impl Profiler {
    pub fn start() -> Self {
        Self {
            started_at: Instant::now(),
            phases: Vec::new(),
        }
    }

    pub fn measure<T>(&mut self, name: &'static str, f: impl FnOnce() -> Result<T>) -> Result<T> {
        let started_at = Instant::now();
        let result = f();
        self.phases.push(Phase {
            name,
            ms: started_at.elapsed().as_secs_f64() * 1000.0,
        });
        result
    }

    pub fn record_zero(&mut self, name: &'static str) {
        self.phases.push(Phase { name, ms: 0.0 });
    }

    pub fn print(&self, title: &str) {
        eprintln!("\n── {title} ─────────────────────────────────────────────────");
        eprintln!("{:<30} {:>10}", "phase", "ms");
        eprintln!("{}", "─".repeat(42));
        for phase in &self.phases {
            eprintln!("{:<30} {:>10.1}", phase.name, phase.ms);
        }
        eprintln!("{}", "─".repeat(42));
        eprintln!(
            "{:<30} {:>10.1}",
            "total",
            self.started_at.elapsed().as_secs_f64() * 1000.0
        );
    }
}
