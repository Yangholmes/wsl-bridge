#![allow(dead_code)]

use anyhow::Result;
use wsl_bridge_core::HyperVProbeDebug;
use wsl_bridge_shared::{
    AppRuntimeStatus, ApplyRulesResult, CreateRuleRequest, LogQueryRequest, LogQueryResult, McpServerConfig,
    McpServerStatus, ProxyRule, RuleLogStatsItem, RuleLogStatsRequest, RulePatch,
    RuntimeStatusItem, StopRulesResult, TailLogsResult, TopologySnapshot,
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

pub fn list_rules(state: &AppState) -> Vec<ProxyRule> {
    state.engine.list_rules()
}

pub fn create_rule(state: &AppState, req: CreateRuleRequest) -> Result<String> {
    state.engine.create_rule(req).map_err(Into::into)
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
