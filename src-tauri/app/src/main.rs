#![cfg_attr(
    all(feature = "tauri", target_os = "windows"),
    windows_subsystem = "windows"
)]

mod commands;
mod mcp;
mod runtime_status;
mod state;
mod tauri_commands;

#[cfg(feature = "tauri")]
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};
#[cfg(not(feature = "tauri"))]
use wsl_bridge_shared::{BindMode, CreateRuleRequest, NewProxyRule, RuleType, TargetKind};

#[cfg(feature = "tauri")]
const MAIN_TRAY_ID: &str = "main-tray";

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
    app.engine.apply_proxy_listeners();

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
        .plugin(tauri_plugin_dialog::init())
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
            state.engine.apply_proxy_listeners();
            println!(
                "wsl-bridge app bootstrap ready: applied={}, failed={}",
                result.applied,
                result.failed.len()
            );
            app.manage(state);
            setup_tray(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            tauri_commands::get_app_runtime_status,
            tauri_commands::get_app_settings,
            tauri_commands::update_app_settings,
            tauri_commands::list_agent_targets,
            tauri_commands::install_agent_mcp_client,
            tauri_commands::uninstall_agent_mcp_client,
            tauri_commands::install_agent_skill_preview,
            tauri_commands::install_agent_skill,
            tauri_commands::uninstall_agent_skill_preview,
            tauri_commands::uninstall_agent_skill,
            tauri_commands::set_tray_visibility,
            tauri_commands::hide_main_window_to_tray,
            tauri_commands::exit_application,
            tauri_commands::scan_topology,
            tauri_commands::debug_hyperv_probe,
            tauri_commands::list_rules,
            tauri_commands::list_rule_migrations,
            tauri_commands::list_proxy_listeners,
            tauri_commands::list_proxy_certificates,
            tauri_commands::list_proxy_routes,
            tauri_commands::list_proxy_upstreams,
            tauri_commands::get_proxy_runtime_status,
            tauri_commands::list_proxy_route_runtime,
            tauri_commands::list_proxy_upstream_runtime,
            tauri_commands::create_proxy_listener,
            tauri_commands::create_proxy_certificate,
            tauri_commands::update_proxy_certificate,
            tauri_commands::delete_proxy_certificate,
            tauri_commands::update_proxy_listener,
            tauri_commands::delete_proxy_listener,
            tauri_commands::create_proxy_route,
            tauri_commands::update_proxy_route,
            tauri_commands::delete_proxy_route,
            tauri_commands::create_proxy_upstream,
            tauri_commands::update_proxy_upstream,
            tauri_commands::delete_proxy_upstream,
            tauri_commands::bootstrap_default_hosts_group,
            tauri_commands::list_hosts_groups,
            tauri_commands::create_hosts_group,
            tauri_commands::update_hosts_group,
            tauri_commands::delete_hosts_group,
            tauri_commands::copy_hosts_group,
            tauri_commands::list_hosts_entries,
            tauri_commands::save_hosts_entries,
            tauri_commands::import_hosts_group,
            tauri_commands::preview_hosts_entries_from_file,
            tauri_commands::export_hosts_group,
            tauri_commands::activate_hosts_group,
            tauri_commands::create_rule,
            tauri_commands::migrate_rule_to_proxy,
            tauri_commands::rollback_rule_migration,
            tauri_commands::update_rule,
            tauri_commands::delete_rule,
            tauri_commands::enable_rule,
            tauri_commands::apply_rules,
            tauri_commands::stop_rules,
            tauri_commands::get_runtime_status,
            tauri_commands::tail_logs,
            tauri_commands::query_logs,
            tauri_commands::get_rule_log_stats,
            tauri_commands::list_traffic_monitor_entities,
            tauri_commands::get_traffic_window_data,
            tauri_commands::query_traffic_stats,
            tauri_commands::get_mcp_server_status,
            tauri_commands::update_mcp_server_config
        ])
        .run(tauri::generate_context!())
        .expect("failed to run tauri app");
}

#[cfg(feature = "tauri")]
fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let show_item = MenuItemBuilder::with_id("show", "Show Window").build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&show_item, &quit_item])
        .build()?;

    let tray = TrayIconBuilder::with_id(MAIN_TRAY_ID)
        .icon(
            app.default_window_icon()
                .expect("default window icon should be available")
                .clone(),
        )
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("WSL Bridge")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                let _ = show_main_window(app);
            }
            "quit" => {
                exit_application(app);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                let _ = show_main_window(&app);
            }
        })
        .build(app)?;

    let show_tray = app
        .state::<state::AppState>()
        .engine
        .get_app_settings()
        .show_tray_on_start;
    let _ = tray.set_visible(show_tray);
    Ok(())
}

#[cfg(feature = "tauri")]
fn show_main_window(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    Ok(())
}

#[cfg(feature = "tauri")]
fn exit_application(app: &AppHandle) {
    let state = app.state::<state::AppState>();
    state.engine.stop_rules();
    app.exit(0);
}
