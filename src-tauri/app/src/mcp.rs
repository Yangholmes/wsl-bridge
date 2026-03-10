use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;
use wsl_bridge_core::RuleEngine;
use wsl_bridge_shared::{
    CreateRuleRequest, FirewallPolicy, McpClientPreset, McpServerConfig, McpServerStatus,
    McpToolDescriptor, NewFirewallPolicy, NewProxyRule, ProxyRule, RulePatch, RuleType,
    TargetKind, TopologySnapshot,
};

use crate::state::AppState;

const MCP_PATH: &str = "/mcp";
const HEALTH_PATH: &str = "/health";
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-03-26", "2024-11-05"];
const DEFAULT_PROTOCOL_VERSION: &str = "2025-03-26";

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
            let body = serde_json::to_vec(&jsonrpc_error(None, -32700, &format!("parse error: {err}")))?;
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
        return Some(jsonrpc_error(message.id.clone(), -32600, "method is required"));
    };

    match method {
        "initialize" => handle_initialize(message.id, message.params.as_ref()),
        "notifications/initialized" => None,
        "ping" => Some(jsonrpc_result(message.id, json!({}))),
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
    let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

    let result = match name {
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

fn execute_read_virtualization_topology(engine: &Arc<RuleEngine>, arguments: Value) -> Result<Value> {
    let args: TopologyArgs = serde_json::from_value(arguments)?;
    let topology = engine.scan_topology();
    Ok(topology_to_value(topology, args.include_adapters))
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
            bind_mode: args.bind_mode.unwrap_or(wsl_bridge_shared::BindMode::AllNics),
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
        target_ref: args.target_ref.map(|value| value.map(|item| item.trim().to_owned())),
        target_host: args.target_host.map(|value| value.map(|item| item.trim().to_owned())),
        target_port: args.target_port,
        bind_mode: args.bind_mode,
        nic_id: args.nic_id.map(|value| value.map(|item| item.trim().to_owned())),
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

fn build_tool_definitions(config: &McpServerConfig) -> Vec<Value> {
    let mut tools = Vec::new();
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
            name: "read_virtualization_topology".to_owned(),
            description: "读取 WSL / Hyper-V 当前配置与解析后的 IP 拓扑。".to_owned(),
            enabled: config.expose_topology_read,
        },
        McpToolDescriptor {
            name: "list_forward_rules".to_owned(),
            description: "读取当前 TCP / UDP 转发规则与防火墙配置。".to_owned(),
            enabled: config.expose_rule_config,
        },
        McpToolDescriptor {
            name: "create_forward_rule".to_owned(),
            description: "创建新的 TCP / UDP 转发规则。".to_owned(),
            enabled: config.expose_rule_config,
        },
        McpToolDescriptor {
            name: "update_forward_rule".to_owned(),
            description: "更新现有 TCP / UDP 转发规则。".to_owned(),
            enabled: config.expose_rule_config,
        },
        McpToolDescriptor {
            name: "delete_forward_rule".to_owned(),
            description: "删除指定转发规则。".to_owned(),
            enabled: config.expose_rule_config,
        },
        McpToolDescriptor {
            name: "set_forward_rule_enabled".to_owned(),
            description: "启用或禁用指定转发规则。".to_owned(),
            enabled: config.expose_rule_config,
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
        match TcpListener::bind(("127.0.0.1", port)) {
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
    let request_line = lines.next().ok_or_else(|| anyhow!("missing request line"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or_else(|| anyhow!("missing method"))?.to_owned();
    let path = parts.next().ok_or_else(|| anyhow!("missing path"))?.to_owned();
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
        stream.shutdown(std::net::Shutdown::Write).expect("shutdown write");

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
        };

        state
            .engine
            .update_mcp_config(config.clone())
            .expect("save config");
        state.mcp_service.apply_config(&config);
        thread::sleep(Duration::from_millis(150));

        let updated = state.engine.get_mcp_config();
        assert_eq!(updated.listen_port, blocked_port + 1);
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
        };

        state
            .engine
            .update_mcp_config(config.clone())
            .expect("save config");
        state.mcp_service.apply_config(&config);
        thread::sleep(Duration::from_millis(150));

        let request = json!({
          "jsonrpc": "2.0",
          "id": 1,
          "method": "tools/list",
          "params": {}
        });

        let (unauthorized_status, unauthorized_body) = send_http_request(port, None, request.clone());
        assert_eq!(unauthorized_status, 401);
        assert!(unauthorized_body.contains("unauthorized"));

        let (authorized_status, authorized_body) =
            send_http_request(port, Some("secret-token"), request);
        assert_eq!(authorized_status, 200);
        assert!(authorized_body.contains("\"result\""));
        assert!(authorized_body.contains("read_virtualization_topology"));

        cleanup_state(&state, path);
    }
}
