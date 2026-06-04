#[cfg(feature = "tauri")]
use tauri::Manager;
use serde_json::Value;
#[cfg(feature = "tauri")]
use wsl_bridge_core::HyperVProbeDebug;
#[cfg(feature = "tauri")]
use wsl_bridge_shared::{
    AppRuntimeStatus, AppSettings, ApplyRulesResult, CopyHostsGroupRequest,
    CreateHostsGroupRequest, CreateProxyCertificateRequest, CreateProxyListenerRequest,
    CreateProxyRouteRequest, CreateProxyUpstreamRequest, CreateRuleRequest,
    ExportHostsGroupRequest, HostsEntry, HostsEntryInput, HostsGroup, ImportHostsGroupRequest,
    LogQueryRequest, LogQueryResult, McpServerConfig, McpServerStatus, ProxyCertificate,
    ProxyListener, ProxyRoute, ProxyRouteRuntimeItem, ProxyRule, ProxyRuntimeStatusItem,
    ProxyUpstream, ProxyUpstreamRuntimeItem, QueryTrafficStatsRequest,
    QueryTrafficStatsResult, RuleLogStatsItem, RuleLogStatsRequest, RuleMigrationRecord,
    RulePatch, RuntimeStatusItem, SaveHostsEntriesRequest, StopRulesResult, TailLogsResult,
    TopologySnapshot, TrafficMonitorEntity, TrafficWindowData, TrafficWindowQueryEntity,
    UpdateHostsGroupRequest,
    UpdateProxyCertificateRequest, UpdateProxyListenerRequest, UpdateProxyRouteRequest,
    UpdateProxyUpstreamRequest,
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
pub fn list_agent_targets(scope: Option<String>) -> Result<Value, String> {
    commands::list_agent_targets(scope).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn install_agent_skill_preview(
    target: String,
    scope: Option<String>,
    mode: Option<String>,
    fallback_to_agents_dir: Option<bool>,
) -> Result<Value, String> {
    commands::install_agent_skill_preview(target, scope, mode, fallback_to_agents_dir)
        .map_err(|err| err.to_string())
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
pub fn list_rule_migrations(state: tauri::State<'_, AppState>) -> Vec<RuleMigrationRecord> {
    commands::list_rule_migrations(&state)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn list_proxy_listeners(state: tauri::State<'_, AppState>) -> Vec<ProxyListener> {
    commands::list_proxy_listeners(&state)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn list_proxy_certificates(state: tauri::State<'_, AppState>) -> Vec<ProxyCertificate> {
    commands::list_proxy_certificates(&state)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn list_proxy_routes(
    state: tauri::State<'_, AppState>,
    listener_id: String,
) -> Result<Vec<ProxyRoute>, String> {
    commands::list_proxy_routes(&state, &listener_id).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn list_proxy_upstreams(
    state: tauri::State<'_, AppState>,
    route_id: String,
) -> Result<Vec<ProxyUpstream>, String> {
    commands::list_proxy_upstreams(&state, &route_id).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn get_proxy_runtime_status(
    state: tauri::State<'_, AppState>,
) -> Vec<ProxyRuntimeStatusItem> {
    commands::get_proxy_runtime_status(&state)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn list_proxy_route_runtime(
    state: tauri::State<'_, AppState>,
    listener_id: String,
) -> Vec<ProxyRouteRuntimeItem> {
    commands::list_proxy_route_runtime(&state, &listener_id)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn list_proxy_upstream_runtime(
    state: tauri::State<'_, AppState>,
    route_id: String,
) -> Vec<ProxyUpstreamRuntimeItem> {
    commands::list_proxy_upstream_runtime(&state, &route_id)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn create_proxy_listener(
    state: tauri::State<'_, AppState>,
    req: CreateProxyListenerRequest,
) -> Result<String, String> {
    commands::create_proxy_listener(&state, req).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn create_proxy_certificate(
    state: tauri::State<'_, AppState>,
    req: CreateProxyCertificateRequest,
) -> Result<String, String> {
    commands::create_proxy_certificate(&state, req).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn update_proxy_certificate(
    state: tauri::State<'_, AppState>,
    id: String,
    req: UpdateProxyCertificateRequest,
) -> Result<(), String> {
    commands::update_proxy_certificate(&state, &id, req).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn delete_proxy_certificate(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    commands::delete_proxy_certificate(&state, &id).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn update_proxy_listener(
    state: tauri::State<'_, AppState>,
    id: String,
    req: UpdateProxyListenerRequest,
) -> Result<(), String> {
    commands::update_proxy_listener(&state, &id, req).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn delete_proxy_listener(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    commands::delete_proxy_listener(&state, &id).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn create_proxy_route(
    state: tauri::State<'_, AppState>,
    req: CreateProxyRouteRequest,
) -> Result<String, String> {
    commands::create_proxy_route(&state, req).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn update_proxy_route(
    state: tauri::State<'_, AppState>,
    id: String,
    req: UpdateProxyRouteRequest,
) -> Result<(), String> {
    commands::update_proxy_route(&state, &id, req).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn delete_proxy_route(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    commands::delete_proxy_route(&state, &id).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn create_proxy_upstream(
    state: tauri::State<'_, AppState>,
    req: CreateProxyUpstreamRequest,
) -> Result<String, String> {
    commands::create_proxy_upstream(&state, req).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn update_proxy_upstream(
    state: tauri::State<'_, AppState>,
    id: String,
    req: UpdateProxyUpstreamRequest,
) -> Result<(), String> {
    commands::update_proxy_upstream(&state, &id, req).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn delete_proxy_upstream(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    commands::delete_proxy_upstream(&state, &id).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn bootstrap_default_hosts_group(
    state: tauri::State<'_, AppState>,
) -> Result<HostsGroup, String> {
    commands::bootstrap_default_hosts_group(&state).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn list_hosts_groups(state: tauri::State<'_, AppState>) -> Vec<HostsGroup> {
    commands::list_hosts_groups(&state)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn create_hosts_group(
    state: tauri::State<'_, AppState>,
    req: CreateHostsGroupRequest,
) -> Result<String, String> {
    commands::create_hosts_group(&state, req).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn update_hosts_group(
    state: tauri::State<'_, AppState>,
    id: String,
    req: UpdateHostsGroupRequest,
) -> Result<(), String> {
    commands::update_hosts_group(&state, &id, req).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn delete_hosts_group(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    commands::delete_hosts_group(&state, &id).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn copy_hosts_group(
    state: tauri::State<'_, AppState>,
    req: CopyHostsGroupRequest,
) -> Result<String, String> {
    commands::copy_hosts_group(&state, req).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn list_hosts_entries(
    state: tauri::State<'_, AppState>,
    group_id: String,
) -> Result<Vec<HostsEntry>, String> {
    commands::list_hosts_entries(&state, &group_id).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn save_hosts_entries(
    state: tauri::State<'_, AppState>,
    req: SaveHostsEntriesRequest,
) -> Result<(), String> {
    commands::save_hosts_entries(&state, req).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn import_hosts_group(
    state: tauri::State<'_, AppState>,
    req: ImportHostsGroupRequest,
) -> Result<String, String> {
    commands::import_hosts_group(&state, req).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn preview_hosts_entries_from_file(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<Vec<HostsEntryInput>, String> {
    commands::preview_hosts_entries_from_file(&state, &path).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn export_hosts_group(
    state: tauri::State<'_, AppState>,
    req: ExportHostsGroupRequest,
) -> Result<(), String> {
    commands::export_hosts_group(&state, req).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn activate_hosts_group(
    state: tauri::State<'_, AppState>,
    group_id: String,
) -> Result<(), String> {
    commands::activate_hosts_group(&state, &group_id).map_err(|err| err.to_string())
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
pub fn migrate_rule_to_proxy(
    state: tauri::State<'_, AppState>,
    rule_id: String,
) -> Result<RuleMigrationRecord, String> {
    commands::migrate_rule_to_proxy(&state, &rule_id).map_err(|err| err.to_string())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn rollback_rule_migration(
    state: tauri::State<'_, AppState>,
    rule_id: String,
) -> Result<RuleMigrationRecord, String> {
    commands::rollback_rule_migration(&state, &rule_id).map_err(|err| err.to_string())
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
pub fn list_traffic_monitor_entities(
    state: tauri::State<'_, AppState>,
) -> Vec<TrafficMonitorEntity> {
    commands::list_traffic_monitor_entities(&state)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn get_traffic_window_data(
    state: tauri::State<'_, AppState>,
    entities: Vec<TrafficWindowQueryEntity>,
) -> Vec<TrafficWindowData> {
    commands::get_traffic_window_data(&state, entities)
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
