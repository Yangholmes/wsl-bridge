#![cfg_attr(all(feature = "tauri", target_os = "windows"), windows_subsystem = "windows")]

mod commands;
mod mcp;
mod runtime_status;
mod state;
mod tauri_commands;

#[cfg(not(feature = "tauri"))]
use wsl_bridge_shared::{BindMode, CreateRuleRequest, NewProxyRule, RuleType, TargetKind};
#[cfg(feature = "tauri")]
use tauri::Manager;

#[cfg(not(feature = "tauri"))]
fn main() {
    // Bootstrap mode for early backend development without full Tauri runtime.
    let app = state::AppState::new();

    if commands::list_rules(&app).is_empty() {
        let boot_rule = CreateRuleRequest {
            rule: NewProxyRule {
                name: "sample-tcp-forward".to_owned(),
                rule_type: RuleType::TcpFwd,
                listen_host: "0.0.0.0".to_owned(),
                listen_port: 18080,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port: Some(8080),
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            },
            firewall: None,
        };

        let _ = commands::create_rule(&app, boot_rule);
    }
    let result = commands::apply_rules(&app);

    println!(
        "wsl-bridge app bootstrap ready: applied={}, failed={}, db={}",
        result.applied,
        result.failed.len(),
        app.engine
            .sqlite_path()
            .map(|v| v.display().to_string())
            .unwrap_or_else(|| "memory".to_owned())
    );
}

#[cfg(feature = "tauri")]
fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let db_path = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|err| {
                    eprintln!(
                        "failed to resolve app data dir: {err}; fallback to current directory"
                    );
                    std::env::current_dir()
                        .unwrap_or_else(|_| ".".into())
                        .join("data")
                })
                .join("state.db");
            let state = state::AppState::new_with_storage_path(db_path);
            let result = commands::apply_rules(&state);
            println!(
                "wsl-bridge app bootstrap ready: applied={}, failed={}",
                result.applied,
                result.failed.len()
            );
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            tauri_commands::get_app_runtime_status,
            tauri_commands::scan_topology,
            tauri_commands::debug_hyperv_probe,
            tauri_commands::list_rules,
            tauri_commands::create_rule,
            tauri_commands::update_rule,
            tauri_commands::delete_rule,
            tauri_commands::enable_rule,
            tauri_commands::apply_rules,
            tauri_commands::stop_rules,
            tauri_commands::get_runtime_status,
            tauri_commands::tail_logs,
            tauri_commands::query_logs,
            tauri_commands::get_rule_log_stats,
            tauri_commands::get_mcp_server_status,
            tauri_commands::update_mcp_server_config
        ])
        .run(tauri::generate_context!())
        .expect("failed to run tauri app");
}
