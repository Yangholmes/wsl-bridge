#[cfg(feature = "tauri")]
use wsl_bridge_shared::{
    ApplyRulesResult, CreateRuleRequest, ProxyRule, RulePatch, RuntimeStatusItem, StopRulesResult,
    TailLogsResult, TopologySnapshot,
};

#[cfg(feature = "tauri")]
use crate::{commands, state::AppState};

#[cfg(feature = "tauri")]
#[tauri::command]
pub fn scan_topology(state: tauri::State<'_, AppState>) -> TopologySnapshot {
    commands::scan_topology(&state)
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
