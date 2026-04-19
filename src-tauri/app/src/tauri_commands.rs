#[cfg(feature = "tauri")]
use tauri::Manager;
#[cfg(feature = "tauri")]
use wsl_bridge_core::HyperVProbeDebug;
#[cfg(feature = "tauri")]
use wsl_bridge_shared::{
    AppRuntimeStatus, AppSettings, ApplyRulesResult, CreateRuleRequest, LogQueryRequest,
    LogQueryResult, McpServerConfig, McpServerStatus, ProxyRule, QueryTrafficStatsRequest,
    QueryTrafficStatsResult, RuleLogStatsItem, RuleLogStatsRequest, RulePatch, RuntimeStatusItem,
    StopRulesResult, TailLogsResult, TopologySnapshot, TrafficWindowData,
};

#[cfg(feature = "tauri")]
use crate::{commands, state::AppState};

#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn scan_topology(state: tauri::State<'_, AppState>) -> Result<TopologySnapshot, String> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || commands::scan_topology(&app_state))
        .await
        .map_err(|err| format!("scan_topology join error: {err}"))
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn debug_hyperv_probe(
    state: tauri::State<'_, AppState>,
) -> Result<HyperVProbeDebug, String> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || commands::debug_hyperv_probe(&app_state))
        .await
        .map_err(|err| format!("debug_hyperv_probe join error: {err}"))
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn get_app_runtime_status() -> AppRuntimeStatus {
    commands::get_app_runtime_status()
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn get_app_settings(state: tauri::State<'_, AppState>) -> AppSettings {
    commands::get_app_settings(&state)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn update_app_settings(
    state: tauri::State<'_, AppState>,
    settings: AppSettings,
) -> Result<(), String> {
    commands::update_app_settings(&state, settings).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn set_tray_visibility(app: tauri::AppHandle, visible: bool) -> Result<(), String> {
    let Some(tray) = app.tray_by_id("main-tray") else {
        return Err("main tray not initialized".to_owned());
    };
    tray.set_visible(visible).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn hide_main_window_to_tray(app: tauri::AppHandle) -> Result<(), String> {
    let Some(tray) = app.tray_by_id("main-tray") else {
        return Err("main tray not initialized".to_owned());
    };
    tray.set_visible(true).map_err(|err| err.to_string())?;

    let Some(window) = app.get_webview_window("main") else {
        return Err("main window not initialized".to_owned());
    };
    window.hide().map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn exit_application(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    commands::stop_rules(&state);
    app.exit(0);
    Ok(())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn list_rules(state: tauri::State<'_, AppState>) -> Vec<ProxyRule> {
    commands::list_rules(&state)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn create_rule(
    state: tauri::State<'_, AppState>,
    req: CreateRuleRequest,
) -> Result<String, String> {
    commands::create_rule(&state, req).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn update_rule(
    state: tauri::State<'_, AppState>,
    id: String,
    patch: RulePatch,
) -> Result<(), String> {
    commands::update_rule(&state, &id, patch).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn delete_rule(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    commands::delete_rule(&state, &id).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn enable_rule(
    state: tauri::State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    commands::enable_rule(&state, &id, enabled).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn apply_rules(state: tauri::State<'_, AppState>) -> ApplyRulesResult {
    commands::apply_rules(&state)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn stop_rules(state: tauri::State<'_, AppState>) -> StopRulesResult {
    commands::stop_rules(&state)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn get_runtime_status(state: tauri::State<'_, AppState>) -> Vec<RuntimeStatusItem> {
    commands::get_runtime_status(&state)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn tail_logs(state: tauri::State<'_, AppState>, cursor: usize) -> TailLogsResult {
    commands::tail_logs(&state, cursor)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn query_logs(state: tauri::State<'_, AppState>, req: LogQueryRequest) -> LogQueryResult {
    commands::query_logs(&state, req)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn get_rule_log_stats(
    state: tauri::State<'_, AppState>,
    req: RuleLogStatsRequest,
) -> Vec<RuleLogStatsItem> {
    commands::get_rule_log_stats(&state, req)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn get_traffic_window_data(
    state: tauri::State<'_, AppState>,
    rule_ids: Vec<String>,
) -> Vec<TrafficWindowData> {
    commands::get_traffic_window_data(&state, rule_ids)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn query_traffic_stats(
    state: tauri::State<'_, AppState>,
    req: QueryTrafficStatsRequest,
) -> QueryTrafficStatsResult {
    commands::query_traffic_stats(&state, req)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn get_mcp_server_status(state: tauri::State<'_, AppState>) -> McpServerStatus {
    commands::get_mcp_server_status(&state)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn update_mcp_server_config(
    state: tauri::State<'_, AppState>,
    config: McpServerConfig,
) -> Result<(), String> {
    commands::update_mcp_server_config(&state, config).map_err(|err| err.to_string())
}
