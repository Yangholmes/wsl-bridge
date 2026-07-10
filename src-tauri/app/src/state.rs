use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{env, fs};
use std::{thread, time::Duration};

use wsl_bridge_core::{EngineOptions, FirewallMode, RuleEngine};

use crate::mcp::{self, McpHttpService};

#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<RuleEngine>,
    pub mcp_service: Arc<McpHttpService>,
}

impl AppState {
    #[cfg(not(feature = "tauri"))]
    pub fn new() -> Self {
        Self::new_with_default_storage()
    }

    #[cfg(not(feature = "tauri"))]
    pub fn new_with_default_storage() -> Self {
        Self::new_with_storage_path(db_path())
    }

    pub fn new_with_storage_path(path: PathBuf) -> Self {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let options = engine_options_from_env();
        let log_dir = resolve_log_dir(&path);

        match RuleEngine::with_sqlite_and_options_and_log_dir(&path, options, Some(log_dir.clone()))
        {
            Ok(engine) => {
                let engine = Arc::new(engine);
                let mcp_service = Arc::new(McpHttpService::new(engine.clone()));
                let state = Self {
                    engine: engine.clone(),
                    mcp_service,
                };
                mcp::ensure_initialized_config(&state);
                start_topology_reconcile_loop(engine.clone());
                state
            }
            Err(err) => {
                eprintln!(
                    "failed to initialize sqlite storage at {}: {err}; fallback to memory",
                    path.display()
                );
                let engine = Arc::new(
                    RuleEngine::new_with_options_and_log_dir(options, log_dir)
                        .unwrap_or_else(|_| RuleEngine::new_with_options(options)),
                );
                let mcp_service = Arc::new(McpHttpService::new(engine.clone()));
                let state = Self {
                    engine: engine.clone(),
                    mcp_service,
                };
                mcp::ensure_initialized_config(&state);
                start_topology_reconcile_loop(engine.clone());
                state
            }
        }
    }
}

fn resolve_log_dir(db_path: &PathBuf) -> PathBuf {
    if let Ok(explicit) = env::var("WSL_BRIDGE_LOG_DIR") {
        return explicit.into();
    }

    #[cfg(windows)]
    if let Ok(program_data) = env::var("PROGRAMDATA") {
        let candidate = PathBuf::from(program_data).join("wsl-bridge").join("logs");
        if fs::create_dir_all(&candidate).is_ok() {
            return candidate;
        }
    }

    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("logs")
}

#[cfg(not(feature = "tauri"))]
fn db_path() -> std::path::PathBuf {
    default_storage_path()
}

#[cfg(not(feature = "tauri"))]
pub fn default_storage_path() -> std::path::PathBuf {
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
