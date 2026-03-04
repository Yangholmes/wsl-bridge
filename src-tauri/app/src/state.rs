use std::sync::Arc;
use std::{env, fs};
use std::{thread, time::Duration};

use wsl_bridge_core::{EngineOptions, FirewallMode, RuleEngine};

#[derive(Clone, Default)]
pub struct AppState {
    pub engine: Arc<RuleEngine>,
}

impl AppState {
    pub fn new() -> Self {
        Self::new_with_default_storage()
    }

    pub fn new_with_default_storage() -> Self {
        let path = db_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let options = engine_options_from_env();

        match RuleEngine::with_sqlite_and_options(&path, options) {
            Ok(engine) => {
                let engine = Arc::new(engine);
                start_topology_reconcile_loop(engine.clone());
                Self { engine }
            }
            Err(err) => {
                eprintln!(
                    "failed to initialize sqlite storage at {}: {err}; fallback to memory",
                    path.display()
                );
                let engine = Arc::new(RuleEngine::new_with_options(options));
                start_topology_reconcile_loop(engine.clone());
                Self { engine }
            }
        }
    }
}

fn db_path() -> std::path::PathBuf {
    if let Ok(explicit) = env::var("WSL_BRIDGE_DB_PATH") {
        return explicit.into();
    }

    env::current_dir()
        .unwrap_or_else(|_| ".".into())
        .join("data")
        .join("state.db")
}

fn engine_options_from_env() -> EngineOptions {
    let mode = env::var("WSL_BRIDGE_FIREWALL_MODE")
        .ok()
        .map(|value| FirewallMode::from_env_value(&value))
        .unwrap_or(FirewallMode::BestEffort);
    EngineOptions {
        firewall_mode: mode,
    }
}

fn start_topology_reconcile_loop(engine: Arc<RuleEngine>) {
    let interval_secs = env::var("WSL_BRIDGE_TOPOLOGY_POLL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(8);

    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(interval_secs));
        if let Some(result) = engine.reconcile_runtime_topology() {
            eprintln!(
                "topology changed: rules reapplied, applied={}, failed={}",
                result.applied,
                result.failed.len()
            );
        }
    });
}
