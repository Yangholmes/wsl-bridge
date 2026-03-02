#![allow(dead_code)]

use anyhow::Result;
use wsl_bridge_shared::{
    ApplyRulesResult, CreateRuleRequest, ProxyRule, RulePatch, RuntimeStatusItem, StopRulesResult,
    TailLogsResult, TopologySnapshot,
};

use crate::state::AppState;

// These functions are intentionally plain Rust handlers.
// In the next step they can be directly wrapped with #[tauri::command].
pub fn scan_topology(state: &AppState) -> TopologySnapshot {
    state.engine.scan_topology()
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
