use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddrV4, TcpListener, TcpStream};
#[cfg(windows)]
use std::os::windows::io::AsRawSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Value};
use socket2::{Domain, Protocol, Socket, Type};
use uuid::Uuid;
use wsl_bridge_core::RuleEngine;
use wsl_bridge_shared::{
    CreateRuleRequest, FirewallPolicy, LogQueryRequest, McpClientPreset, McpServerConfig,
    McpServerStatus, McpToolDescriptor, NewFirewallPolicy, NewProxyRule, ProxyRule,
    QueryTrafficStatsRequest, RulePatch, RuleType, TargetKind, TopologySnapshot,
};

use crate::state::AppState;
#[cfg(windows)]
use windows_sys::Win32::Networking::WinSock::{
    setsockopt, SOCKET_ERROR, SOL_SOCKET, SO_EXCLUSIVEADDRUSE,
};

const MCP_PATH: &str = "/mcp";
const HEALTH_PATH: &str = "/health";
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-03-26", "2024-11-05"];
const DEFAULT_PROTOCOL_VERSION: &str = "2025-03-26";
const AI_API_VERSION: &str = "phase3.ai.v1";
const CONFIG_PATCH_VERSION: &str = "phase3.ai-patch.v1";

#[derive(Debug)]
struct ServerHandle {
    shutdown: Arc<AtomicBool>,
    port: u16,
    join: JoinHandle<()>,
}

#[derive(Debug)]
pub struct McpHttpService {
    engine: Arc<RuleEngine>,
    active: Mutex<Option<ServerHandle>>,
    last_error: Mutex<Option<String>>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ForwardRuleView {
    rule: ProxyRule,
    firewall: FirewallPolicy,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TopologyArgs {
    #[serde(default)]
    include_adapters: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FirewallArgs {
    allow_domain: Option<bool>,
    allow_private: Option<bool>,
    allow_public: Option<bool>,
    direction: Option<String>,
    action: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateForwardRuleArgs {
    name: String,
    #[serde(rename = "type")]
    rule_type: RuleType,
    listen_host: Option<String>,
    listen_port: u16,
    target_kind: TargetKind,
    target_ref: Option<String>,
    target_host: Option<String>,
    target_port: u16,
    bind_mode: Option<wsl_bridge_shared::BindMode>,
    nic_id: Option<String>,
    enabled: Option<bool>,
    firewall: Option<FirewallArgs>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateForwardRuleArgs {
    id: String,
    name: Option<String>,
    listen_host: Option<String>,
    listen_port: Option<u16>,
    target_ref: Option<Option<String>>,
    target_host: Option<Option<String>>,
    target_port: Option<Option<u16>>,
    bind_mode: Option<wsl_bridge_shared::BindMode>,
    nic_id: Option<Option<String>>,
    enabled: Option<bool>,
    firewall: Option<FirewallArgs>,
}

#[derive(Debug, Deserialize)]
struct DeleteForwardRuleArgs {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToggleForwardRuleArgs {
    id: String,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryTrafficStatsArgs {
    entity_type: Option<wsl_bridge_shared::TrafficEntityType>,
    entity_id: Option<String>,
    rule_id: String,
    start_time: Option<chrono::DateTime<chrono::Utc>>,
    end_time: Option<chrono::DateTime<chrono::Utc>>,
    interval: Option<wsl_bridge_shared::TrafficStatsInterval>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetTrafficWindowArgs {
    entity_type: Option<wsl_bridge_shared::TrafficEntityType>,
    entity_id: Option<String>,
    rule_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InspectAppArgs {
    modules: Option<Vec<String>>,
    detail: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ValidateConfigArgs {
    modules: Option<Vec<String>>,
    patch: Option<Value>,
    checks: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListAgentTargetsArgs {
    scope: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallAgentSkillArgs {
    target: String,
    scope: Option<String>,
    mode: Option<String>,
    fallback_to_agents_dir: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcMessage {
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: Option<String>,
    params: Option<Value>,
}

#[derive(Debug)]
struct ParsedRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl McpHttpService {
    pub fn new(engine: Arc<RuleEngine>) -> Self {
        Self {
            engine,
            active: Mutex::new(None),
            last_error: Mutex::new(None),
        }
    }

    pub fn apply_config(&self, config: &McpServerConfig) {
        self.stop();
        if !config.enabled {
            *self.last_error.lock() = None;
            return;
        }

        match self.start(config.listen_port) {
            Ok((handle, actual_port)) => {
                if actual_port != config.listen_port {
                    let mut updated = config.clone();
                    updated.listen_port = actual_port;
                    if let Err(err) = self.engine.update_mcp_config(updated) {
                        *self.last_error.lock() = Some(err.to_string());
                    }
                }
                *self.active.lock() = Some(handle);
                *self.last_error.lock() = None;
            }
            Err(err) => {
                *self.last_error.lock() = Some(err.to_string());
            }
        }
    }

    pub fn stop(&self) {
        let old = self.active.lock().take();
        if let Some(handle) = old {
            handle.shutdown.store(true, Ordering::Relaxed);
            let _ = TcpStream::connect(("127.0.0.1", handle.port));
            let _ = handle.join.join();
        }
    }

    pub fn is_running(&self) -> bool {
        self.active.lock().is_some()
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().clone()
    }

    fn start(&self, port: u16) -> Result<(ServerHandle, u16)> {
        let (listener, actual_port) = bind_listener(port)?;
        listener
            .set_nonblocking(true)
            .map_err(|err| anyhow!("failed to configure listener: {err}"))?;

        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_flag = shutdown.clone();
        let engine = self.engine.clone();

        let join = thread::spawn(move || {
            while !shutdown_flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let engine = engine.clone();
                        let _ = thread::spawn(move || {
                            let _ = handle_connection(stream, &engine);
                        });
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(80));
                    }
                    Err(_) => {
                        thread::sleep(Duration::from_millis(80));
                    }
                }
            }
        });

        Ok((
            ServerHandle {
                shutdown,
                port: actual_port,
                join,
            },
            actual_port,
        ))
    }
}

impl Drop for McpHttpService {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn ensure_initialized_config(state: &AppState) {
    let mut config = state.engine.get_mcp_config();
    let mut changed = false;
    if config.api_token.trim().is_empty() {
        config.api_token = generate_api_token();
        changed = true;
    }
    if config.listen_port == 0 {
        config.listen_port = 13746;
        changed = true;
    }
    if changed {
        let _ = state.engine.update_mcp_config(config.clone());
    }
    state.mcp_service.apply_config(&config);
}

pub fn build_server_status(state: &AppState) -> McpServerStatus {
    let config = state.engine.get_mcp_config();
    let base_url = format!("http://127.0.0.1:{}{}", config.listen_port, MCP_PATH);
    let tools = describe_tools(&config);
    let client_presets = build_client_presets(&config, &base_url);

    McpServerStatus {
        config,
        base_url,
        running: state.mcp_service.is_running(),
        last_error: state.mcp_service.last_error(),
        tools,
        client_presets,
    }
}

fn handle_connection(mut stream: TcpStream, engine: &Arc<RuleEngine>) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let request = read_request(&mut stream)?;

    if request.method == "GET" && request.path == HEALTH_PATH {
        write_http_response(
            &mut stream,
            200,
            "OK",
            &[("Content-Type", "application/json")],
            br#"{"ok":true}"#,
        )?;
        let _ = stream.shutdown(Shutdown::Both);
        return Ok(());
    }

    if request.method != "POST" || request.path != MCP_PATH {
        write_http_response(
            &mut stream,
            404,
            "Not Found",
            &[("Content-Type", "application/json")],
            br#"{"error":"not_found"}"#,
        )?;
        let _ = stream.shutdown(Shutdown::Both);
        return Ok(());
    }

    let config = engine.get_mcp_config();
    if !is_authorized(&request, &config.api_token) {
        write_http_response(
            &mut stream,
            401,
            "Unauthorized",
            &[
                ("Content-Type", "application/json"),
                ("WWW-Authenticate", "Bearer"),
            ],
            br#"{"error":"unauthorized"}"#,
        )?;
        let _ = stream.shutdown(Shutdown::Both);
        return Ok(());
    }

    let message = match serde_json::from_slice::<JsonRpcMessage>(&request.body) {
        Ok(value) => value,
        Err(err) => {
            let body =
                serde_json::to_vec(&jsonrpc_error(None, -32700, &format!("parse error: {err}")))?;
            write_http_response(
                &mut stream,
                400,
                "Bad Request",
                &[("Content-Type", "application/json")],
                &body,
            )?;
            let _ = stream.shutdown(Shutdown::Both);
            return Ok(());
        }
    };

    let response = match handle_jsonrpc_message(engine, message) {
        Some(value) => value,
        None => json!({}),
    };
    let body = serde_json::to_vec(&response)?;
    write_http_response(
        &mut stream,
        200,
        "OK",
        &[
            ("Content-Type", "application/json"),
            ("Cache-Control", "no-store"),
        ],
        &body,
    )?;
    let _ = stream.shutdown(Shutdown::Both);
    Ok(())
}

fn handle_jsonrpc_message(engine: &Arc<RuleEngine>, message: JsonRpcMessage) -> Option<Value> {
    if message.jsonrpc.as_deref() != Some("2.0") {
        return Some(jsonrpc_error(
            message.id.clone(),
            -32600,
            "invalid jsonrpc version",
        ));
    }

    let Some(method) = message.method.as_deref() else {
        return Some(jsonrpc_error(
            message.id.clone(),
            -32600,
            "method is required",
        ));
    };

    match method {
        "initialize" => handle_initialize(message.id, message.params.as_ref()),
        "notifications/initialized" => None,
        "ping" => Some(jsonrpc_result(message.id, json!({}))),
        "resources/list" => Some(handle_resources_list(message.id)),
        "resources/read" => Some(handle_resources_read(
            message.id,
            engine,
            &engine.get_mcp_config(),
            message.params.as_ref(),
        )),
        "tools/list" => Some(handle_tools_list(message.id, &engine.get_mcp_config())),
        "tools/call" => Some(handle_tools_call(
            message.id,
            engine,
            &engine.get_mcp_config(),
            message.params.as_ref(),
        )),
        _ => Some(jsonrpc_error(
            message.id,
            -32601,
            &format!("method not found: {method}"),
        )),
    }
}

fn handle_initialize(id: Option<Value>, params: Option<&Value>) -> Option<Value> {
    let requested = params
        .and_then(|value| value.get("protocolVersion"))
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_PROTOCOL_VERSION);
    let protocol_version = if SUPPORTED_PROTOCOL_VERSIONS.contains(&requested) {
        requested
    } else {
        DEFAULT_PROTOCOL_VERSION
    };

    Some(jsonrpc_result(
        id,
        json!({
          "protocolVersion": protocol_version,
          "capabilities": {
            "tools": {
              "listChanged": false
            },
            "resources": {
              "subscribe": false,
              "listChanged": false
            }
          },
          "serverInfo": {
            "name": "wsl-bridge",
            "title": "WSL Bridge MCP",
            "version": env!("CARGO_PKG_VERSION")
          }
        }),
    ))
}

fn handle_resources_list(id: Option<Value>) -> Value {
    jsonrpc_result(
        id,
        json!({
          "resources": [
            {
              "uri": "wsl-bridge://ai-guide",
              "name": "AI Guide",
              "description": "Usage guide for AI agents operating wsl-bridge.",
              "mimeType": "text/markdown"
            },
            {
              "uri": "wsl-bridge://capabilities",
              "name": "Capabilities",
              "description": "Current AI API capabilities and safety defaults.",
              "mimeType": "application/json"
            },
            {
              "uri": "wsl-bridge://state/summary",
              "name": "State Summary",
              "description": "Compact app state summary for AI agents.",
              "mimeType": "application/json"
            },
            {
              "uri": "wsl-bridge://state/proxy",
              "name": "Proxy State",
              "description": "Proxy listener, route, upstream, certificate and runtime summary.",
              "mimeType": "application/json"
            },
            {
              "uri": "wsl-bridge://state/hosts",
              "name": "Hosts State",
              "description": "Hosts group, active group and entry summary.",
              "mimeType": "application/json"
            },
            {
              "uri": "wsl-bridge://state/traffic",
              "name": "Traffic State",
              "description": "Traffic monitor entity and recent in-memory traffic summary.",
              "mimeType": "application/json"
            },
            {
              "uri": "wsl-bridge://logs/recent",
              "name": "Recent Logs",
              "description": "Recent audit logs for diagnostics.",
              "mimeType": "application/json"
            },
            {
              "uri": "wsl-bridge://schemas/config-patch",
              "name": "ConfigPatch Schema",
              "description": "Draft schema for structured configuration patches.",
              "mimeType": "application/json"
            }
          ]
        }),
    )
}

fn handle_resources_read(
    id: Option<Value>,
    engine: &Arc<RuleEngine>,
    config: &McpServerConfig,
    params: Option<&Value>,
) -> Value {
    let Some(uri) = params
        .and_then(|value| value.get("uri"))
        .and_then(Value::as_str)
    else {
        return jsonrpc_error(id, -32602, "resources/read params.uri is required");
    };

    let result = match read_resource(uri, engine, config) {
        Ok((mime_type, text)) => jsonrpc_result(
            id,
            json!({
              "contents": [
                {
                  "uri": uri,
                  "mimeType": mime_type,
                  "text": text
                }
              ]
            }),
        ),
        Err(err) => jsonrpc_error(id, -32602, &err.to_string()),
    };
    result
}

fn read_resource(
    uri: &str,
    engine: &Arc<RuleEngine>,
    config: &McpServerConfig,
) -> Result<(&'static str, String)> {
    match uri {
        "wsl-bridge://ai-guide" => Ok(("text/markdown", ai_guide_resource().to_owned())),
        "wsl-bridge://capabilities" => Ok((
            "application/json",
            serde_json::to_string_pretty(&capabilities_resource(config))?,
        )),
        "wsl-bridge://state/summary" => Ok((
            "application/json",
            serde_json::to_string_pretty(&state_summary_resource(engine, config))?,
        )),
        "wsl-bridge://state/proxy" => Ok((
            "application/json",
            serde_json::to_string_pretty(&state_proxy_resource(engine))?,
        )),
        "wsl-bridge://state/hosts" => Ok((
            "application/json",
            serde_json::to_string_pretty(&state_hosts_resource(engine))?,
        )),
        "wsl-bridge://state/traffic" => Ok((
            "application/json",
            serde_json::to_string_pretty(&state_traffic_resource(engine))?,
        )),
        "wsl-bridge://logs/recent" => Ok((
            "application/json",
            serde_json::to_string_pretty(&recent_logs_resource(engine))?,
        )),
        "wsl-bridge://schemas/config-patch" => Ok((
            "application/json",
            serde_json::to_string_pretty(&config_patch_schema_resource())?,
        )),
        _ => Err(anyhow!("resource not found: {uri}")),
    }
}

fn handle_tools_list(id: Option<Value>, config: &McpServerConfig) -> Value {
    jsonrpc_result(
        id,
        json!({
          "tools": build_tool_definitions(config)
        }),
    )
}

fn handle_tools_call(
    id: Option<Value>,
    engine: &Arc<RuleEngine>,
    config: &McpServerConfig,
    params: Option<&Value>,
) -> Value {
    let Some(params) = params else {
        return jsonrpc_error(id, -32602, "tools/call params are required");
    };
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return jsonrpc_error(id, -32602, "tool name is required");
    };
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let result = match name {
        "inspect_app" => execute_inspect_app(engine, config, arguments),
        "validate_config" => execute_validate_config(engine, arguments),
        "list_agent_targets" => execute_list_agent_targets(arguments),
        "install_agent_skill" => execute_install_agent_skill(arguments),
        "read_virtualization_topology" if config.expose_topology_read => {
            execute_read_virtualization_topology(engine, arguments)
        }
        "list_forward_rules" if config.expose_rule_config => execute_list_forward_rules(engine),
        "create_forward_rule" if config.expose_rule_config => {
            execute_create_forward_rule(engine, arguments)
        }
        "update_forward_rule" if config.expose_rule_config => {
            execute_update_forward_rule(engine, arguments)
        }
        "delete_forward_rule" if config.expose_rule_config => {
            execute_delete_forward_rule(engine, arguments)
        }
        "set_forward_rule_enabled" if config.expose_rule_config => {
            execute_set_forward_rule_enabled(engine, arguments)
        }
        "query_traffic_stats" if config.expose_traffic_stats => {
            execute_query_traffic_stats(engine, arguments)
        }
        "get_traffic_window" if config.expose_traffic_stats => {
            execute_get_traffic_window(engine, arguments)
        }
        _ => Err(anyhow!("tool not found or disabled: {name}")),
    };

    match result {
        Ok(payload) => jsonrpc_result(
            id,
            json!({
              "content": [
                {
                  "type": "text",
                  "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_owned())
                }
              ],
              "structuredContent": payload
            }),
        ),
        Err(err) => jsonrpc_result(
            id,
            json!({
              "content": [
                {
                  "type": "text",
                  "text": err.to_string()
                }
              ],
              "isError": true
            }),
        ),
    }
}

fn execute_read_virtualization_topology(
    engine: &Arc<RuleEngine>,
    arguments: Value,
) -> Result<Value> {
    let args: TopologyArgs = serde_json::from_value(arguments)?;
    let topology = engine.scan_topology();
    Ok(topology_to_value(topology, args.include_adapters))
}

fn execute_inspect_app(
    engine: &Arc<RuleEngine>,
    config: &McpServerConfig,
    arguments: Value,
) -> Result<Value> {
    let args: InspectAppArgs = serde_json::from_value(arguments)?;
    let modules = args.modules.unwrap_or_else(|| {
        vec![
            "summary".to_owned(),
            "rules".to_owned(),
            "proxy".to_owned(),
            "hosts".to_owned(),
            "traffic".to_owned(),
        ]
    });
    let detail = args.detail.unwrap_or_else(|| "summary".to_owned());
    let mut result = serde_json::Map::new();

    for module in modules {
        match module.as_str() {
            "summary" => {
                result.insert("summary".to_owned(), state_summary_resource(engine, config));
            }
            "rules" => {
                result.insert("rules".to_owned(), inspect_rules(engine, &detail));
            }
            "proxy" => {
                result.insert("proxy".to_owned(), state_proxy_resource(engine));
            }
            "hosts" => {
                result.insert("hosts".to_owned(), state_hosts_resource(engine));
            }
            "traffic" => {
                result.insert("traffic".to_owned(), state_traffic_resource(engine));
            }
            other => {
                result.insert(
                    other.to_owned(),
                    json!({
                      "status": "unknown-module"
                    }),
                );
            }
        }
    }

    Ok(json!({
      "aiApiVersion": AI_API_VERSION,
      "detail": detail,
      "modules": result
    }))
}

fn execute_validate_config(engine: &Arc<RuleEngine>, arguments: Value) -> Result<Value> {
    let args: ValidateConfigArgs = serde_json::from_value(arguments)?;
    let modules = args.modules.unwrap_or_else(|| vec!["summary".to_owned(), "rules".to_owned()]);
    let checks = args.checks.unwrap_or_else(|| {
        vec![
            "schema".to_owned(),
            "conflict".to_owned(),
            "permission".to_owned(),
        ]
    });
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if checks.iter().any(|check| check == "schema") {
        if let Some(patch) = args.patch.as_ref() {
            validate_config_patch_shape(patch, &mut errors, &mut warnings);
        }
    }

    if checks.iter().any(|check| check == "conflict") && modules.iter().any(|item| item == "rules") {
        validate_legacy_rule_listen_conflicts(engine, &mut warnings);
    }

    if checks.iter().any(|check| check == "permission") {
        warnings.push(json!({
          "severity": "info",
          "code": "SENSITIVE_OPERATIONS_REQUIRE_CONFIRMATION",
          "message": "System hosts writes, 0.0.0.0 listeners, destructive proxy changes, config overwrites, and Agent skill installation require explicit confirmation."
        }));
    }

    if args.patch.is_some() {
        warnings.push(json!({
          "severity": "info",
          "code": "CONFIG_PATCH_APPLY_NOT_AVAILABLE",
          "message": "ConfigPatch apply is not enabled yet. This tool only validates shape and obvious conflicts in the current build."
        }));
    }

    Ok(json!({
      "ok": errors.is_empty(),
      "aiApiVersion": AI_API_VERSION,
      "checkedModules": modules,
      "checks": checks,
      "errors": errors,
      "warnings": warnings
    }))
}

fn execute_list_agent_targets(arguments: Value) -> Result<Value> {
    let args: ListAgentTargetsArgs = serde_json::from_value(arguments)?;
    list_agent_targets_payload(args.scope)
}

pub(crate) fn list_agent_targets_payload(scope: Option<String>) -> Result<Value> {
    let scope = normalize_install_scope(scope.as_deref());
    Ok(json!({
      "skill": skill_manifest_summary(),
      "scope": scope,
      "targets": agent_targets()
        .into_iter()
        .map(|target| agent_target_descriptor(target, scope))
        .collect::<Vec<_>>()
    }))
}

fn execute_install_agent_skill(arguments: Value) -> Result<Value> {
    let args: InstallAgentSkillArgs = serde_json::from_value(arguments)?;
    install_agent_skill_payload(
        args.target,
        args.scope,
        args.mode,
        args.fallback_to_agents_dir,
    )
}

pub(crate) fn install_agent_skill_payload(
    target: String,
    scope: Option<String>,
    mode: Option<String>,
    fallback_to_agents_dir: Option<bool>,
) -> Result<Value> {
    let mode = mode.as_deref().unwrap_or("dryRun");
    if mode != "dryRun" {
        return Err(anyhow!(
            "install_agent_skill currently supports mode=dryRun only; apply is planned but not enabled"
        ));
    }

    let scope = normalize_install_scope(scope.as_deref());
    let fallback_to_agents_dir = fallback_to_agents_dir.unwrap_or(true);
    let target = normalize_agent_target(&target);
    let plan = build_agent_skill_install_plan(&target, scope, fallback_to_agents_dir)?;
    Ok(plan)
}

fn execute_list_forward_rules(engine: &Arc<RuleEngine>) -> Result<Value> {
    let items = engine
        .list_forward_rules_with_firewall()
        .into_iter()
        .map(|(rule, firewall)| ForwardRuleView { rule, firewall })
        .collect::<Vec<_>>();
    Ok(json!({ "items": items, "count": items.len() }))
}

fn execute_create_forward_rule(engine: &Arc<RuleEngine>, arguments: Value) -> Result<Value> {
    let args: CreateForwardRuleArgs = serde_json::from_value(arguments)?;
    ensure_forward_rule_type(args.rule_type)?;

    let req = CreateRuleRequest {
        rule: NewProxyRule {
            name: args.name,
            rule_type: args.rule_type,
            listen_host: args
                .listen_host
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "0.0.0.0".to_owned()),
            listen_port: args.listen_port,
            target_kind: args.target_kind,
            target_ref: clean_optional(args.target_ref),
            target_host: clean_optional(args.target_host),
            target_port: Some(args.target_port),
            bind_mode: args
                .bind_mode
                .unwrap_or(wsl_bridge_shared::BindMode::AllNics),
            nic_id: clean_optional(args.nic_id),
            enabled: args.enabled.unwrap_or(true),
        },
        firewall: Some(to_new_firewall_policy(args.firewall)),
    };

    let id = engine.create_rule(req)?;
    Ok(json!({
      "ok": true,
      "id": id,
      "requiresApplyInDesktopApp": true
    }))
}

fn execute_update_forward_rule(engine: &Arc<RuleEngine>, arguments: Value) -> Result<Value> {
    let args: UpdateForwardRuleArgs = serde_json::from_value(arguments)?;
    let id = args.id.clone();
    ensure_forward_rule_id(engine, &id)?;

    let patch = RulePatch {
        name: args.name.map(|value| value.trim().to_owned()),
        listen_host: args.listen_host.map(|value| value.trim().to_owned()),
        listen_port: args.listen_port,
        target_ref: args
            .target_ref
            .map(|value| value.map(|item| item.trim().to_owned())),
        target_host: args
            .target_host
            .map(|value| value.map(|item| item.trim().to_owned())),
        target_port: args.target_port,
        bind_mode: args.bind_mode,
        nic_id: args
            .nic_id
            .map(|value| value.map(|item| item.trim().to_owned())),
        enabled: args.enabled,
    };

    engine.update_rule(&id, patch)?;
    if let Some(firewall) = args.firewall {
        engine.update_firewall_policy(&id, to_new_firewall_policy(Some(firewall)))?;
    }

    Ok(json!({
      "ok": true,
      "id": id,
      "requiresApplyInDesktopApp": true
    }))
}

fn execute_delete_forward_rule(engine: &Arc<RuleEngine>, arguments: Value) -> Result<Value> {
    let args: DeleteForwardRuleArgs = serde_json::from_value(arguments)?;
    ensure_forward_rule_id(engine, &args.id)?;
    engine.delete_rule(&args.id)?;
    Ok(json!({
      "ok": true,
      "id": args.id,
      "requiresApplyInDesktopApp": true
    }))
}

fn execute_set_forward_rule_enabled(engine: &Arc<RuleEngine>, arguments: Value) -> Result<Value> {
    let args: ToggleForwardRuleArgs = serde_json::from_value(arguments)?;
    ensure_forward_rule_id(engine, &args.id)?;
    engine.enable_rule(&args.id, args.enabled)?;
    Ok(json!({
      "ok": true,
      "id": args.id,
      "enabled": args.enabled,
      "requiresApplyInDesktopApp": true
    }))
}

fn execute_query_traffic_stats(engine: &Arc<RuleEngine>, arguments: Value) -> Result<Value> {
    let args: QueryTrafficStatsArgs = serde_json::from_value(arguments)?;
    let result = engine.query_traffic_stats(QueryTrafficStatsRequest {
        entity_type: args.entity_type.unwrap_or(wsl_bridge_shared::TrafficEntityType::LegacyRule),
        entity_id: args.entity_id.unwrap_or(args.rule_id),
        start_time: args.start_time,
        end_time: args.end_time,
        interval: args.interval,
    });
    Ok(serde_json::to_value(result)?)
}

fn execute_get_traffic_window(engine: &Arc<RuleEngine>, arguments: Value) -> Result<Value> {
    let args: GetTrafficWindowArgs = serde_json::from_value(arguments)?;
    let result = engine.get_traffic_window_data(vec![wsl_bridge_shared::TrafficWindowQueryEntity {
        entity_type: args.entity_type.unwrap_or(wsl_bridge_shared::TrafficEntityType::LegacyRule),
        entity_id: args.entity_id.unwrap_or(args.rule_id),
    }]);
    Ok(json!({
      "items": result
    }))
}

fn ensure_forward_rule_type(rule_type: RuleType) -> Result<()> {
    if matches!(rule_type, RuleType::TcpFwd | RuleType::UdpFwd) {
        Ok(())
    } else {
        Err(anyhow!("only tcp_fwd and udp_fwd are supported by MCP"))
    }
}

fn ensure_forward_rule_id(engine: &Arc<RuleEngine>, id: &str) -> Result<()> {
    let rule = engine
        .list_rules()
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| anyhow!("rule not found: {id}"))?;
    ensure_forward_rule_type(rule.rule_type)
}

fn to_new_firewall_policy(value: Option<FirewallArgs>) -> NewFirewallPolicy {
    let value = value.unwrap_or(FirewallArgs {
        allow_domain: Some(true),
        allow_private: Some(true),
        allow_public: Some(false),
        direction: None,
        action: None,
    });
    NewFirewallPolicy {
        allow_domain: value.allow_domain.unwrap_or(true),
        allow_private: value.allow_private.unwrap_or(true),
        allow_public: value.allow_public.unwrap_or(false),
        direction: value.direction,
        action: value.action,
    }
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

fn topology_to_value(topology: TopologySnapshot, include_adapters: bool) -> Value {
    if include_adapters {
        json!(topology)
    } else {
        json!({
          "wsl": topology.wsl,
          "hyperv": topology.hyperv,
          "hypervError": topology.hyperv_error,
          "timestamp": topology.timestamp
        })
    }
}

fn inspect_rules(engine: &Arc<RuleEngine>, detail: &str) -> Value {
    let rules = engine.list_rules();
    let total = rules.len();
    let enabled = rules.iter().filter(|rule| rule.enabled).count();
    let tcp_fwd = rules
        .iter()
        .filter(|rule| matches!(rule.rule_type, RuleType::TcpFwd))
        .count();
    let udp_fwd = rules
        .iter()
        .filter(|rule| matches!(rule.rule_type, RuleType::UdpFwd))
        .count();
    let http_proxy = rules
        .iter()
        .filter(|rule| matches!(rule.rule_type, RuleType::HttpProxy))
        .count();

    if detail == "full" || detail == "diagnostic" {
        json!({
          "legacyMode": true,
          "total": total,
          "enabled": enabled,
          "byType": {
            "tcp_fwd": tcp_fwd,
            "udp_fwd": udp_fwd,
            "http_proxy": http_proxy
          },
          "allowedCreateTypes": ["udp_fwd", "socks5_proxy"],
          "migratableTypes": ["tcp_fwd", "http_proxy"],
          "items": rules
        })
    } else {
        json!({
          "legacyMode": true,
          "total": total,
          "enabled": enabled,
          "byType": {
            "tcp_fwd": tcp_fwd,
            "udp_fwd": udp_fwd,
            "http_proxy": http_proxy
          },
          "allowedCreateTypes": ["udp_fwd", "socks5_proxy"],
          "migratableTypes": ["tcp_fwd", "http_proxy"]
        })
    }
}

fn validate_config_patch_shape(
    patch: &Value,
    errors: &mut Vec<Value>,
    warnings: &mut Vec<Value>,
) {
    let Some(object) = patch.as_object() else {
        errors.push(json!({
          "code": "PATCH_NOT_OBJECT",
          "target": "patch",
          "message": "ConfigPatch must be a JSON object."
        }));
        return;
    };

    match object.get("version").and_then(Value::as_str) {
        Some(CONFIG_PATCH_VERSION) => {}
        Some(other) => errors.push(json!({
          "code": "PATCH_VERSION_UNSUPPORTED",
          "target": "patch.version",
          "message": format!("Unsupported ConfigPatch version: {other}")
        })),
        None => errors.push(json!({
          "code": "PATCH_VERSION_REQUIRED",
          "target": "patch.version",
          "message": "ConfigPatch version is required."
        })),
    }

    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "version" | "reason" | "proxy" | "hosts" | "rules" | "settings"
        ) {
            errors.push(json!({
              "code": "PATCH_UNKNOWN_TOP_LEVEL_FIELD",
              "target": key,
              "message": format!("Unknown ConfigPatch top-level field: {key}")
            }));
        }
    }

    for key in ["proxy", "hosts", "rules", "settings"] {
        if let Some(value) = object.get(key) {
            if !value.is_object() {
                errors.push(json!({
                  "code": "PATCH_SECTION_NOT_OBJECT",
                  "target": key,
                  "message": format!("ConfigPatch section `{key}` must be an object.")
                }));
            }
        }
    }

    if object.contains_key("hosts") {
        warnings.push(json!({
          "severity": "warning",
          "code": "HOSTS_MAY_REQUIRE_ADMIN",
          "target": "hosts",
          "message": "Hosts activation and system hosts writes require administrator privileges."
        }));
    }

    if object.contains_key("proxy") {
        warnings.push(json!({
          "severity": "info",
          "code": "PROXY_PATCH_SCHEMA_DRAFT",
          "target": "proxy",
          "message": "Proxy ConfigPatch detailed schema is still draft in this build."
        }));
    }
}

fn validate_legacy_rule_listen_conflicts(engine: &Arc<RuleEngine>, warnings: &mut Vec<Value>) {
    let mut seen: HashMap<(String, u16), Vec<String>> = HashMap::new();
    for rule in engine.list_rules() {
        if !matches!(rule.rule_type, RuleType::TcpFwd | RuleType::UdpFwd) {
            continue;
        }
        let target_port = rule.target_port.unwrap_or_default();
        if rule.listen_port == 0 || target_port == 0 {
            warnings.push(json!({
              "severity": "warning",
              "code": "LEGACY_RULE_PORT_INCOMPLETE",
              "target": rule.id,
              "message": format!("Legacy rule `{}` has incomplete listen or target port.", rule.name)
            }));
        }
        seen.entry((rule.listen_host.clone(), rule.listen_port))
            .or_default()
            .push(rule.name);
    }

    for ((host, port), names) in seen {
        if names.len() > 1 {
            warnings.push(json!({
              "severity": "warning",
              "code": "LEGACY_RULE_LISTEN_CONFLICT",
              "target": format!("{host}:{port}"),
              "message": format!("Multiple legacy rules use {host}:{port}: {}", names.join(", "))
            }));
        }
    }
}

fn skill_manifest_summary() -> Value {
    json!({
      "id": "wsl-bridge-operator",
      "name": "wsl-bridge Operator",
      "version": "0.1.0",
      "requiresWslBridgeAiApi": AI_API_VERSION,
      "canonicalPackage": "skills/wsl-bridge-operator"
    })
}

fn agent_targets() -> Vec<&'static str> {
    vec![
        "claude-code",
        "codex",
        "cursor",
        "copilot",
        "opencode",
        "openclaw",
        "generic",
    ]
}

fn normalize_install_scope(scope: Option<&str>) -> &'static str {
    match scope {
        Some("user") => "user",
        _ => "project",
    }
}

fn normalize_agent_target(target: &str) -> String {
    match target.trim().to_ascii_lowercase().as_str() {
        "claude" | "claude_code" | "claude-code" => "claude-code".to_owned(),
        "github-copilot" | "copilot" => "copilot".to_owned(),
        "open-code" | "opencode" => "opencode".to_owned(),
        "open-claw" | "openclaw" => "openclaw".to_owned(),
        "cursor" => "cursor".to_owned(),
        "codex" => "codex".to_owned(),
        _ => "generic".to_owned(),
    }
}

fn agent_target_descriptor(target: &str, scope: &str) -> Value {
    let install_type = match target {
        "claude-code" => "native-skill",
        "cursor" => "project-rule",
        "copilot" => "repository-instructions",
        _ => "generic-project-skill",
    };
    json!({
      "id": target,
      "displayName": agent_target_display_name(target),
      "scope": scope,
      "detected": "unknown",
      "supportsNativeSkill": target == "claude-code",
      "supportsProjectInstall": true,
      "supportsUserInstall": target == "claude-code",
      "installType": install_type,
      "fallbackToAgentsDir": !matches!(target, "claude-code" | "cursor" | "copilot"),
      "dryRunSupported": true,
      "applySupported": false
    })
}

fn agent_target_display_name(target: &str) -> &'static str {
    match target {
        "claude-code" => "Claude Code",
        "codex" => "Codex",
        "cursor" => "Cursor",
        "copilot" => "Copilot",
        "opencode" => "OpenCode",
        "openclaw" => "OpenClaw",
        _ => "Generic .agents",
    }
}

fn build_agent_skill_install_plan(
    target: &str,
    scope: &str,
    fallback_to_agents_dir: bool,
) -> Result<Value> {
    let install_type = match target {
        "claude-code" => "native-skill",
        "cursor" if scope == "project" => "project-rule",
        "copilot" if scope == "project" => "repository-instructions",
        "cursor" | "copilot" if fallback_to_agents_dir => "generic-project-skill",
        "codex" | "opencode" | "openclaw" | "generic" if fallback_to_agents_dir => {
            "generic-project-skill"
        }
        "codex" | "opencode" | "openclaw" | "generic" => "manual-package",
        _ => "generic-project-skill",
    };

    let writes = match install_type {
        "native-skill" => canonical_skill_file_paths(&format!(
            "{}/skills/wsl-bridge-operator",
            if scope == "user" { "~/.claude" } else { ".claude" }
        )),
        "project-rule" => vec![json!({
          "path": ".cursor/rules/wsl-bridge.mdc",
          "action": "create-or-update",
          "source": "rendered-cursor-rule"
        })],
        "repository-instructions" => vec![json!({
          "path": ".github/copilot-instructions.md",
          "action": "create-or-update",
          "source": "rendered-copilot-instructions"
        })],
        "generic-project-skill" => canonical_skill_file_paths(
            ".agents/skills/wsl-bridge-operator",
        ),
        _ => canonical_skill_file_paths("wsl-bridge-operator-skill"),
    };

    Ok(json!({
      "ok": true,
      "mode": "dryRun",
      "skill": skill_manifest_summary(),
      "targetAgent": target,
      "scope": scope,
      "installType": install_type,
      "writes": writes,
      "warnings": agent_install_warnings(target, scope, install_type)
    }))
}

fn canonical_skill_file_paths(base: &str) -> Vec<Value> {
    [
        "SKILL.md",
        "manifest.json",
        "references/concepts.md",
        "references/proxy-recipes.md",
        "references/hosts-recipes.md",
        "references/rules-legacy.md",
        "references/troubleshooting.md",
        "references/patch-schema.md",
        "references/safety.md",
    ]
    .iter()
    .map(|path| {
        json!({
          "path": format!("{base}/{path}"),
          "action": "create-or-update",
          "source": format!("skills/wsl-bridge-operator/{path}")
        })
    })
    .collect()
}

fn agent_install_warnings(target: &str, scope: &str, install_type: &str) -> Vec<Value> {
    let mut warnings = Vec::new();
    warnings.push(json!({
      "severity": "info",
      "code": "DRY_RUN_ONLY",
      "message": "This preview does not write files. install_agent_skill apply is not enabled yet."
    }));
    if install_type == "generic-project-skill" {
        warnings.push(json!({
          "severity": "warning",
          "code": "GENERIC_SKILL_FALLBACK",
          "message": "The skill will be installed to the project-level .agents fallback path when apply becomes available."
        }));
    }
    if scope == "user" {
        warnings.push(json!({
          "severity": "warning",
          "code": "USER_SCOPE_AFFECTS_ALL_PROJECTS",
          "message": "User-scope installation can affect multiple projects used by the target Agent."
        }));
    }
    if matches!(target, "cursor" | "copilot") {
        warnings.push(json!({
          "severity": "info",
          "code": "ADAPTED_INSTRUCTIONS",
          "message": "This target uses an adapted rule or instructions file rather than a native Skill runtime."
        }));
    }
    warnings
}

fn ai_guide_resource() -> &'static str {
    r#"# wsl-bridge AI Guide

wsl-bridge is a Windows desktop app for managing WSL / Hyper-V bridge rules, reverse proxy configuration, structured Hosts groups, runtime status, and traffic monitoring.

Recommended AI workflow:

1. Read `wsl-bridge://capabilities` and `wsl-bridge://state/summary`.
2. Inspect module-specific state before suggesting changes.
3. Represent complex writes as `ConfigPatch`.
4. Dry-run patches before apply.
5. Explain warnings to the user.
6. Validate configuration and connectivity after changes.

Current Phase3 AI API status:

- Resources are available for discovery and read-only context.
- Existing legacy MCP tools remain available according to the user's exposed capability toggles.
- `ConfigPatch` is documented as a draft schema; apply support is not enabled in this build.
- Rules is a legacy module. New `tcp_fwd` and `http_proxy` rules should migrate to Proxy instead of being created in Rules.
"#
}

fn capabilities_resource(config: &McpServerConfig) -> Value {
    json!({
      "aiApiVersion": AI_API_VERSION,
      "configPatchVersion": CONFIG_PATCH_VERSION,
      "server": {
        "name": config.server_name,
        "enabled": config.enabled,
        "listenPort": config.listen_port
      },
      "resources": [
        "wsl-bridge://ai-guide",
        "wsl-bridge://capabilities",
        "wsl-bridge://state/summary",
        "wsl-bridge://state/proxy",
        "wsl-bridge://state/hosts",
        "wsl-bridge://state/traffic",
        "wsl-bridge://logs/recent",
        "wsl-bridge://schemas/config-patch"
      ],
      "tools": {
        "legacyTools": build_tool_definitions(config),
        "configPatch": {
          "dryRun": false,
          "apply": false,
          "status": "planned"
        },
        "agentSkill": {
          "listTargets": true,
          "installDryRun": true,
          "installApply": false,
          "genericFallback": ".agents/skills/wsl-bridge-operator"
        }
      },
      "safety": {
        "defaultMode": "planning",
        "writesRequireConfirmation": true,
        "sensitiveOperations": [
          "system-hosts-write",
          "hosts-group-activation",
          "proxy-object-delete",
          "listener-0.0.0.0",
          "agent-skill-install",
          "config-import-overwrite"
        ]
      }
    })
}

fn state_summary_resource(engine: &Arc<RuleEngine>, config: &McpServerConfig) -> Value {
    let rules = engine.list_rules();
    let proxy_listeners = engine.list_proxy_listeners();
    let proxy_routes = proxy_listeners
        .iter()
        .map(|listener| engine.list_proxy_routes(&listener.id).unwrap_or_default().len())
        .sum::<usize>();
    let proxy_upstreams = proxy_listeners
        .iter()
        .flat_map(|listener| engine.list_proxy_routes(&listener.id).unwrap_or_default())
        .map(|route| engine.list_proxy_upstreams(&route.id).unwrap_or_default().len())
        .sum::<usize>();
    let hosts_groups = engine.list_hosts_groups();
    let hosts_entries = hosts_groups
        .iter()
        .map(|group| engine.list_hosts_entries(&group.id).unwrap_or_default().len())
        .sum::<usize>();
    let legacy_forward_rules = rules
        .iter()
        .filter(|rule| matches!(rule.rule_type, RuleType::TcpFwd | RuleType::UdpFwd))
        .count();
    let migratable_rules = rules
        .iter()
        .filter(|rule| matches!(rule.rule_type, RuleType::TcpFwd | RuleType::HttpProxy))
        .count();

    json!({
      "app": {
        "name": "wsl-bridge",
        "aiApiVersion": AI_API_VERSION
      },
      "mcp": {
        "enabled": config.enabled,
        "serverName": config.server_name,
        "listenPort": config.listen_port,
        "exposedTools": build_tool_definitions(config).len()
      },
      "rules": {
        "legacyMode": true,
        "total": rules.len(),
        "legacyForwardRules": legacy_forward_rules,
        "migratable": migratable_rules,
        "allowedCreateTypes": ["udp_fwd", "socks5_proxy"]
      },
      "proxy": {
        "listeners": proxy_listeners.len(),
        "routes": proxy_routes,
        "upstreams": proxy_upstreams,
        "certificates": engine.list_proxy_certificates().len(),
        "resource": "wsl-bridge://state/proxy"
      },
      "hosts": {
        "groups": hosts_groups.len(),
        "entries": hosts_entries,
        "activeGroup": hosts_groups.iter().find(|group| group.is_active).map(|group| group.name.clone()),
        "requiresAdminForSystemApply": true,
        "resource": "wsl-bridge://state/hosts"
      },
      "configPatch": {
        "version": CONFIG_PATCH_VERSION,
        "dryRun": "planned",
        "apply": "planned"
      }
    })
}

fn state_proxy_resource(engine: &Arc<RuleEngine>) -> Value {
    let listeners = engine.list_proxy_listeners();
    let certificates = engine.list_proxy_certificates();
    let runtime = engine.get_proxy_runtime_status();
    let mut route_count = 0usize;
    let mut upstream_count = 0usize;
    let mut enabled_routes = 0usize;
    let mut enabled_upstreams = 0usize;
    let mut route_runtime_total_hits = 0u64;
    let mut route_runtime_total_errors = 0u64;
    let mut upstream_runtime_total_hits = 0u64;
    let mut upstream_runtime_total_errors = 0u64;
    let mut topology = Vec::new();

    for listener in listeners.iter() {
        let routes = engine.list_proxy_routes(&listener.id).unwrap_or_default();
        let route_runtime = engine.list_proxy_route_runtime(&listener.id);
        route_runtime_total_hits += route_runtime.iter().map(|item| item.hit_count).sum::<u64>();
        route_runtime_total_errors += route_runtime.iter().map(|item| item.error_count).sum::<u64>();
        route_count += routes.len();
        enabled_routes += routes.iter().filter(|route| route.enabled).count();

        let route_items = routes
            .iter()
            .map(|route| {
                let upstreams = engine.list_proxy_upstreams(&route.id).unwrap_or_default();
                let upstream_runtime = engine.list_proxy_upstream_runtime(&route.id);
                upstream_runtime_total_hits += upstream_runtime.iter().map(|item| item.hit_count).sum::<u64>();
                upstream_runtime_total_errors += upstream_runtime.iter().map(|item| item.error_count).sum::<u64>();
                upstream_count += upstreams.len();
                enabled_upstreams += upstreams.iter().filter(|upstream| upstream.enabled).count();
                json!({
                  "route": route,
                  "runtime": route_runtime.iter().find(|item| item.route_id == route.id),
                  "upstreams": upstreams,
                  "upstreamRuntime": upstream_runtime
                })
            })
            .collect::<Vec<_>>();

        topology.push(json!({
          "listener": listener,
          "runtime": runtime.iter().find(|item| item.listener_id == listener.id),
          "routes": route_items
        }));
    }

    json!({
      "summary": {
        "listeners": listeners.len(),
        "enabledListeners": listeners.iter().filter(|listener| listener.enabled).count(),
        "routes": route_count,
        "enabledRoutes": enabled_routes,
        "upstreams": upstream_count,
        "enabledUpstreams": enabled_upstreams,
        "certificates": certificates.len(),
        "runtime": {
          "listeners": runtime.len(),
          "running": runtime.iter().filter(|item| matches!(item.state, wsl_bridge_shared::RuntimeState::Running)).count(),
          "stopped": runtime.iter().filter(|item| matches!(item.state, wsl_bridge_shared::RuntimeState::Stopped)).count(),
          "error": runtime.iter().filter(|item| matches!(item.state, wsl_bridge_shared::RuntimeState::Error)).count()
        },
        "metrics": {
          "routeHits": route_runtime_total_hits,
          "routeErrors": route_runtime_total_errors,
          "upstreamHits": upstream_runtime_total_hits,
          "upstreamErrors": upstream_runtime_total_errors
        }
      },
      "certificates": certificates,
      "topology": topology
    })
}

fn state_hosts_resource(engine: &Arc<RuleEngine>) -> Value {
    let groups = engine.list_hosts_groups();
    let mut total_entries = 0usize;
    let mut enabled_entries = 0usize;
    let group_items = groups
        .iter()
        .map(|group| {
            let entries = engine.list_hosts_entries(&group.id).unwrap_or_default();
            let enabled = entries.iter().filter(|entry| entry.enabled).count();
            total_entries += entries.len();
            enabled_entries += enabled;
            json!({
              "group": group,
              "entries": {
                "total": entries.len(),
                "enabled": enabled,
                "disabled": entries.len().saturating_sub(enabled)
              }
            })
        })
        .collect::<Vec<_>>();
    let active_group = groups.iter().find(|group| group.is_active);

    json!({
      "summary": {
        "groups": groups.len(),
        "entries": total_entries,
        "enabledEntries": enabled_entries,
        "disabledEntries": total_entries.saturating_sub(enabled_entries),
        "activeGroup": active_group.map(|group| json!({
          "id": group.id,
          "name": group.name,
          "updatedAt": group.updated_at
        })),
        "requiresAdminForSystemApply": true
      },
      "groups": group_items,
      "notes": [
        "Only one Hosts group can be active at a time.",
        "Activating a group writes the system hosts file and requires administrator privileges."
      ]
    })
}

fn state_traffic_resource(engine: &Arc<RuleEngine>) -> Value {
    let entities = engine.list_traffic_monitor_entities();
    let queries = entities
        .iter()
        .map(|entity| wsl_bridge_shared::TrafficWindowQueryEntity {
            entity_type: entity.entity_type,
            entity_id: entity.entity_id.clone(),
        })
        .collect::<Vec<_>>();
    let windows = engine.get_traffic_window_data(queries);
    let mut total_bytes_in = 0u64;
    let mut total_bytes_out = 0u64;
    let mut total_connections = 0u64;
    let mut active_series = 0usize;

    let series = windows
        .iter()
        .map(|window| {
            let bytes_in = window.samples.iter().map(|sample| sample.bytes_in).sum::<u64>();
            let bytes_out = window.samples.iter().map(|sample| sample.bytes_out).sum::<u64>();
            let connections = window
                .samples
                .iter()
                .map(|sample| sample.connections)
                .sum::<u64>();
            let latest_timestamp = window.samples.iter().map(|sample| sample.timestamp).max();
            if bytes_in > 0 || bytes_out > 0 || connections > 0 {
                active_series += 1;
            }
            total_bytes_in += bytes_in;
            total_bytes_out += bytes_out;
            total_connections += connections;

            let label = entities
                .iter()
                .find(|entity| {
                    entity.entity_type == window.entity_type && entity.entity_id == window.entity_id
                })
                .map(|entity| entity.label.clone())
                .unwrap_or_else(|| window.entity_id.clone());

            json!({
              "entityType": window.entity_type,
              "entityId": window.entity_id,
              "label": label,
              "samples": window.samples.len(),
              "latestTimestamp": latest_timestamp,
              "bytesIn": bytes_in,
              "bytesOut": bytes_out,
              "connections": connections
            })
        })
        .collect::<Vec<_>>();

    json!({
      "summary": {
        "entities": entities.len(),
        "enabledEntities": entities.iter().filter(|entity| entity.enabled).count(),
        "legacyRuleEntities": entities.iter().filter(|entity| matches!(entity.entity_type, wsl_bridge_shared::TrafficEntityType::LegacyRule)).count(),
        "proxyUpstreamEntities": entities.iter().filter(|entity| matches!(entity.entity_type, wsl_bridge_shared::TrafficEntityType::ProxyUpstream)).count(),
        "activeSeries": active_series,
        "windowTotals": {
          "bytesIn": total_bytes_in,
          "bytesOut": total_bytes_out,
          "connections": total_connections
        }
      },
      "entities": entities,
      "series": series
    })
}

fn recent_logs_resource(engine: &Arc<RuleEngine>) -> Value {
    let result = engine.query_logs(LogQueryRequest {
        limit: Some(80),
        newest_first: Some(true),
        ..Default::default()
    });
    let errors = result
        .events
        .iter()
        .filter(|event| event.level.eq_ignore_ascii_case("error"))
        .count();
    let warnings = result
        .events
        .iter()
        .filter(|event| event.level.eq_ignore_ascii_case("warn") || event.level.eq_ignore_ascii_case("warning"))
        .count();
    let modules = result.events.iter().fold(
        std::collections::BTreeMap::<String, usize>::new(),
        |mut acc, event| {
            *acc.entry(event.module.clone()).or_insert(0) += 1;
            acc
        },
    );

    json!({
      "summary": {
        "totalMatched": result.total,
        "returned": result.events.len(),
        "errors": errors,
        "warnings": warnings,
        "modules": modules
      },
      "events": result.events
    })
}

fn config_patch_schema_resource() -> Value {
    json!({
      "$schema": "https://json-schema.org/draft/2020-12/schema",
      "title": "wsl-bridge ConfigPatch",
      "type": "object",
      "required": ["version"],
      "properties": {
        "version": {
          "const": CONFIG_PATCH_VERSION
        },
        "reason": {
          "type": "string"
        },
        "proxy": {
          "type": "object",
          "description": "Proxy listener / route / upstream changes. Detailed schema is planned."
        },
        "hosts": {
          "type": "object",
          "description": "Hosts group / record changes. Detailed schema is planned."
        },
        "rules": {
          "type": "object",
          "description": "Legacy Rules migration and limited legacy-rule changes. Detailed schema is planned."
        },
        "settings": {
          "type": "object",
          "description": "Application or AI integration settings. Detailed schema is planned."
        }
      },
      "additionalProperties": false
    })
}

fn build_tool_definitions(config: &McpServerConfig) -> Vec<Value> {
    let mut tools = Vec::new();
    tools.push(json!({
      "name": "inspect_app",
      "description": "Inspect wsl-bridge AI API state and selected module summaries without changing configuration.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "modules": {
            "type": "array",
            "items": {
              "type": "string",
              "enum": ["summary", "rules", "proxy", "hosts", "traffic"]
            }
          },
          "detail": {
            "type": "string",
            "enum": ["summary", "full", "diagnostic"]
          }
        }
      }
    }));
    tools.push(json!({
      "name": "validate_config",
      "description": "Validate current configuration or a draft ConfigPatch without applying changes.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "modules": {
            "type": "array",
            "items": {
              "type": "string",
              "enum": ["summary", "rules", "proxy", "hosts", "traffic"]
            }
          },
          "patch": {
            "type": "object",
            "description": "Draft ConfigPatch to validate."
          },
          "checks": {
            "type": "array",
            "items": {
              "type": "string",
              "enum": ["schema", "conflict", "permission", "reachability", "runtime"]
            }
          }
        }
      }
    }));
    tools.push(json!({
      "name": "list_agent_targets",
      "description": "List supported Agent skill installation targets and dry-run capabilities.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "scope": {
            "type": "string",
            "enum": ["project", "user"]
          }
        }
      }
    }));
    tools.push(json!({
      "name": "install_agent_skill",
      "description": "Preview installation of the wsl-bridge-operator skill for a target Agent. Only mode=dryRun is supported.",
      "inputSchema": {
        "type": "object",
        "required": ["target"],
        "properties": {
          "target": {
            "type": "string",
            "enum": ["claude-code", "codex", "cursor", "copilot", "opencode", "openclaw", "generic"]
          },
          "scope": {
            "type": "string",
            "enum": ["project", "user"]
          },
          "mode": {
            "type": "string",
            "enum": ["dryRun"]
          },
          "fallbackToAgentsDir": {
            "type": "boolean",
            "description": "Use project-level .agents/skills fallback when native installation is unavailable."
          }
        }
      }
    }));
    if config.expose_topology_read {
        tools.push(json!({
          "name": "read_virtualization_topology",
          "description": "Read current WSL and Hyper-V topology, networking mode and resolved IP information.",
          "inputSchema": {
            "type": "object",
            "properties": {
              "includeAdapters": {
                "type": "boolean",
                "description": "Include Windows adapter information in the response."
              }
            }
          }
        }));
    }
    if config.expose_rule_config {
        tools.push(json!({
          "name": "list_forward_rules",
          "description": "List configured TCP/UDP forward rules together with firewall profile settings.",
          "inputSchema": {
            "type": "object",
            "properties": {}
          }
        }));
        tools.push(json!({
          "name": "create_forward_rule",
          "description": "Create a new TCP or UDP forward rule. Changes are persisted immediately and still require applying rules in the desktop app.",
          "inputSchema": create_forward_rule_schema()
        }));
        tools.push(json!({
          "name": "update_forward_rule",
          "description": "Update an existing TCP or UDP forward rule. Changes are persisted immediately and still require applying rules in the desktop app.",
          "inputSchema": update_forward_rule_schema()
        }));
        tools.push(json!({
          "name": "delete_forward_rule",
          "description": "Delete a forward rule by id. Changes are persisted immediately and still require applying rules in the desktop app.",
          "inputSchema": {
            "type": "object",
            "required": ["id"],
            "properties": {
              "id": { "type": "string" }
            }
          }
        }));
        tools.push(json!({
          "name": "set_forward_rule_enabled",
          "description": "Enable or disable a forward rule. Changes are persisted immediately and still require applying rules in the desktop app.",
          "inputSchema": {
            "type": "object",
            "required": ["id", "enabled"],
            "properties": {
              "id": { "type": "string" },
              "enabled": { "type": "boolean" }
            }
          }
        }));
    }
    if config.expose_traffic_stats {
        tools.push(json!({
          "name": "query_traffic_stats",
          "description": "Query minute-level traffic statistics for a single rule within a time range.",
          "inputSchema": {
            "type": "object",
            "required": ["ruleId"],
            "properties": {
              "ruleId": { "type": "string" },
              "startTime": { "type": "string", "format": "date-time" },
              "endTime": { "type": "string", "format": "date-time" },
              "interval": { "type": "string", "enum": ["minute"] }
            }
          }
        }));
        tools.push(json!({
          "name": "get_traffic_window",
          "description": "Get the in-memory real-time traffic window for a single rule.",
          "inputSchema": {
            "type": "object",
            "required": ["ruleId"],
            "properties": {
              "ruleId": { "type": "string" }
            }
          }
        }));
    }
    tools
}

fn create_forward_rule_schema() -> Value {
    json!({
      "type": "object",
      "required": ["name", "type", "listenPort", "targetKind", "targetPort"],
      "properties": {
        "name": { "type": "string" },
        "type": { "type": "string", "enum": ["tcp_fwd", "udp_fwd"] },
        "listenHost": { "type": "string" },
        "listenPort": { "type": "integer", "minimum": 1, "maximum": 65535 },
        "targetKind": { "type": "string", "enum": ["wsl", "hyperv", "static"] },
        "targetRef": { "type": "string" },
        "targetHost": { "type": "string" },
        "targetPort": { "type": "integer", "minimum": 1, "maximum": 65535 },
        "bindMode": { "type": "string", "enum": ["single_nic", "all_nics"] },
        "nicId": { "type": "string" },
        "enabled": { "type": "boolean" },
        "firewall": firewall_schema()
      }
    })
}

fn update_forward_rule_schema() -> Value {
    json!({
      "type": "object",
      "required": ["id"],
      "properties": {
        "id": { "type": "string" },
        "name": { "type": "string" },
        "listenHost": { "type": "string" },
        "listenPort": { "type": "integer", "minimum": 1, "maximum": 65535 },
        "targetRef": { "type": ["string", "null"] },
        "targetHost": { "type": ["string", "null"] },
        "targetPort": { "type": ["integer", "null"], "minimum": 1, "maximum": 65535 },
        "bindMode": { "type": "string", "enum": ["single_nic", "all_nics"] },
        "nicId": { "type": ["string", "null"] },
        "enabled": { "type": "boolean" },
        "firewall": firewall_schema()
      }
    })
}

fn firewall_schema() -> Value {
    json!({
      "type": "object",
      "properties": {
        "allowDomain": { "type": "boolean" },
        "allowPrivate": { "type": "boolean" },
        "allowPublic": { "type": "boolean" },
        "direction": { "type": "string" },
        "action": { "type": "string" }
      }
    })
}

fn describe_tools(config: &McpServerConfig) -> Vec<McpToolDescriptor> {
    vec![
        McpToolDescriptor {
            name: "inspect_app".to_owned(),
            description_key: "mcpToolInspectApp".to_owned(),
            enabled: true,
        },
        McpToolDescriptor {
            name: "validate_config".to_owned(),
            description_key: "mcpToolValidateConfig".to_owned(),
            enabled: true,
        },
        McpToolDescriptor {
            name: "list_agent_targets".to_owned(),
            description_key: "mcpToolListAgentTargets".to_owned(),
            enabled: true,
        },
        McpToolDescriptor {
            name: "install_agent_skill".to_owned(),
            description_key: "mcpToolInstallAgentSkill".to_owned(),
            enabled: true,
        },
        McpToolDescriptor {
            name: "read_virtualization_topology".to_owned(),
            description_key: "mcpToolTopology".to_owned(),
            enabled: config.expose_topology_read,
        },
        McpToolDescriptor {
            name: "list_forward_rules".to_owned(),
            description_key: "mcpToolListRules".to_owned(),
            enabled: config.expose_rule_config,
        },
        McpToolDescriptor {
            name: "create_forward_rule".to_owned(),
            description_key: "mcpToolCreateRule".to_owned(),
            enabled: config.expose_rule_config,
        },
        McpToolDescriptor {
            name: "update_forward_rule".to_owned(),
            description_key: "mcpToolUpdateRule".to_owned(),
            enabled: config.expose_rule_config,
        },
        McpToolDescriptor {
            name: "delete_forward_rule".to_owned(),
            description_key: "mcpToolDeleteRule".to_owned(),
            enabled: config.expose_rule_config,
        },
        McpToolDescriptor {
            name: "set_forward_rule_enabled".to_owned(),
            description_key: "mcpToolToggleRule".to_owned(),
            enabled: config.expose_rule_config,
        },
        McpToolDescriptor {
            name: "query_traffic_stats".to_owned(),
            description_key: "mcpToolTrafficStats".to_owned(),
            enabled: config.expose_traffic_stats,
        },
        McpToolDescriptor {
            name: "get_traffic_window".to_owned(),
            description_key: "mcpToolTrafficWindow".to_owned(),
            enabled: config.expose_traffic_stats,
        },
    ]
}

fn build_client_presets(config: &McpServerConfig, base_url: &str) -> Vec<McpClientPreset> {
    let token = config.api_token.as_str();
    vec![
        McpClientPreset {
            id: "claude-code".to_owned(),
            label: "Claude Code".to_owned(),
            format: "bash".to_owned(),
            content: format!(
                "claude mcp add --scope user --transport http {name} {url} \\\n  --header \"Authorization: Bearer {token}\"",
                name = config.server_name,
                url = base_url
            ),
        },
        McpClientPreset {
            id: "codex".to_owned(),
            label: "Codex".to_owned(),
            format: "toml".to_owned(),
            content: format!(
                "[mcp_servers.{name}]\nurl = \"{url}\"\nhttp_headers = {{ \"Authorization\" = \"Bearer {token}\" }}",
                name = config.server_name,
                url = base_url
            ),
        },
        McpClientPreset {
            id: "opencode".to_owned(),
            label: "OpenCode".to_owned(),
            format: "json".to_owned(),
            content: serde_json::to_string_pretty(&json!({
              "mcp": {
                config.server_name.clone(): {
                  "type": "remote",
                  "url": base_url,
                  "headers": {
                    "Authorization": format!("Bearer {token}")
                  },
                  "enabled": true
                }
              }
            }))
            .unwrap_or_else(|_| "{}".to_owned()),
        },
        McpClientPreset {
            id: "cursor".to_owned(),
            label: "Cursor".to_owned(),
            format: "json".to_owned(),
            content: serde_json::to_string_pretty(&json!({
              "mcpServers": {
                config.server_name.clone(): {
                  "url": base_url,
                  "headers": {
                    "Authorization": format!("Bearer {token}")
                  }
                }
              }
            }))
            .unwrap_or_else(|_| "{}".to_owned()),
        },
    ]
}

fn generate_api_token() -> String {
    format!("wb_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn bind_listener(start_port: u16) -> Result<(TcpListener, u16)> {
    let mut port = start_port;
    loop {
        match bind_listener_on_port(port) {
            Ok(listener) => return Ok((listener, port)),
            Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
                if port == u16::MAX {
                    return Err(anyhow!("failed to bind http listener: no available port"));
                }
                port = port.saturating_add(1);
            }
            Err(err) => {
                return Err(anyhow!("failed to bind http://127.0.0.1:{port}: {err}"));
            }
        }
    }
}

fn bind_listener_on_port(port: u16) -> std::io::Result<TcpListener> {
    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;
    #[cfg(windows)]
    set_exclusive_address_use(&socket)?;
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    socket.bind(&addr.into())?;
    socket.listen(1024)?;
    Ok(socket.into())
}

#[cfg(windows)]
fn set_exclusive_address_use(socket: &Socket) -> std::io::Result<()> {
    let enabled: i32 = 1;
    let result = unsafe {
        setsockopt(
            socket.as_raw_socket() as usize,
            SOL_SOCKET,
            SO_EXCLUSIVEADDRUSE,
            &enabled as *const i32 as *const u8,
            std::mem::size_of::<i32>() as i32,
        )
    };
    if result == SOCKET_ERROR {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_exclusive_address_use(_socket: &Socket) -> std::io::Result<()> {
    Ok(())
}

fn is_authorized(request: &ParsedRequest, api_token: &str) -> bool {
    if api_token.trim().is_empty() {
        return false;
    }
    if let Some(value) = request.headers.get("authorization") {
        let expected = format!("bearer {}", api_token);
        if value.trim().eq_ignore_ascii_case(&expected) {
            return true;
        }
    }
    request
        .headers
        .get("x-api-token")
        .map(|value| value.trim() == api_token)
        .unwrap_or(false)
}

fn read_request(stream: &mut TcpStream) -> Result<ParsedRequest> {
    let mut buffer = Vec::new();
    let mut temp = [0u8; 1024];
    let header_end;
    loop {
        let n = stream.read(&mut temp)?;
        if n == 0 {
            return Err(anyhow!("connection closed before headers"));
        }
        buffer.extend_from_slice(&temp[..n]);
        if let Some(pos) = find_header_end(&buffer) {
            header_end = pos;
            break;
        }
        if buffer.len() > 1024 * 1024 {
            return Err(anyhow!("request too large"));
        }
    }

    let header_bytes = &buffer[..header_end];
    let mut body = buffer[(header_end + 4)..].to_vec();
    let header_text = String::from_utf8(header_bytes.to_vec())?;
    let mut lines = header_text.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| anyhow!("missing request line"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| anyhow!("missing method"))?
        .to_owned();
    let path = parts
        .next()
        .ok_or_else(|| anyhow!("missing path"))?
        .to_owned();
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }

    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    while body.len() < content_length {
        let n = stream.read(&mut temp)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&temp[..n]);
    }
    body.truncate(content_length);

    Ok(ParsedRequest {
        method,
        path,
        headers,
        body,
    })
}

fn write_http_response(
    stream: &mut TcpStream,
    status: u16,
    status_text: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Result<()> {
    let mut response = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        response.push_str(name);
        response.push_str(": ");
        response.push_str(value);
        response.push_str("\r\n");
    }
    response.push_str("\r\n");
    stream.write_all(response.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn jsonrpc_result(id: Option<Value>, result: Value) -> Value {
    json!({
      "jsonrpc": "2.0",
      "id": id.unwrap_or(Value::Null),
      "result": result
    })
}

fn jsonrpc_error(id: Option<Value>, code: i32, message: &str) -> Value {
    json!({
      "jsonrpc": "2.0",
      "id": id.unwrap_or(Value::Null),
      "error": {
        "code": code,
        "message": message
      }
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use serde_json::json;

    use super::MCP_PATH;
    use crate::state::AppState;
    use wsl_bridge_shared::McpServerConfig;

    fn temp_db_path(name: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("duration")
            .as_nanos();
        std::env::temp_dir().join(format!("wsl-bridge-app-{name}-{now}.db"))
    }

    fn setup_state(name: &str) -> (AppState, PathBuf) {
        let path = temp_db_path(name);
        let state = AppState::new_with_storage_path(path.clone());
        state.mcp_service.stop();
        (state, path)
    }

    fn cleanup_state(state: &AppState, path: PathBuf) {
        state.mcp_service.stop();
        let _ = fs::remove_file(path);
    }

    fn send_http_request(port: u16, token: Option<&str>, body: serde_json::Value) -> (u16, String) {
        let payload = serde_json::to_string(&body).expect("serialize body");
        let mut request = format!(
            "POST {MCP_PATH} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
            payload.len()
        );
        if let Some(token) = token {
            request.push_str(&format!("Authorization: Bearer {token}\r\n"));
        }
        request.push_str("\r\n");
        request.push_str(&payload);

        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("read timeout");
        stream.write_all(request.as_bytes()).expect("write request");
        stream
            .shutdown(std::net::Shutdown::Write)
            .expect("shutdown write");

        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read response");
        let (head, body) = response
            .split_once("\r\n\r\n")
            .expect("http response separator");
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse::<u16>().ok())
            .expect("status code");
        (status, body.to_owned())
    }

    #[test]
    fn mcp_port_conflict_auto_increments() {
        let blocker = TcpListener::bind(("127.0.0.1", 0)).expect("bind blocker");
        let blocked_port = blocker.local_addr().expect("local addr").port();

        let (state, path) = setup_state("mcp-port-conflict");
        let config = McpServerConfig {
            enabled: true,
            server_name: "wsl-bridge".to_owned(),
            listen_port: blocked_port,
            api_token: "test-token".to_owned(),
            expose_topology_read: true,
            expose_rule_config: true,
            expose_traffic_stats: true,
        };

        state
            .engine
            .update_mcp_config(config.clone())
            .expect("save config");
        state.mcp_service.apply_config(&config);
        thread::sleep(Duration::from_millis(150));

        let updated = state.engine.get_mcp_config();
        assert!(updated.listen_port > blocked_port);
        assert!(state.mcp_service.is_running());

        cleanup_state(&state, path);
        drop(blocker);
    }

    #[test]
    fn mcp_http_bearer_token_auth_works() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind temp");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        let (state, path) = setup_state("mcp-auth");
        let config = McpServerConfig {
            enabled: true,
            server_name: "wsl-bridge".to_owned(),
            listen_port: port,
            api_token: "secret-token".to_owned(),
            expose_topology_read: true,
            expose_rule_config: true,
            expose_traffic_stats: true,
        };

        state
            .engine
            .update_mcp_config(config.clone())
            .expect("save config");
        state.mcp_service.apply_config(&config);
        thread::sleep(Duration::from_millis(150));
        let actual_port = state.engine.get_mcp_config().listen_port;

        let request = json!({
          "jsonrpc": "2.0",
          "id": 1,
          "method": "tools/list",
          "params": {}
        });

        let (unauthorized_status, unauthorized_body) =
            send_http_request(actual_port, None, request.clone());
        assert_eq!(unauthorized_status, 401);
        assert!(unauthorized_body.contains("unauthorized"));

        let (authorized_status, authorized_body) =
            send_http_request(actual_port, Some("secret-token"), request);
        assert_eq!(authorized_status, 200);
        assert!(authorized_body.contains("\"result\""));
        assert!(authorized_body.contains("read_virtualization_topology"));

        cleanup_state(&state, path);
    }
}
