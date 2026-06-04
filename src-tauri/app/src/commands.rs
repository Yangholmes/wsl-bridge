#![allow(dead_code)]

use anyhow::{bail, Result};
use serde_json::Value;
use wsl_bridge_core::HyperVProbeDebug;
use wsl_bridge_shared::{
    AppRuntimeStatus, AppSettings, ApplyRulesResult, CopyHostsGroupRequest,
    CreateHostsGroupRequest, CreateProxyCertificateRequest, CreateProxyListenerRequest,
    CreateProxyRouteRequest, CreateProxyUpstreamRequest, CreateRuleRequest,
    ExportHostsGroupRequest, HostsEntry, HostsEntryInput, HostsGroup, ImportHostsGroupRequest,
    LogQueryRequest, LogQueryResult, McpServerConfig, McpServerStatus, ProxyCertificate,
    ProxyListener, ProxyRoute, ProxyRouteRuntimeItem, ProxyRule, ProxyRuntimeStatusItem,
    ProxyUpstream, ProxyUpstreamRuntimeItem, QueryTrafficStatsRequest,
    QueryTrafficStatsResult, RuleLogStatsItem, RuleLogStatsRequest, RuleMigrationRecord,
    RulePatch, RuleType, RuntimeStatusItem, SaveHostsEntriesRequest, StopRulesResult,
    TailLogsResult, TopologySnapshot, TrafficMonitorEntity, TrafficWindowData,
    TrafficWindowQueryEntity, UpdateHostsGroupRequest, UpdateProxyCertificateRequest,
    UpdateProxyListenerRequest, UpdateProxyRouteRequest, UpdateProxyUpstreamRequest,
};

use crate::{mcp, runtime_status, state::AppState};

// These functions are intentionally plain Rust handlers.
// In the next step they can be directly wrapped with #[tauri::command].
pub fn scan_topology(state: &AppState) -> TopologySnapshot {
    state.engine.scan_topology()
}

pub fn debug_hyperv_probe(state: &AppState) -> HyperVProbeDebug {
    state.engine.debug_hyperv_probe()
}

pub fn get_app_runtime_status() -> AppRuntimeStatus {
    runtime_status::current_runtime_status()
}

pub fn get_app_settings(state: &AppState) -> AppSettings {
    state.engine.get_app_settings()
}

pub fn update_app_settings(state: &AppState, settings: AppSettings) -> Result<()> {
    state
        .engine
        .update_app_settings(settings)
        .map_err(Into::into)
}

pub fn list_agent_targets(scope: Option<String>) -> Result<Value> {
    mcp::list_agent_targets_payload(scope)
}

pub fn install_agent_skill_preview(
    target: String,
    scope: Option<String>,
    mode: Option<String>,
    fallback_to_agents_dir: Option<bool>,
) -> Result<Value> {
    mcp::install_agent_skill_payload(target, scope, mode, fallback_to_agents_dir)
}

pub fn list_rules(state: &AppState) -> Vec<ProxyRule> {
    state.engine.list_rules()
}

pub fn list_rule_migrations(state: &AppState) -> Vec<RuleMigrationRecord> {
    state.engine.list_rule_migrations()
}

pub fn list_proxy_listeners(state: &AppState) -> Vec<ProxyListener> {
    state.engine.list_proxy_listeners()
}

pub fn list_proxy_certificates(state: &AppState) -> Vec<ProxyCertificate> {
    state.engine.list_proxy_certificates()
}

pub fn list_proxy_routes(state: &AppState, listener_id: &str) -> Result<Vec<ProxyRoute>> {
    state.engine.list_proxy_routes(listener_id).map_err(Into::into)
}

pub fn list_proxy_upstreams(state: &AppState, route_id: &str) -> Result<Vec<ProxyUpstream>> {
    state.engine.list_proxy_upstreams(route_id).map_err(Into::into)
}

pub fn get_proxy_runtime_status(state: &AppState) -> Vec<ProxyRuntimeStatusItem> {
    state.engine.get_proxy_runtime_status()
}

pub fn list_proxy_route_runtime(
    state: &AppState,
    listener_id: &str,
) -> Vec<ProxyRouteRuntimeItem> {
    state.engine.list_proxy_route_runtime(listener_id)
}

pub fn list_proxy_upstream_runtime(
    state: &AppState,
    route_id: &str,
) -> Vec<ProxyUpstreamRuntimeItem> {
    state.engine.list_proxy_upstream_runtime(route_id)
}

pub fn create_proxy_listener(state: &AppState, req: CreateProxyListenerRequest) -> Result<String> {
    state.engine.create_proxy_listener(req).map_err(Into::into)
}

pub fn create_proxy_certificate(
    state: &AppState,
    req: CreateProxyCertificateRequest,
) -> Result<String> {
    state.engine.create_proxy_certificate(req).map_err(Into::into)
}

pub fn update_proxy_certificate(
    state: &AppState,
    id: &str,
    req: UpdateProxyCertificateRequest,
) -> Result<()> {
    state
        .engine
        .update_proxy_certificate(id, req)
        .map_err(Into::into)
}

pub fn delete_proxy_certificate(state: &AppState, id: &str) -> Result<()> {
    state.engine.delete_proxy_certificate(id).map_err(Into::into)
}

pub fn update_proxy_listener(
    state: &AppState,
    id: &str,
    req: UpdateProxyListenerRequest,
) -> Result<()> {
    state.engine.update_proxy_listener(id, req).map_err(Into::into)
}

pub fn delete_proxy_listener(state: &AppState, id: &str) -> Result<()> {
    state.engine.delete_proxy_listener(id).map_err(Into::into)
}

pub fn create_proxy_route(state: &AppState, req: CreateProxyRouteRequest) -> Result<String> {
    state.engine.create_proxy_route(req).map_err(Into::into)
}

pub fn update_proxy_route(
    state: &AppState,
    id: &str,
    req: UpdateProxyRouteRequest,
) -> Result<()> {
    state.engine.update_proxy_route(id, req).map_err(Into::into)
}

pub fn delete_proxy_route(state: &AppState, id: &str) -> Result<()> {
    state.engine.delete_proxy_route(id).map_err(Into::into)
}

pub fn create_proxy_upstream(state: &AppState, req: CreateProxyUpstreamRequest) -> Result<String> {
    state.engine.create_proxy_upstream(req).map_err(Into::into)
}

pub fn update_proxy_upstream(
    state: &AppState,
    id: &str,
    req: UpdateProxyUpstreamRequest,
) -> Result<()> {
    state.engine.update_proxy_upstream(id, req).map_err(Into::into)
}

pub fn delete_proxy_upstream(state: &AppState, id: &str) -> Result<()> {
    state.engine.delete_proxy_upstream(id).map_err(Into::into)
}

pub fn bootstrap_default_hosts_group(state: &AppState) -> Result<HostsGroup> {
    state.engine.bootstrap_default_hosts_group().map_err(Into::into)
}

pub fn list_hosts_groups(state: &AppState) -> Vec<HostsGroup> {
    state.engine.list_hosts_groups()
}

pub fn create_hosts_group(state: &AppState, req: CreateHostsGroupRequest) -> Result<String> {
    state.engine.create_hosts_group(req).map_err(Into::into)
}

pub fn update_hosts_group(
    state: &AppState,
    id: &str,
    req: UpdateHostsGroupRequest,
) -> Result<()> {
    state.engine.update_hosts_group(id, req).map_err(Into::into)
}

pub fn delete_hosts_group(state: &AppState, id: &str) -> Result<()> {
    state.engine.delete_hosts_group(id).map_err(Into::into)
}

pub fn copy_hosts_group(state: &AppState, req: CopyHostsGroupRequest) -> Result<String> {
    state.engine.copy_hosts_group(req).map_err(Into::into)
}

pub fn list_hosts_entries(state: &AppState, group_id: &str) -> Result<Vec<HostsEntry>> {
    state.engine.list_hosts_entries(group_id).map_err(Into::into)
}

pub fn save_hosts_entries(state: &AppState, req: SaveHostsEntriesRequest) -> Result<()> {
    state.engine.save_hosts_entries(req).map_err(Into::into)
}

pub fn import_hosts_group(state: &AppState, req: ImportHostsGroupRequest) -> Result<String> {
    state.engine.import_hosts_group(req).map_err(Into::into)
}

pub fn preview_hosts_entries_from_file(
    state: &AppState,
    path: &str,
) -> Result<Vec<HostsEntryInput>> {
    state.engine.preview_hosts_entries_from_file(path).map_err(Into::into)
}

pub fn export_hosts_group(state: &AppState, req: ExportHostsGroupRequest) -> Result<()> {
    state.engine.export_hosts_group(req).map_err(Into::into)
}

pub fn activate_hosts_group(state: &AppState, group_id: &str) -> Result<()> {
    state.engine.activate_hosts_group(group_id).map_err(Into::into)
}

pub fn create_rule(state: &AppState, req: CreateRuleRequest) -> Result<String> {
    if matches!(req.rule.rule_type, RuleType::TcpFwd | RuleType::HttpProxy) {
        bail!("tcp_fwd and http_proxy can no longer be created in Rules; use Proxy instead");
    }
    state.engine.create_rule(req).map_err(Into::into)
}

pub fn migrate_rule_to_proxy(state: &AppState, rule_id: &str) -> Result<RuleMigrationRecord> {
    state.engine.migrate_rule_to_proxy(rule_id).map_err(Into::into)
}

pub fn rollback_rule_migration(
    state: &AppState,
    rule_id: &str,
) -> Result<RuleMigrationRecord> {
    state.engine.rollback_rule_migration(rule_id).map_err(Into::into)
}

pub fn update_rule(state: &AppState, id: &str, patch: RulePatch) -> Result<()> {
    state.engine.update_rule(id, patch).map_err(Into::into)
}

pub fn delete_rule(state: &AppState, id: &str) -> Result<()> {
    state.engine.delete_rule(id).map_err(Into::into)
}

pub fn enable_rule(state: &AppState, id: &str, enabled: bool) -> Result<()> {
    state.engine.enable_rule(id, enabled).map_err(Into::into)
}

pub fn apply_rules(state: &AppState) -> ApplyRulesResult {
    state.engine.apply_rules()
}

pub fn stop_rules(state: &AppState) -> StopRulesResult {
    state.engine.stop_rules()
}

pub fn get_runtime_status(state: &AppState) -> Vec<RuntimeStatusItem> {
    state.engine.get_runtime_status()
}

pub fn tail_logs(state: &AppState, cursor: usize) -> TailLogsResult {
    state.engine.tail_logs(cursor)
}

pub fn query_logs(state: &AppState, req: LogQueryRequest) -> LogQueryResult {
    state.engine.query_logs(req)
}

pub fn get_rule_log_stats(state: &AppState, req: RuleLogStatsRequest) -> Vec<RuleLogStatsItem> {
    state.engine.get_rule_log_stats(req)
}

pub fn list_traffic_monitor_entities(state: &AppState) -> Vec<TrafficMonitorEntity> {
    state.engine.list_traffic_monitor_entities()
}

pub fn get_traffic_window_data(
    state: &AppState,
    entities: Vec<TrafficWindowQueryEntity>,
) -> Vec<TrafficWindowData> {
    state.engine.get_traffic_window_data(entities)
}

pub fn query_traffic_stats(
    state: &AppState,
    req: QueryTrafficStatsRequest,
) -> QueryTrafficStatsResult {
    state.engine.query_traffic_stats(req)
}

pub fn get_mcp_server_status(state: &AppState) -> McpServerStatus {
    mcp::build_server_status(state)
}

pub fn update_mcp_server_config(state: &AppState, config: McpServerConfig) -> Result<()> {
    state
        .engine
        .update_mcp_config(config.clone())
        .map_err(anyhow::Error::from)?;
    state.mcp_service.apply_config(&config);
    Ok(())
}
