mod commands;
mod state;
mod tauri_commands;

#[cfg(not(feature = "tauri"))]
use wsl_bridge_shared::{BindMode, CreateRuleRequest, NewProxyRule, RuleType, TargetKind};

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
    let app_state = state::AppState::new();

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
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
            tauri_commands::tail_logs
        ])
        .run(tauri::generate_context!())
        .expect("failed to run tauri app");
}
