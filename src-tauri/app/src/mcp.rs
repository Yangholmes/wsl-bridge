use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddrV4, TcpListener, TcpStream};
use std::net::ToSocketAddrs;
#[cfg(windows)]
use std::os::windows::io::AsRawSocket;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use socket2::{Domain, Protocol, Socket, Type};
use uuid::Uuid;
use wsl_bridge_core::{parse_hosts_text, render_hosts_text, RuleEngine};
use wsl_bridge_shared::{
    BindMode, CreateHostsGroupRequest, CreateProxyListenerRequest, CreateProxyRouteRequest,
    CreateProxyUpstreamRequest, CreateRuleRequest, FirewallPolicy, HostsEntryInput,
    LogQueryRequest, McpClientPreset, McpServerConfig, McpServerStatus, McpToolDescriptor,
    NewFirewallPolicy, NewProxyRule, ProxyProtocol, ProxyRule, ProxyTlsMode,
    QueryTrafficStatsRequest, RulePatch, RuleType, SaveHostsEntriesRequest, TargetKind,
    TopologySnapshot, UpdateHostsGroupRequest, UpdateProxyListenerRequest,
    UpdateProxyRouteRequest, UpdateProxyUpstreamRequest, UpstreamScheme,
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
const AI_API_VERSION: &str = "v1";
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
struct ApplyConfigPatchArgs {
    mode: Option<String>,
    patch: Value,
    idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportConfigArgs {
    modules: Option<Vec<String>>,
    format: Option<String>,
    group_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportConfigArgs {
    module: String,
    content: String,
    mode: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TestConnectivityArgs {
    target: ConnectivityTargetInput,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectivityTargetInput {
    #[serde(rename = "type")]
    target_type: String,
    value: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostPortConnectivityTarget {
    host: String,
    port: u16,
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpstreamConnectivityTarget {
    id: Option<String>,
    upstream_ref: Option<String>,
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxyRouteConnectivityTarget {
    id: Option<String>,
    route_ref: Option<String>,
    host: Option<String>,
    path: Option<String>,
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UrlConnectivityTarget {
    url: String,
    timeout_ms: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ConfigPatchInput {
    version: String,
    reason: Option<String>,
    proxy: Option<ProxyPatchInput>,
    hosts: Option<HostsPatchInput>,
    rules: Option<Value>,
    settings: Option<Value>,
}

#[derive(Debug, Deserialize, Default)]
struct ProxyPatchInput {
    listeners: Option<ProxyListenerPatchOps>,
    routes: Option<ProxyRoutePatchOps>,
    upstreams: Option<ProxyUpstreamPatchOps>,
}

#[derive(Debug, Deserialize, Default)]
struct HostsPatchInput {
    groups: Option<HostsGroupPatchOps>,
    records: Option<HostsRecordPatchOps>,
}

#[derive(Debug, Deserialize, Default)]
struct ProxyListenerPatchOps {
    #[serde(default)]
    create: Vec<ProxyListenerCreatePatch>,
    #[serde(default)]
    update: Vec<ProxyListenerUpdatePatch>,
    #[serde(default)]
    delete: Vec<ProxyListenerDeletePatch>,
}

#[derive(Debug, Deserialize, Default)]
struct ProxyRoutePatchOps {
    #[serde(default)]
    create: Vec<ProxyRouteCreatePatch>,
    #[serde(default)]
    update: Vec<ProxyRouteUpdatePatch>,
    #[serde(default)]
    delete: Vec<ProxyRouteDeletePatch>,
}

#[derive(Debug, Deserialize, Default)]
struct ProxyUpstreamPatchOps {
    #[serde(default)]
    create: Vec<ProxyUpstreamCreatePatch>,
    #[serde(default)]
    update: Vec<ProxyUpstreamUpdatePatch>,
    #[serde(default)]
    delete: Vec<ProxyUpstreamDeletePatch>,
}

#[derive(Debug, Deserialize, Default)]
struct HostsGroupPatchOps {
    #[serde(default)]
    create: Vec<HostsGroupCreatePatch>,
    #[serde(default)]
    update: Vec<HostsGroupUpdatePatch>,
    #[serde(default)]
    delete: Vec<HostsGroupDeletePatch>,
    activate: Option<HostsGroupActivatePatch>,
}

#[derive(Debug, Deserialize, Default)]
struct HostsRecordPatchOps {
    #[serde(default)]
    create: Vec<HostsRecordCreatePatch>,
    #[serde(default)]
    update: Vec<HostsRecordUpdatePatch>,
    #[serde(default)]
    delete: Vec<HostsRecordDeletePatch>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxyListenerCreatePatch {
    client_id: Option<String>,
    name: String,
    #[serde(alias = "listenHost")]
    bind_address: String,
    #[serde(alias = "listenPort")]
    port: u16,
    protocol: ProxyProtocol,
    #[serde(default)]
    tls_mode: Option<ProxyTlsMode>,
    cert_id: Option<String>,
    #[serde(default)]
    bind_mode: Option<BindMode>,
    nic_id: Option<String>,
    enabled: Option<bool>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxyListenerUpdatePatch {
    id: Option<String>,
    listener_ref: Option<String>,
    listener_name: Option<String>,
    name: Option<String>,
    #[serde(alias = "listenHost")]
    bind_address: Option<String>,
    #[serde(alias = "listenPort")]
    port: Option<u16>,
    protocol: Option<ProxyProtocol>,
    tls_mode: Option<ProxyTlsMode>,
    cert_id: Option<String>,
    bind_mode: Option<BindMode>,
    nic_id: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxyListenerDeletePatch {
    id: Option<String>,
    listener_ref: Option<String>,
    listener_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxyRouteCreatePatch {
    client_id: Option<String>,
    listener_ref: String,
    server_names: Vec<String>,
    path_prefix: Option<String>,
    is_default: Option<bool>,
    enabled: Option<bool>,
    upstream_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxyRouteUpdatePatch {
    id: Option<String>,
    route_ref: Option<String>,
    match_listener_ref: Option<String>,
    match_listener_name: Option<String>,
    match_server_names: Option<Vec<String>>,
    match_path_prefix: Option<Option<String>>,
    match_is_default: Option<bool>,
    server_names: Option<Vec<String>>,
    path_prefix: Option<Option<String>>,
    is_default: Option<bool>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxyRouteDeletePatch {
    id: Option<String>,
    route_ref: Option<String>,
    match_listener_ref: Option<String>,
    match_listener_name: Option<String>,
    match_server_names: Option<Vec<String>>,
    match_path_prefix: Option<Option<String>>,
    match_is_default: Option<bool>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxyUpstreamCreatePatch {
    client_id: Option<String>,
    route_ref: Option<String>,
    #[serde(alias = "targetType")]
    target_kind: TargetKind,
    target_ref: Option<String>,
    target_host: Option<String>,
    target_port: u16,
    #[serde(alias = "protocol")]
    upstream_scheme: UpstreamScheme,
    path_rewrite_from: Option<String>,
    path_rewrite_to: Option<String>,
    enabled: Option<bool>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxyUpstreamUpdatePatch {
    id: Option<String>,
    upstream_ref: Option<String>,
    route_ref: Option<String>,
    match_route_ref: Option<String>,
    match_listener_ref: Option<String>,
    match_listener_name: Option<String>,
    match_server_names: Option<Vec<String>>,
    match_path_prefix: Option<Option<String>>,
    match_is_default: Option<bool>,
    match_target_kind: Option<TargetKind>,
    match_target_ref: Option<String>,
    match_target_host: Option<String>,
    match_target_port: Option<u16>,
    #[serde(alias = "matchProtocol")]
    match_upstream_scheme: Option<UpstreamScheme>,
    target_kind: Option<TargetKind>,
    target_ref: Option<String>,
    target_host: Option<String>,
    target_port: Option<u16>,
    #[serde(alias = "protocol")]
    upstream_scheme: Option<UpstreamScheme>,
    path_rewrite_from: Option<Option<String>>,
    path_rewrite_to: Option<Option<String>>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxyUpstreamDeletePatch {
    id: Option<String>,
    upstream_ref: Option<String>,
    match_route_ref: Option<String>,
    match_listener_ref: Option<String>,
    match_listener_name: Option<String>,
    match_server_names: Option<Vec<String>>,
    match_path_prefix: Option<Option<String>>,
    match_is_default: Option<bool>,
    match_target_kind: Option<TargetKind>,
    match_target_ref: Option<String>,
    match_target_host: Option<String>,
    match_target_port: Option<u16>,
    #[serde(alias = "matchProtocol")]
    match_upstream_scheme: Option<UpstreamScheme>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostsGroupCreatePatch {
    client_id: Option<String>,
    name: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostsGroupUpdatePatch {
    id: Option<String>,
    group_ref: Option<String>,
    name: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostsGroupDeletePatch {
    id: Option<String>,
    group_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostsGroupActivatePatch {
    group_ref: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostsRecordCreatePatch {
    client_id: Option<String>,
    group_ref: String,
    ip: String,
    domain: String,
    enabled: Option<bool>,
    comment: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostsRecordUpdatePatch {
    id: Option<String>,
    record_ref: Option<String>,
    group_ref: Option<String>,
    ip: Option<String>,
    domain: Option<String>,
    enabled: Option<bool>,
    comment: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostsRecordDeletePatch {
    id: Option<String>,
    record_ref: Option<String>,
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
    project_root: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UninstallAgentSkillArgs {
    target: String,
    scope: Option<String>,
    mode: Option<String>,
    fallback_to_agents_dir: Option<bool>,
    project_root: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentSkillInstallWrite {
    path: String,
    action: String,
    source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentMcpClientState {
    target_agent: String,
    install_supported: bool,
    detected_state: String,
    path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentSkillInstallWarning {
    severity: String,
    code: String,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentSkillInstallPlan {
    ok: bool,
    mode: String,
    operation: String,
    skill: Value,
    target_agent: String,
    scope: String,
    install_type: String,
    detected_state: Option<String>,
    root_path: Option<String>,
    resolved_paths: Vec<String>,
    writes: Vec<AgentSkillInstallWrite>,
    deletes: Vec<AgentSkillInstallWrite>,
    warnings: Vec<AgentSkillInstallWarning>,
}

#[derive(Debug, Clone)]
struct AgentSkillResolvedWrite {
    write: AgentSkillInstallWrite,
    destination: PathBuf,
    exists: bool,
    managed: bool,
}

#[derive(Debug, Clone)]
struct AgentSkillDetection {
    state: String,
    files: Vec<AgentSkillResolvedWrite>,
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
              "uri": "wsl-bridge://state/rules",
              "name": "Rules State",
              "description": "Legacy Rules status, create restrictions and migration summary.",
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
            },
            {
              "uri": "wsl-bridge://schemas/state",
              "name": "State Resource Schema",
              "description": "Shape guide for summary, proxy, hosts, rules, traffic and logs resources.",
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
        "wsl-bridge://state/rules" => Ok((
            "application/json",
            serde_json::to_string_pretty(&state_rules_resource(engine, "full"))?,
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
        "wsl-bridge://schemas/state" => Ok((
            "application/json",
            serde_json::to_string_pretty(&state_schema_resource())?,
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
        "apply_config_patch" => execute_apply_config_patch(engine, arguments),
        "export_config" => execute_export_config(engine, arguments),
        "import_config" => execute_import_config(engine, arguments),
        "test_connectivity" => execute_test_connectivity(engine, arguments),
        "list_agent_targets" => execute_list_agent_targets(arguments),
        "install_agent_skill" => execute_install_agent_skill(engine, arguments),
        "uninstall_agent_skill" => execute_uninstall_agent_skill(engine, arguments),
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
          "code": "CONFIG_PATCH_VALIDATE_ONLY",
          "message": "This tool only validates shape and obvious conflicts. Use apply_config_patch to persist supported Proxy / Hosts changes."
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

fn execute_export_config(engine: &Arc<RuleEngine>, arguments: Value) -> Result<Value> {
    let args: ExportConfigArgs = serde_json::from_value(arguments)?;
    let format = args
        .format
        .as_deref()
        .unwrap_or("json")
        .trim()
        .to_ascii_lowercase();
    let mut modules = args.modules.unwrap_or_else(|| {
        vec!["proxy".to_owned(), "hosts".to_owned(), "rules".to_owned()]
    });
    modules = normalize_export_modules(modules)?;

    match format.as_str() {
        "json" => Ok(json!({
          "ok": true,
          "format": "json",
          "modules": modules,
          "content": export_config_json_payload(engine, &modules)
        })),
        "hosts-file" => {
            if modules.len() != 1 || modules.first().map(String::as_str) != Some("hosts") {
                return Err(anyhow!(
                    "export_config format=hosts-file only supports modules=[\"hosts\"]"
                ));
            }
            let group = resolve_export_hosts_group(engine, args.group_ref.as_deref())?;
            let entries = engine.list_hosts_entries(&group.id)?;
            let content = render_hosts_text(
                &entries
                    .into_iter()
                    .map(|entry| HostsEntryInput {
                        id: Some(entry.id),
                        ip: entry.ip,
                        domain: entry.domain,
                        comment: entry.comment,
                        enabled: entry.enabled,
                        order_index: entry.order_index,
                    })
                    .collect::<Vec<_>>(),
            );
            Ok(json!({
              "ok": true,
              "format": "hosts-file",
              "modules": ["hosts"],
              "group": {
                "id": group.id,
                "name": group.name,
                "isActive": group.is_active,
                "updatedAt": group.updated_at
              },
              "content": content
            }))
        }
        _ => Err(anyhow!(
            "export_config format must be either json or hosts-file"
        )),
    }
}

fn execute_import_config(engine: &Arc<RuleEngine>, arguments: Value) -> Result<Value> {
    let args: ImportConfigArgs = serde_json::from_value(arguments)?;
    let module = args.module.trim().to_ascii_lowercase();
    let mode = args.mode.unwrap_or_else(|| "dryRun".to_owned());
    if mode != "dryRun" && mode != "apply" {
        return Err(anyhow!(
            "import_config mode must be either dryRun or apply"
        ));
    }

    let content = args.content.trim();
    if content.is_empty() {
        return Err(anyhow!("import_config content is required"));
    }

    let (import_kind, patch) = match module.as_str() {
        "hosts" => build_hosts_import_patch(content)?,
        "proxy" => build_proxy_import_patch(content)?,
        _ => return Err(anyhow!("import_config module must be either hosts or proxy")),
    };

    let result = execute_apply_config_patch(
        engine,
        json!({
          "mode": mode,
          "patch": patch
        }),
    )?;

    let mut object = result
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("import_config expected object result from apply_config_patch"))?;
    object.insert("module".to_owned(), json!(module));
    object.insert("importKind".to_owned(), json!(import_kind));
    Ok(Value::Object(object))
}

fn execute_apply_config_patch(engine: &Arc<RuleEngine>, arguments: Value) -> Result<Value> {
    let args: ApplyConfigPatchArgs = serde_json::from_value(arguments)?;
    let mode = args.mode.unwrap_or_else(|| "dryRun".to_owned());
    if mode != "dryRun" && mode != "apply" {
        return Err(anyhow!(
            "apply_config_patch mode must be either dryRun or apply"
        ));
    }

    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    validate_config_patch_shape(&args.patch, &mut errors, &mut warnings);

    let patch = match serde_json::from_value::<ConfigPatchInput>(args.patch.clone()) {
        Ok(value) => value,
        Err(err) => {
            errors.push(json!({
              "code": "PATCH_PARSE_FAILED",
              "target": "patch",
              "message": format!("ConfigPatch parse failed: {err}")
            }));
            return Ok(json!({
              "ok": false,
              "mode": mode,
              "errors": errors,
              "warnings": warnings,
              "conflicts": [],
              "effects": {
                "creates": [],
                "updates": [],
                "deletes": [],
                "runtimeRestartRequired": false,
                "requiresAdmin": false,
                "requiresConfirmation": false
              }
            }));
        }
    };

    let mut dry_run = DryRunAccumulator::default();
    let mut ctx = DryRunContext::new(engine);

    if patch.version != CONFIG_PATCH_VERSION {
        errors.push(json!({
          "code": "PATCH_VERSION_UNSUPPORTED",
          "target": "patch.version",
          "message": format!("Unsupported ConfigPatch version: {}", patch.version)
        }));
    }
    if patch.rules.is_some() {
        errors.push(json!({
          "code": "PATCH_RULES_NOT_SUPPORTED",
          "target": "rules",
          "message": "Rules ConfigPatch dry-run is not supported in this build yet."
        }));
    }
    if patch.settings.is_some() {
        errors.push(json!({
          "code": "PATCH_SETTINGS_NOT_SUPPORTED",
          "target": "settings",
          "message": "Settings ConfigPatch dry-run is not supported in this build yet."
        }));
    }

    if mode == "apply" {
        if let Some(idempotency_key) = args.idempotency_key.as_deref() {
            if apply_config_patch_already_applied(engine, idempotency_key) {
                return Ok(json!({
                  "ok": true,
                  "mode": mode,
                  "summary": "ConfigPatch already applied for this idempotency key; skipped duplicate apply.",
                  "warnings": warnings,
                  "conflicts": [],
                  "errors": [],
                  "effects": {
                    "creates": [],
                    "updates": [],
                    "deletes": [],
                    "runtimeRestartRequired": false,
                    "requiresAdmin": false,
                    "requiresConfirmation": false,
                    "idempotencyKeyAccepted": true,
                    "skippedDuplicate": true
                  }
                }));
            }
        }
    }

    if let Some(proxy) = patch.proxy.as_ref() {
        dry_run_proxy_patch(&mut ctx, &mut dry_run, proxy);
    }
    if let Some(hosts) = patch.hosts.as_ref() {
        dry_run_hosts_patch(&mut ctx, &mut dry_run, hosts);
    }

    let summary = dry_run.summary();
    warnings.extend(std::mem::take(&mut dry_run.warnings));
    let dry_run_ok = errors.is_empty() && dry_run.conflicts.is_empty();
    if mode == "dryRun" {
        return Ok(json!({
          "ok": dry_run_ok,
          "mode": mode,
          "summary": summary,
          "warnings": warnings,
          "conflicts": dry_run.conflicts,
          "errors": errors,
          "effects": {
            "creates": dry_run.creates,
            "updates": dry_run.updates,
            "deletes": dry_run.deletes,
            "runtimeRestartRequired": dry_run.runtime_restart_required,
            "requiresAdmin": dry_run.requires_admin,
            "requiresConfirmation": dry_run.requires_confirmation,
            "idempotencyKeyAccepted": args.idempotency_key.is_some()
          }
        }));
    }

    if !dry_run_ok {
        return Ok(json!({
          "ok": false,
          "mode": mode,
          "summary": summary,
          "warnings": warnings,
          "conflicts": dry_run.conflicts,
          "errors": errors,
          "effects": {
            "creates": dry_run.creates,
            "updates": dry_run.updates,
            "deletes": dry_run.deletes,
            "runtimeRestartRequired": dry_run.runtime_restart_required,
            "requiresAdmin": dry_run.requires_admin,
            "requiresConfirmation": dry_run.requires_confirmation,
            "idempotencyKeyAccepted": args.idempotency_key.is_some()
          }
        }));
    }

    let rollback_snapshot = engine.capture_snapshot();
    let apply_result = apply_config_patch_changes(engine, &patch);
    let references = match apply_result {
        Ok(references) => references,
        Err(err) => {
            let rollback_result = engine.restore_snapshot(rollback_snapshot);
            let mut apply_errors = errors;
            apply_errors.push(json!({
              "code": "APPLY_FAILED",
              "target": "patch",
              "message": err.to_string()
            }));
            if let Err(rollback_err) = rollback_result {
                apply_errors.push(json!({
                  "code": "ROLLBACK_FAILED",
                  "target": "patch",
                  "message": rollback_err.to_string()
                }));
            } else {
                engine.append_audit_log(
                    "warn",
                    "ai",
                    "config_patch_rolled_back",
                    &audit_detail(
                        patch.reason.as_deref(),
                        args.idempotency_key.as_deref(),
                        "rolled_back",
                    ),
                );
            }
            return Ok(json!({
              "ok": false,
              "mode": mode,
              "summary": summary,
              "warnings": warnings,
              "conflicts": dry_run.conflicts,
              "errors": apply_errors,
              "effects": {
                "creates": dry_run.creates,
                "updates": dry_run.updates,
                "deletes": dry_run.deletes,
                "runtimeRestartRequired": dry_run.runtime_restart_required,
                "requiresAdmin": dry_run.requires_admin,
                "requiresConfirmation": dry_run.requires_confirmation,
                "idempotencyKeyAccepted": args.idempotency_key.is_some()
              }
            }));
        }
    };

    engine.append_audit_log(
        "info",
        "ai",
        "config_patch_applied",
        &audit_detail(
            patch.reason.as_deref(),
            args.idempotency_key.as_deref(),
            "applied",
        ),
    );
    Ok(json!({
      "ok": true,
      "mode": mode,
      "summary": summary,
      "warnings": warnings,
      "conflicts": [],
      "errors": [],
      "effects": {
        "creates": dry_run.creates,
        "updates": dry_run.updates,
        "deletes": dry_run.deletes,
        "runtimeRestartRequired": dry_run.runtime_restart_required,
        "requiresAdmin": dry_run.requires_admin,
        "requiresConfirmation": dry_run.requires_confirmation,
        "idempotencyKeyAccepted": args.idempotency_key.is_some()
      },
      "references": references
    }))
}

#[derive(Default)]
struct ApplyExecutionContext {
    listener_refs: HashMap<String, String>,
    route_refs: HashMap<String, String>,
    upstream_refs: HashMap<String, String>,
    group_refs: HashMap<String, String>,
    record_refs: HashMap<String, String>,
    record_groups: HashMap<String, String>,
    staged_entries: HashMap<String, Vec<HostsEntryInput>>,
}

fn apply_config_patch_changes(
    engine: &Arc<RuleEngine>,
    patch: &ConfigPatchInput,
) -> Result<Value> {
    let mut ctx = ApplyExecutionContext::default();

    if let Some(proxy) = patch.proxy.as_ref() {
        apply_proxy_patch(engine, patch, proxy, &mut ctx)?;
    }
    if let Some(hosts) = patch.hosts.as_ref() {
        apply_hosts_patch(engine, hosts, &mut ctx)?;
    }

    Ok(json!({
      "listeners": ctx.listener_refs,
      "routes": ctx.route_refs,
      "upstreams": ctx.upstream_refs,
      "hostsGroups": ctx.group_refs,
      "hostsRecords": ctx.record_refs
    }))
}

fn apply_proxy_patch(
    engine: &Arc<RuleEngine>,
    patch: &ConfigPatchInput,
    proxy: &ProxyPatchInput,
    ctx: &mut ApplyExecutionContext,
) -> Result<()> {
    let inferred_route_refs = proxy
        .routes
        .as_ref()
        .map(|ops| {
            ops.create
                .iter()
                .enumerate()
                .filter_map(|(index, route)| {
                    route.upstream_ref.as_ref().map(|upstream_ref| {
                        (
                            upstream_ref.clone(),
                            patch_reference(route.client_id.as_deref(), "route", index),
                        )
                    })
                })
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();

    if let Some(listeners) = proxy.listeners.as_ref() {
        for (index, item) in listeners.create.iter().enumerate() {
            let listener_id = engine.create_proxy_listener(CreateProxyListenerRequest {
                name: item.name.clone(),
                listen_host: item.bind_address.clone(),
                listen_port: item.port,
                protocol: item.protocol,
                tls_mode: item.tls_mode.unwrap_or_else(|| default_tls_mode(item.protocol)),
                cert_id: item.cert_id.clone(),
                bind_mode: item.bind_mode.unwrap_or(BindMode::AllNics),
                nic_id: item.nic_id.clone(),
                enabled: item.enabled.unwrap_or(true),
            })?;
            ctx.listener_refs.insert(
                patch_reference(item.client_id.as_deref(), "listener", index),
                listener_id,
            );
        }

        for item in &listeners.update {
            let listener_id = resolve_apply_listener_target(
                engine,
                item.id.as_deref(),
                item.listener_ref.as_deref(),
                item.listener_name.as_deref(),
                &ctx.listener_refs,
            )?
            .ok_or_else(|| anyhow!("listener update target is missing"))?;
            let current = find_listener_by_id(engine, &listener_id)
                .ok_or_else(|| anyhow!("listener `{listener_id}` not found during apply"))?;
            engine.update_proxy_listener(
                &listener_id,
                UpdateProxyListenerRequest {
                    name: item.name.clone().unwrap_or(current.name),
                    listen_host: item.bind_address.clone().unwrap_or(current.listen_host),
                    listen_port: item.port.unwrap_or(current.listen_port),
                    protocol: item.protocol.unwrap_or(current.protocol),
                    tls_mode: item.tls_mode.unwrap_or(current.tls_mode),
                    cert_id: item.cert_id.clone().or(current.cert_id),
                    bind_mode: item.bind_mode.unwrap_or(current.bind_mode),
                    nic_id: item.nic_id.clone().or(current.nic_id),
                    enabled: item.enabled.unwrap_or(current.enabled),
                },
            )?;
        }
    }

    if let Some(routes) = proxy.routes.as_ref() {
        for (index, item) in routes.create.iter().enumerate() {
            let listener_id = resolve_apply_reference(
                None,
                Some(&item.listener_ref),
                &ctx.listener_refs,
            )
            .ok_or_else(|| anyhow!("route listener reference `{}` is missing", item.listener_ref))?;
            let route_id = engine.create_proxy_route(CreateProxyRouteRequest {
                listener_id,
                server_names: item.server_names.clone(),
                path_prefix: item.path_prefix.clone(),
                is_default: item.is_default.unwrap_or(false),
                enabled: item.enabled.unwrap_or(true),
            })?;
            ctx.route_refs.insert(
                patch_reference(item.client_id.as_deref(), "route", index),
                route_id,
            );
        }

        for item in &routes.update {
            let route_id = if let Some(value) =
                resolve_apply_reference(item.id.as_deref(), item.route_ref.as_deref(), &ctx.route_refs)
            {
                value
            } else {
                find_route_id_by_selector(
                    engine,
                    item.match_listener_ref.as_deref(),
                    item.match_listener_name.as_deref(),
                    item.match_server_names.as_deref(),
                    item.match_path_prefix.as_ref().map(|value| value.as_deref()),
                    item.match_is_default,
                    &ctx.listener_refs,
                )?
                .ok_or_else(|| anyhow!("route update target is missing"))?
            };
            let (_, current) = resolve_route_with_listener(engine, Some(&route_id), None)
                .ok_or_else(|| anyhow!("route `{route_id}` not found during apply"))?;
            engine.update_proxy_route(
                &route_id,
                UpdateProxyRouteRequest {
                    server_names: item
                        .server_names
                        .clone()
                        .unwrap_or(current.server_names),
                    path_prefix: item
                        .path_prefix
                        .clone()
                        .unwrap_or(current.path_prefix),
                    is_default: item.is_default.unwrap_or(current.is_default),
                    enabled: item.enabled.unwrap_or(current.enabled),
                },
            )?;
        }

        if let Some(upstreams) = proxy.upstreams.as_ref() {
            for (index, item) in upstreams.create.iter().enumerate() {
                let route_ref = item
                    .route_ref
                    .clone()
                    .or_else(|| {
                        let key = patch_reference(item.client_id.as_deref(), "upstream", index);
                        inferred_route_refs.get(&key).cloned()
                    });
                let route_id = route_ref
                    .as_deref()
                    .and_then(|value| resolve_apply_reference(None, Some(value), &ctx.route_refs))
                    .ok_or_else(|| anyhow!("upstream route reference is missing"))?;
                let upstream_id = engine.create_proxy_upstream(CreateProxyUpstreamRequest {
                    route_id,
                    target_kind: item.target_kind,
                    target_ref: item.target_ref.clone(),
                    target_host: item.target_host.clone(),
                    target_port: item.target_port,
                    upstream_scheme: item.upstream_scheme,
                    path_rewrite_from: item.path_rewrite_from.clone(),
                    path_rewrite_to: item.path_rewrite_to.clone(),
                    enabled: item.enabled.unwrap_or(true),
                })?;
                ctx.upstream_refs.insert(
                    patch_reference(item.client_id.as_deref(), "upstream", index),
                    upstream_id,
                );
            }

            for item in &upstreams.update {
                let upstream_id = if let Some(value) = resolve_apply_reference(
                    item.id.as_deref(),
                    item.upstream_ref.as_deref(),
                    &ctx.upstream_refs,
                ) {
                    value
                } else {
                    find_upstream_id_by_selector(
                        engine,
                        item.match_route_ref.as_deref(),
                        item.match_listener_ref.as_deref(),
                        item.match_listener_name.as_deref(),
                        item.match_server_names.as_deref(),
                        item.match_path_prefix.as_ref().map(|value| value.as_deref()),
                        item.match_is_default,
                        item.match_target_kind,
                        item.match_target_ref.as_deref(),
                        item.match_target_host.as_deref(),
                        item.match_target_port,
                        item.match_upstream_scheme,
                        &ctx.listener_refs,
                        &ctx.route_refs,
                    )?
                    .ok_or_else(|| anyhow!("upstream update target is missing"))?
                };
                let current = resolve_upstream_by_ref(engine, Some(&upstream_id), None)
                    .ok_or_else(|| anyhow!("upstream `{upstream_id}` not found during apply"))?;
                engine.update_proxy_upstream(
                    &upstream_id,
                    UpdateProxyUpstreamRequest {
                        target_kind: item.target_kind.unwrap_or(current.target_kind),
                        target_ref: item.target_ref.clone().or(current.target_ref),
                        target_host: item.target_host.clone().or(current.target_host),
                        target_port: item.target_port.unwrap_or(current.target_port),
                        upstream_scheme: item.upstream_scheme.unwrap_or(current.upstream_scheme),
                        path_rewrite_from: item
                            .path_rewrite_from
                            .clone()
                            .unwrap_or(current.path_rewrite_from),
                        path_rewrite_to: item
                            .path_rewrite_to
                            .clone()
                            .unwrap_or(current.path_rewrite_to),
                        enabled: item.enabled.unwrap_or(current.enabled),
                    },
                )?;
            }

            for item in &upstreams.delete {
                let upstream_id = if let Some(value) = resolve_apply_reference(
                    item.id.as_deref(),
                    item.upstream_ref.as_deref(),
                    &ctx.upstream_refs,
                ) {
                    value
                } else {
                    find_upstream_id_by_selector(
                        engine,
                        item.match_route_ref.as_deref(),
                        item.match_listener_ref.as_deref(),
                        item.match_listener_name.as_deref(),
                        item.match_server_names.as_deref(),
                        item.match_path_prefix.as_ref().map(|value| value.as_deref()),
                        item.match_is_default,
                        item.match_target_kind,
                        item.match_target_ref.as_deref(),
                        item.match_target_host.as_deref(),
                        item.match_target_port,
                        item.match_upstream_scheme,
                        &ctx.listener_refs,
                        &ctx.route_refs,
                    )?
                    .ok_or_else(|| anyhow!("upstream delete target is missing"))?
                };
                engine.delete_proxy_upstream(&upstream_id)?;
            }
        }

        for item in &routes.delete {
            let route_id = if let Some(value) =
                resolve_apply_reference(item.id.as_deref(), item.route_ref.as_deref(), &ctx.route_refs)
            {
                value
            } else {
                find_route_id_by_selector(
                    engine,
                    item.match_listener_ref.as_deref(),
                    item.match_listener_name.as_deref(),
                    item.match_server_names.as_deref(),
                    item.match_path_prefix.as_ref().map(|value| value.as_deref()),
                    item.match_is_default,
                    &ctx.listener_refs,
                )?
                .ok_or_else(|| anyhow!("route delete target is missing"))?
            };
            engine.delete_proxy_route(&route_id)?;
        }
    }

    if let Some(listeners) = proxy.listeners.as_ref() {
        for item in &listeners.delete {
            let listener_id = resolve_apply_listener_target(
                engine,
                item.id.as_deref(),
                item.listener_ref.as_deref(),
                item.listener_name.as_deref(),
                &ctx.listener_refs,
            )?
            .ok_or_else(|| anyhow!("listener delete target is missing"))?;
            engine.delete_proxy_listener(&listener_id)?;
        }
    }

    if patch.rules.is_some() || patch.settings.is_some() {
        return Err(anyhow!(
            "rules/settings config patch apply is not supported in this build"
        ));
    }

    Ok(())
}

fn apply_hosts_patch(
    engine: &Arc<RuleEngine>,
    hosts: &HostsPatchInput,
    ctx: &mut ApplyExecutionContext,
) -> Result<()> {
    if let Some(groups) = hosts.groups.as_ref() {
        for (index, item) in groups.create.iter().enumerate() {
            let group_id = engine.create_hosts_group(CreateHostsGroupRequest {
                name: item.name.clone(),
                description: item.description.clone(),
            })?;
            ctx.group_refs.insert(
                patch_reference(item.client_id.as_deref(), "hosts-group", index),
                group_id,
            );
        }

        for item in &groups.update {
            let group_id = resolve_apply_reference(
                item.id.as_deref(),
                item.group_ref.as_deref(),
                &ctx.group_refs,
            )
            .ok_or_else(|| anyhow!("hosts group update target is missing"))?;
            let current = find_hosts_group_by_id(engine, &group_id)
                .ok_or_else(|| anyhow!("hosts group `{group_id}` not found during apply"))?;
            engine.update_hosts_group(
                &group_id,
                UpdateHostsGroupRequest {
                    name: item.name.clone().unwrap_or(current.name),
                    description: item.description.clone().or(current.description),
                },
            )?;
        }
    }

    build_hosts_record_index(engine, ctx)?;

    if let Some(records) = hosts.records.as_ref() {
        for (index, item) in records.create.iter().enumerate() {
            let group_id = resolve_apply_reference(
                None,
                Some(&item.group_ref),
                &ctx.group_refs,
            )
            .ok_or_else(|| anyhow!("hosts record group reference `{}` is missing", item.group_ref))?;
            let entries = staged_entries_for_group(engine, ctx, &group_id)?;
            let record_id = Uuid::new_v4().to_string();
            entries.push(HostsEntryInput {
                id: Some(record_id.clone()),
                ip: item.ip.clone(),
                domain: item.domain.clone(),
                comment: item.comment.clone(),
                enabled: item.enabled.unwrap_or(true),
                order_index: entries.len() as u32,
            });
            ctx.record_refs.insert(
                patch_reference(item.client_id.as_deref(), "hosts-record", index),
                record_id.clone(),
            );
            ctx.record_groups.insert(record_id, group_id);
        }

        for item in &records.update {
            let record_id = resolve_apply_reference(
                item.id.as_deref(),
                item.record_ref.as_deref(),
                &ctx.record_refs,
            )
            .ok_or_else(|| anyhow!("hosts record update target is missing"))?;
            let source_group_id = ctx
                .record_groups
                .get(&record_id)
                .cloned()
                .ok_or_else(|| anyhow!("hosts record `{record_id}` group is missing"))?;
            let target_group_id = item
                .group_ref
                .as_deref()
                .and_then(|value| resolve_apply_reference(None, Some(value), &ctx.group_refs))
                .unwrap_or_else(|| source_group_id.clone());

            let source_entries = staged_entries_for_group(engine, ctx, &source_group_id)?;
            let source_index = source_entries
                .iter()
                .position(|entry| entry.id.as_deref() == Some(&record_id))
                .ok_or_else(|| anyhow!("hosts record `{record_id}` not found during apply"))?;
            let mut record = source_entries.remove(source_index);
            record.ip = item.ip.clone().unwrap_or(record.ip);
            record.domain = item.domain.clone().unwrap_or(record.domain);
            record.comment = item.comment.clone().or(record.comment);
            record.enabled = item.enabled.unwrap_or(record.enabled);

            if source_group_id == target_group_id {
                source_entries.insert(source_index, record);
            } else {
                let target_entries = staged_entries_for_group(engine, ctx, &target_group_id)?;
                target_entries.push(record);
                ctx.record_groups.insert(record_id, target_group_id);
            }
        }

        for item in &records.delete {
            let record_id = resolve_apply_reference(
                item.id.as_deref(),
                item.record_ref.as_deref(),
                &ctx.record_refs,
            )
            .ok_or_else(|| anyhow!("hosts record delete target is missing"))?;
            let group_id = ctx
                .record_groups
                .get(&record_id)
                .cloned()
                .ok_or_else(|| anyhow!("hosts record `{record_id}` group is missing"))?;
            let entries = staged_entries_for_group(engine, ctx, &group_id)?;
            let index = entries
                .iter()
                .position(|entry| entry.id.as_deref() == Some(&record_id))
                .ok_or_else(|| anyhow!("hosts record `{record_id}` not found during apply"))?;
            entries.remove(index);
            ctx.record_groups.remove(&record_id);
        }
    }

    persist_staged_hosts_entries(engine, ctx)?;

    if let Some(groups) = hosts.groups.as_ref() {
        if let Some(activate) = groups.activate.as_ref() {
            let group_id = resolve_apply_reference(
                None,
                Some(&activate.group_ref),
                &ctx.group_refs,
            )
            .ok_or_else(|| anyhow!("hosts activate target `{}` is missing", activate.group_ref))?;
            engine.activate_hosts_group(&group_id)?;
        }

        for item in &groups.delete {
            let group_id = resolve_apply_reference(
                item.id.as_deref(),
                item.group_ref.as_deref(),
                &ctx.group_refs,
            )
            .ok_or_else(|| anyhow!("hosts group delete target is missing"))?;
            engine.delete_hosts_group(&group_id)?;
        }
    }

    Ok(())
}

fn build_hosts_record_index(engine: &Arc<RuleEngine>, ctx: &mut ApplyExecutionContext) -> Result<()> {
    for group in engine.list_hosts_groups() {
        for entry in engine.list_hosts_entries(&group.id)? {
            ctx.record_groups.insert(entry.id.clone(), group.id.clone());
        }
    }
    Ok(())
}

fn staged_entries_for_group<'a>(
    engine: &Arc<RuleEngine>,
    ctx: &'a mut ApplyExecutionContext,
    group_id: &str,
) -> Result<&'a mut Vec<HostsEntryInput>> {
    if !ctx.staged_entries.contains_key(group_id) {
        let entries = engine
            .list_hosts_entries(group_id)?
            .into_iter()
            .map(|entry| HostsEntryInput {
                id: Some(entry.id),
                ip: entry.ip,
                domain: entry.domain,
                comment: entry.comment,
                enabled: entry.enabled,
                order_index: entry.order_index,
            })
            .collect::<Vec<_>>();
        ctx.staged_entries.insert(group_id.to_owned(), entries);
    }

    ctx.staged_entries
        .get_mut(group_id)
        .ok_or_else(|| anyhow!("staged hosts group `{group_id}` is missing"))
}

fn persist_staged_hosts_entries(engine: &Arc<RuleEngine>, ctx: &mut ApplyExecutionContext) -> Result<()> {
    let group_ids = ctx.staged_entries.keys().cloned().collect::<Vec<_>>();
    for group_id in group_ids {
        let Some(entries) = ctx.staged_entries.remove(&group_id) else {
            continue;
        };
        let normalized = entries
            .into_iter()
            .enumerate()
            .map(|(index, mut entry)| {
                entry.order_index = index as u32;
                entry
            })
            .collect::<Vec<_>>();
        engine.save_hosts_entries(SaveHostsEntriesRequest {
            group_id,
            entries: normalized,
        })?;
    }
    Ok(())
}

fn resolve_apply_reference(
    id: Option<&str>,
    patch_ref: Option<&str>,
    created_refs: &HashMap<String, String>,
) -> Option<String> {
    if let Some(id) = id.map(str::trim).filter(|value| !value.is_empty()) {
        return Some(id.to_owned());
    }

    let reference = patch_ref?.trim();
    if reference.is_empty() {
        return None;
    }
    Some(
        created_refs
            .get(reference)
            .cloned()
            .unwrap_or_else(|| reference.to_owned()),
    )
}

fn find_listener_by_id(
    engine: &Arc<RuleEngine>,
    id: &str,
) -> Option<wsl_bridge_shared::ProxyListener> {
    engine
        .list_proxy_listeners()
        .into_iter()
        .find(|listener| listener.id == id)
}

fn find_listener_id_by_name(engine: &Arc<RuleEngine>, name: &str) -> Result<Option<String>> {
    let normalized = normalize_selector_text(Some(name));
    let Some(normalized) = normalized else {
        return Ok(None);
    };
    let matches = engine
        .list_proxy_listeners()
        .into_iter()
        .filter(|listener| listener.name.trim().eq_ignore_ascii_case(&normalized))
        .map(|listener| listener.id)
        .collect::<Vec<_>>();
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.into_iter().next()),
        _ => Err(anyhow!(
            "multiple listeners match name `{}`; use id for an exact target",
            normalized
        )),
    }
}

fn resolve_apply_listener_target(
    engine: &Arc<RuleEngine>,
    id: Option<&str>,
    listener_ref: Option<&str>,
    listener_name: Option<&str>,
    created_refs: &HashMap<String, String>,
) -> Result<Option<String>> {
    if let Some(target) = resolve_apply_reference(id, listener_ref, created_refs) {
        return Ok(Some(target));
    }
    find_listener_id_by_name(engine, listener_name.unwrap_or(""))
}

fn find_route_id_by_selector(
    engine: &Arc<RuleEngine>,
    match_listener_ref: Option<&str>,
    match_listener_name: Option<&str>,
    match_server_names: Option<&[String]>,
    match_path_prefix: Option<Option<&str>>,
    match_is_default: Option<bool>,
    listener_refs: &HashMap<String, String>,
) -> Result<Option<String>> {
    let listener_id = if let Some(value) = resolve_apply_reference(None, match_listener_ref, listener_refs) {
        Some(value)
    } else {
        find_listener_id_by_name(engine, match_listener_name.unwrap_or(""))?
    };
    let mut matches = Vec::new();
    for listener in engine.list_proxy_listeners() {
        for route in engine.list_proxy_routes(&listener.id).unwrap_or_default() {
            if route_selector_matches(
                &route.listener_id,
                &route.server_names,
                route.path_prefix.as_deref(),
                route.is_default,
                listener_id.as_deref(),
                match_server_names,
                match_path_prefix,
                match_is_default,
            ) {
                matches.push(route.id);
            }
        }
    }
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.into_iter().next()),
        _ => Err(anyhow!(
            "multiple routes match the provided selector; use id for an exact target"
        )),
    }
}

#[allow(clippy::too_many_arguments)]
fn find_upstream_id_by_selector(
    engine: &Arc<RuleEngine>,
    match_route_ref: Option<&str>,
    match_listener_ref: Option<&str>,
    match_listener_name: Option<&str>,
    match_server_names: Option<&[String]>,
    match_path_prefix: Option<Option<&str>>,
    match_is_default: Option<bool>,
    match_target_kind: Option<TargetKind>,
    match_target_ref: Option<&str>,
    match_target_host: Option<&str>,
    match_target_port: Option<u16>,
    match_upstream_scheme: Option<UpstreamScheme>,
    listener_refs: &HashMap<String, String>,
    route_refs: &HashMap<String, String>,
) -> Result<Option<String>> {
    let route_id = if let Some(value) = resolve_apply_reference(None, match_route_ref, route_refs) {
        Some(value)
    } else {
        find_route_id_by_selector(
            engine,
            match_listener_ref,
            match_listener_name,
            match_server_names,
            match_path_prefix,
            match_is_default,
            listener_refs,
        )?
    };
    let mut matches = Vec::new();
    for listener in engine.list_proxy_listeners() {
        for route in engine.list_proxy_routes(&listener.id).unwrap_or_default() {
            for upstream in engine.list_proxy_upstreams(&route.id).unwrap_or_default() {
                if let Some(expected_route_id) = route_id.as_deref() {
                    if upstream.route_id != expected_route_id {
                        continue;
                    }
                }
                if upstream_selector_matches(
                    &upstream,
                    match_target_kind,
                    match_target_ref,
                    match_target_host,
                    match_target_port,
                    match_upstream_scheme,
                ) {
                    matches.push(upstream.id);
                }
            }
        }
    }
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.into_iter().next()),
        _ => Err(anyhow!(
            "multiple upstreams match the provided selector; use id for an exact target"
        )),
    }
}

fn find_hosts_group_by_id(engine: &Arc<RuleEngine>, id: &str) -> Option<wsl_bridge_shared::HostsGroup> {
    engine
        .list_hosts_groups()
        .into_iter()
        .find(|group| group.id == id)
}

fn default_tls_mode(protocol: ProxyProtocol) -> ProxyTlsMode {
    match protocol {
        ProxyProtocol::Http => ProxyTlsMode::Disabled,
        ProxyProtocol::Https => ProxyTlsMode::ManualCert,
    }
}

fn apply_config_patch_already_applied(engine: &Arc<RuleEngine>, idempotency_key: &str) -> bool {
    let marker = format!("idempotency_key={}", idempotency_key.trim());
    engine.tail_logs(0).events.into_iter().any(|event| {
        event.module == "ai"
            && event.event == "config_patch_applied"
            && event.detail.contains(&marker)
    })
}

fn audit_detail(reason: Option<&str>, idempotency_key: Option<&str>, status: &str) -> String {
    let mut parts = vec![format!("status={status}")];
    if let Some(value) = idempotency_key.map(str::trim).filter(|value| !value.is_empty()) {
        parts.push(format!("idempotency_key={value}"));
    }
    if let Some(value) = reason.map(str::trim).filter(|value| !value.is_empty()) {
        parts.push(format!("reason={value}"));
    }
    parts.join(",")
}

fn execute_test_connectivity(engine: &Arc<RuleEngine>, arguments: Value) -> Result<Value> {
    let args: TestConnectivityArgs = serde_json::from_value(arguments)?;
    match args.target.target_type.as_str() {
        "host-port" => {
            let target: HostPortConnectivityTarget = serde_json::from_value(args.target.value)?;
            Ok(test_host_port_connectivity(
                &target.host,
                target.port,
                target.timeout_ms.unwrap_or(2_000),
            ))
        }
        "upstream" => {
            let target: UpstreamConnectivityTarget = serde_json::from_value(args.target.value)?;
            Ok(test_upstream_connectivity(
                engine,
                target.id.as_deref(),
                target.upstream_ref.as_deref(),
                target.timeout_ms.unwrap_or(2_000),
            ))
        }
        "proxy-route" => {
            let target: ProxyRouteConnectivityTarget = serde_json::from_value(args.target.value)?;
            Ok(test_proxy_route_connectivity(
                engine,
                target.id.as_deref(),
                target.route_ref.as_deref(),
                target.host.as_deref(),
                target.path.as_deref(),
                target.timeout_ms.unwrap_or(2_000),
            ))
        }
        "url" => {
            let target: UrlConnectivityTarget = serde_json::from_value(args.target.value)?;
            Ok(test_url_connectivity(
                &target.url,
                target.timeout_ms.unwrap_or(2_000),
            ))
        }
        other => Ok(json!({
          "ok": false,
          "stage": "target_type",
          "status": "unsupported",
          "message": format!("Unsupported connectivity target type: {other}"),
          "suggestions": [
            "Use host-port, upstream, proxy-route, or url."
          ]
        })),
    }
}

fn normalize_export_modules(modules: Vec<String>) -> Result<Vec<String>> {
    let mut normalized = Vec::new();
    for module in modules {
        let value = module.trim().to_ascii_lowercase();
        if value.is_empty() {
            continue;
        }
        if !matches!(value.as_str(), "proxy" | "hosts" | "rules") {
            return Err(anyhow!("unsupported export_config module: {value}"));
        }
        if !normalized.iter().any(|item| item == &value) {
            normalized.push(value);
        }
    }
    if normalized.is_empty() {
        return Err(anyhow!(
            "export_config requires at least one of proxy, hosts, or rules"
        ));
    }
    Ok(normalized)
}

fn build_hosts_import_patch(content: &str) -> Result<(String, Value)> {
    if let Ok(value) = serde_json::from_str::<Value>(content) {
        if let Some(hosts) = value
            .get("content")
            .and_then(|item| item.get("hosts"))
            .or_else(|| value.get("hosts"))
        {
            return build_hosts_import_patch_from_json(hosts)
                .map(|patch| ("hosts-json".to_owned(), patch));
        }
    }

    let parsed = parse_hosts_text(content);
    if parsed.is_empty() {
        return Err(anyhow!(
            "hosts import content does not contain any valid hosts entries"
        ));
    }

    let group_ref = "import-hosts-group";
    let create_records = parsed
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            json!({
              "clientId": format!("import-hosts-record-{index}"),
              "groupRef": group_ref,
              "ip": entry.ip,
              "domain": entry.domain,
              "comment": entry.comment,
              "enabled": true
            })
        })
        .collect::<Vec<_>>();

    Ok((
        "hosts-file".to_owned(),
        json!({
          "version": CONFIG_PATCH_VERSION,
          "reason": "import_config:hosts-file",
          "hosts": {
            "groups": {
              "create": [
                {
                  "clientId": group_ref,
                  "name": generated_import_group_name("Imported Hosts"),
                  "description": "Imported from hosts-file text via import_config"
                }
              ]
            },
            "records": {
              "create": create_records
            }
          }
        }),
    ))
}

fn build_hosts_import_patch_from_json(hosts: &Value) -> Result<Value> {
    let groups = hosts
        .get("groups")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("hosts import JSON must contain a groups array"))?;

    let mut create_groups = Vec::new();
    let mut create_records = Vec::new();
    let mut activate_group_ref = None::<String>;

    for (group_index, item) in groups.iter().enumerate() {
        let group = item
            .get("group")
            .ok_or_else(|| anyhow!("hosts import JSON group item is missing `group`"))?;
        let name = group
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("hosts import JSON group name is required"))?;
        let group_ref = format!("import-hosts-group-{group_index}");
        create_groups.push(json!({
          "clientId": group_ref,
          "name": name,
          "description": group.get("description").cloned().unwrap_or(Value::Null)
        }));
        if group.get("is_active").and_then(Value::as_bool) == Some(true) {
            activate_group_ref = Some(group_ref.clone());
        }

        let entries = item
            .get("entries")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("hosts import JSON group item is missing `entries`"))?;
        for (entry_index, entry) in entries.iter().enumerate() {
            let ip = entry
                .get("ip")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("hosts import JSON entry ip is required"))?;
            let domain = entry
                .get("domain")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("hosts import JSON entry domain is required"))?;
            create_records.push(json!({
              "clientId": format!("import-hosts-record-{group_index}-{entry_index}"),
              "groupRef": group_ref,
              "ip": ip,
              "domain": domain,
              "comment": entry.get("comment").cloned().unwrap_or(Value::Null),
              "enabled": entry.get("enabled").and_then(Value::as_bool).unwrap_or(true)
            }));
        }
    }

    Ok(json!({
      "version": CONFIG_PATCH_VERSION,
      "reason": "import_config:hosts-json",
      "hosts": {
        "groups": {
          "create": create_groups,
          "activate": activate_group_ref.map(|group_ref| json!({ "groupRef": group_ref }))
        },
        "records": {
          "create": create_records
        }
      }
    }))
}

fn build_proxy_import_patch(content: &str) -> Result<(String, Value)> {
    let value = serde_json::from_str::<Value>(content)
        .map_err(|err| anyhow!("proxy import content must be valid JSON: {err}"))?;
    let proxy = value
        .get("content")
        .and_then(|item| item.get("proxy"))
        .or_else(|| value.get("proxy"))
        .ok_or_else(|| anyhow!("proxy import JSON must contain a proxy section"))?;

    let certificates = proxy
        .get("certificates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !certificates.is_empty() {
        return Err(anyhow!(
            "proxy import with certificates is not supported in this build yet"
        ));
    }

    let topology = proxy
        .get("topology")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("proxy import JSON must contain a topology array"))?;
    let mut create_listeners = Vec::new();
    let mut create_routes = Vec::new();
    let mut create_upstreams = Vec::new();

    for (listener_index, item) in topology.iter().enumerate() {
        let listener = item
            .get("listener")
            .ok_or_else(|| anyhow!("proxy topology item is missing `listener`"))?;
        if listener.get("cert_id").is_some() && !listener.get("cert_id").unwrap().is_null() {
            return Err(anyhow!(
                "proxy import with listener cert_id is not supported in this build yet"
            ));
        }
        let listener_ref = format!("import-proxy-listener-{listener_index}");
        create_listeners.push(json!({
          "clientId": listener_ref,
          "name": required_json_string(listener, "name", "proxy listener name")?,
          "bindAddress": required_json_string(listener, "listen_host", "proxy listener listen_host")?,
          "port": required_json_u16(listener, "listen_port", "proxy listener listen_port")?,
          "protocol": listener.get("protocol").cloned().ok_or_else(|| anyhow!("proxy listener protocol is required"))?,
          "tlsMode": listener.get("tls_mode").cloned().unwrap_or(json!("disabled")),
          "certId": Value::Null,
          "bindMode": listener.get("bind_mode").cloned().unwrap_or(json!("all_nics")),
          "nicId": listener.get("nic_id").cloned().unwrap_or(Value::Null),
          "enabled": listener.get("enabled").and_then(Value::as_bool).unwrap_or(true)
        }));

        let routes = item
            .get("routes")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("proxy topology item is missing `routes`"))?;
        for (route_index, route_item) in routes.iter().enumerate() {
            let route = route_item
                .get("route")
                .ok_or_else(|| anyhow!("proxy route item is missing `route`"))?;
            let route_ref = format!("import-proxy-route-{listener_index}-{route_index}");
            create_routes.push(json!({
              "clientId": route_ref,
              "listenerRef": listener_ref,
              "serverNames": route.get("server_names").cloned().unwrap_or_else(|| json!([])),
              "pathPrefix": route.get("path_prefix").cloned().unwrap_or(Value::Null),
              "isDefault": route.get("is_default").and_then(Value::as_bool).unwrap_or(false),
              "enabled": route.get("enabled").and_then(Value::as_bool).unwrap_or(true)
            }));

            let upstreams = route_item
                .get("upstreams")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow!("proxy route item is missing `upstreams`"))?;
            for (upstream_index, upstream) in upstreams.iter().enumerate() {
                create_upstreams.push(json!({
                  "clientId": format!("import-proxy-upstream-{listener_index}-{route_index}-{upstream_index}"),
                  "routeRef": route_ref,
                  "targetType": upstream.get("target_kind").cloned().ok_or_else(|| anyhow!("proxy upstream target_kind is required"))?,
                  "targetRef": upstream.get("target_ref").cloned().unwrap_or(Value::Null),
                  "targetHost": upstream.get("target_host").cloned().unwrap_or(Value::Null),
                  "targetPort": required_json_u16(upstream, "target_port", "proxy upstream target_port")?,
                  "protocol": upstream.get("upstream_scheme").cloned().ok_or_else(|| anyhow!("proxy upstream scheme is required"))?,
                  "pathRewriteFrom": upstream.get("path_rewrite_from").cloned().unwrap_or(Value::Null),
                  "pathRewriteTo": upstream.get("path_rewrite_to").cloned().unwrap_or(Value::Null),
                  "enabled": upstream.get("enabled").and_then(Value::as_bool).unwrap_or(true)
                }));
            }
        }
    }

    Ok((
        "proxy-json".to_owned(),
        json!({
          "version": CONFIG_PATCH_VERSION,
          "reason": "import_config:proxy-json",
          "proxy": {
            "listeners": {
              "create": create_listeners
            },
            "routes": {
              "create": create_routes
            },
            "upstreams": {
              "create": create_upstreams
            }
          }
        }),
    ))
}

fn required_json_string(
    value: &Value,
    key: &str,
    label: &str,
) -> Result<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("{label} is required"))
}

fn required_json_u16(value: &Value, key: &str, label: &str) -> Result<u16> {
    let raw = value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("{label} is required"))?;
    u16::try_from(raw).map_err(|_| anyhow!("{label} must fit in u16"))
}

fn generated_import_group_name(prefix: &str) -> String {
    format!("{prefix} {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"))
}

fn export_config_json_payload(engine: &Arc<RuleEngine>, modules: &[String]) -> Value {
    let mut content = serde_json::Map::new();
    for module in modules {
        match module.as_str() {
            "proxy" => {
                content.insert("proxy".to_owned(), export_proxy_config(engine));
            }
            "hosts" => {
                content.insert("hosts".to_owned(), export_hosts_config(engine));
            }
            "rules" => {
                content.insert("rules".to_owned(), export_rules_config(engine));
            }
            _ => {}
        }
    }

    json!({
      "version": "phase3.ai-export.v1",
      "exportedAt": chrono::Utc::now(),
      "modules": modules,
      "content": content
    })
}

fn export_proxy_config(engine: &Arc<RuleEngine>) -> Value {
    let listeners = engine.list_proxy_listeners();
    let certificates = engine.list_proxy_certificates();
    let topology = listeners
        .into_iter()
        .map(|listener| {
            let routes = engine.list_proxy_routes(&listener.id).unwrap_or_default();
            let route_items = routes
                .into_iter()
                .map(|route| {
                    let upstreams = engine.list_proxy_upstreams(&route.id).unwrap_or_default();
                    json!({
                      "route": route,
                      "upstreams": upstreams
                    })
                })
                .collect::<Vec<_>>();
            json!({
              "listener": listener,
              "routes": route_items
            })
        })
        .collect::<Vec<_>>();

    json!({
      "certificates": certificates,
      "topology": topology
    })
}

fn export_hosts_config(engine: &Arc<RuleEngine>) -> Value {
    let groups = engine
        .list_hosts_groups()
        .into_iter()
        .map(|group| {
            let entries = engine.list_hosts_entries(&group.id).unwrap_or_default();
            json!({
              "group": group,
              "entries": entries
            })
        })
        .collect::<Vec<_>>();
    json!({
      "groups": groups
    })
}

fn export_rules_config(engine: &Arc<RuleEngine>) -> Value {
    let firewall_map = engine
        .list_forward_rules_with_firewall()
        .into_iter()
        .map(|(rule, firewall)| (rule.id, firewall))
        .collect::<HashMap<_, _>>();
    let rules = engine
        .list_rules()
        .into_iter()
        .map(|rule| {
            let firewall = firewall_map.get(&rule.id).cloned();
            json!({
              "rule": rule,
              "firewall": firewall
            })
        })
        .collect::<Vec<_>>();

    json!({
      "legacyMode": true,
      "rules": rules,
      "migrations": engine.list_rule_migrations()
    })
}

fn resolve_export_hosts_group(
    engine: &Arc<RuleEngine>,
    group_ref: Option<&str>,
) -> Result<wsl_bridge_shared::HostsGroup> {
    let groups = engine.list_hosts_groups();
    if let Some(target) = group_ref.map(str::trim).filter(|value| !value.is_empty()) {
        return groups
            .into_iter()
            .find(|group| group.id == target)
            .ok_or_else(|| anyhow!("hosts group not found for export: {target}"));
    }

    groups
        .into_iter()
        .find(|group| group.is_active)
        .ok_or_else(|| {
            anyhow!("hosts-file export requires groupRef or an active hosts group")
        })
}

fn execute_list_agent_targets(arguments: Value) -> Result<Value> {
    let args: ListAgentTargetsArgs = serde_json::from_value(arguments)?;
    list_agent_targets_payload(args.scope, None)
}

pub(crate) fn list_agent_targets_payload(
    scope: Option<String>,
    mcp_config: Option<&McpServerConfig>,
) -> Result<Value> {
    let scope = normalize_install_scope(scope.as_deref());
    let project_root = std::env::current_dir().ok();
    let user_root = resolve_user_home_dir().ok();
    let default_mcp_config;
    let effective_mcp_config = if let Some(config) = mcp_config {
        config
    } else {
        default_mcp_config = McpServerConfig::default();
        &default_mcp_config
    };
    Ok(json!({
      "skill": skill_manifest_summary(),
      "scope": scope,
      "targets": agent_targets()
        .into_iter()
        .map(|target| agent_target_descriptor(
            target,
            scope,
            project_root.as_deref(),
            user_root.as_deref(),
            Some(effective_mcp_config),
        ))
        .collect::<Vec<_>>()
    }))
}

pub(crate) fn install_agent_mcp_client_payload(
    target: String,
    mcp_config: &McpServerConfig,
) -> Result<Value> {
    let target = normalize_agent_target(&target);
    let user_root = resolve_user_home_dir()?;
    let (destination, metadata_path) = install_agent_mcp_client_for_user_root(
        &target,
        &user_root,
        mcp_config,
    )?;

    Ok(json!({
      "ok": true,
      "targetAgent": target,
      "detectedState": "installed",
      "path": destination.display().to_string(),
      "metadataPath": metadata_path.display().to_string()
    }))
}

pub(crate) fn uninstall_agent_mcp_client_payload(
    target: String,
    mcp_config: &McpServerConfig,
) -> Result<Value> {
    let target = normalize_agent_target(&target);
    let user_root = resolve_user_home_dir()?;
    let current_state = detect_agent_mcp_client_state(&target, &user_root, mcp_config)?;
    let (destination, removed) =
        uninstall_agent_mcp_client_for_user_root(&target, &user_root, mcp_config)?;

    Ok(json!({
      "ok": true,
      "targetAgent": target,
      "detectedState": if removed {
        "not_installed".to_owned()
      } else {
        current_state.detected_state
      },
      "path": destination.display().to_string()
    }))
}

fn install_agent_mcp_client_for_user_root(
    target: &str,
    user_root: &Path,
    mcp_config: &McpServerConfig,
) -> Result<(PathBuf, PathBuf)> {
    let state = detect_agent_mcp_client_state(target, user_root, mcp_config)?;
    if !state.install_supported {
        return Err(anyhow!("automatic MCP client installation is not supported for this agent"));
    }
    if state.detected_state == "conflict" {
        return Err(anyhow!(
            "mcp client conflict detected at target path; resolve the existing entry manually before installing"
        ));
    }

    let destination = agent_mcp_client_path(target, user_root)
        .ok_or_else(|| anyhow!("resolve MCP client path failed"))?;
    let metadata_path = opencode_client_config_sidecar_path(&destination);
    let rendered = render_opencode_client_config(
        &destination,
        mcp_config,
        "wsl-bridge-operator",
        "0.1.0",
        "skill-directory",
    )?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&destination, rendered)?;
    fs::write(
        &metadata_path,
        render_opencode_client_config_metadata(
            mcp_config,
            "wsl-bridge-operator",
            "0.1.0",
            "skill-directory",
        )?,
    )?;
    Ok((destination, metadata_path))
}

fn uninstall_agent_mcp_client_for_user_root(
    target: &str,
    user_root: &Path,
    mcp_config: &McpServerConfig,
) -> Result<(PathBuf, bool)> {
    let state = detect_agent_mcp_client_state(target, user_root, mcp_config)?;
    if !state.install_supported {
        return Err(anyhow!("automatic MCP client uninstallation is not supported for this agent"));
    }
    if state.detected_state == "conflict" {
        return Err(anyhow!(
            "mcp client conflict detected at target path; resolve the existing entry manually before uninstalling"
        ));
    }

    let destination = agent_mcp_client_path(target, user_root)
        .ok_or_else(|| anyhow!("resolve MCP client path failed"))?;
    let removed = remove_managed_opencode_client_config(
        &destination,
        "wsl-bridge-operator",
        "skill-directory",
    )?;
    Ok((destination, removed))
}

fn cleanup_legacy_project_opencode_mcp_client(project_root: &Path) -> Result<()> {
    let legacy_path = project_root.join("opencode.json");
    if !legacy_path.exists() {
        return Ok(());
    }
    let _ = remove_managed_opencode_client_config(
        &legacy_path,
        "wsl-bridge-operator",
        "skill-directory",
    )?;
    Ok(())
}

fn execute_install_agent_skill(engine: &Arc<RuleEngine>, arguments: Value) -> Result<Value> {
    let args: InstallAgentSkillArgs = serde_json::from_value(arguments)?;
    let result = install_agent_skill_payload(
        args.target,
        args.scope,
        args.mode,
        args.fallback_to_agents_dir,
        args.project_root,
        Some(&engine.get_mcp_config()),
    )?;
    if result.get("mode").and_then(Value::as_str) == Some("apply")
        && result.get("ok").and_then(Value::as_bool) == Some(true)
    {
        let target = result
            .get("targetAgent")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let install_type = result
            .get("installType")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        engine.append_audit_log(
            "info",
            "ai",
            "agent_skill_installed",
            &format!("target={target},install_type={install_type}"),
        );
    }
    Ok(result)
}

fn execute_uninstall_agent_skill(engine: &Arc<RuleEngine>, arguments: Value) -> Result<Value> {
    let args: UninstallAgentSkillArgs = serde_json::from_value(arguments)?;
    let result = uninstall_agent_skill_payload(
        args.target,
        args.scope,
        args.mode,
        args.fallback_to_agents_dir,
        args.project_root,
    )?;
    if result.get("mode").and_then(Value::as_str) == Some("apply")
        && result.get("ok").and_then(Value::as_bool) == Some(true)
    {
        let target = result
            .get("targetAgent")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let install_type = result
            .get("installType")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let detected_state = result
            .get("detectedState")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        engine.append_audit_log(
            "info",
            "ai",
            "agent_skill_uninstalled",
            &format!("target={target},install_type={install_type},detected_state={detected_state}"),
        );
    }
    Ok(result)
}

pub(crate) fn install_agent_skill_payload(
    target: String,
    scope: Option<String>,
    mode: Option<String>,
    fallback_to_agents_dir: Option<bool>,
    project_root: Option<String>,
    mcp_config: Option<&McpServerConfig>,
) -> Result<Value> {
    let mode = mode.as_deref().unwrap_or("dryRun");
    if mode != "dryRun" && mode != "apply" {
        return Err(anyhow!(
            "install_agent_skill mode must be either dryRun or apply"
        ));
    }

    let scope = normalize_install_scope(scope.as_deref());
    let fallback_to_agents_dir = fallback_to_agents_dir.unwrap_or(true);
    let target = normalize_agent_target(&target);
    let user_root = resolve_user_home_dir()?;
    let effective_project_root = resolve_agent_project_root(project_root.as_deref())?;
    let mut plan = build_agent_skill_install_plan(
        &target,
        scope,
        fallback_to_agents_dir,
        &effective_project_root,
        &user_root,
    )?;
    let detection = detect_agent_skill_installation(&plan, &effective_project_root, &user_root)?;
    plan.detected_state = Some(detection.state.clone());
    if mode == "dryRun" {
        return Ok(serde_json::to_value(plan)?);
    }
    if detection.state == "conflict" {
        return Err(anyhow!(
            "skill conflict detected at target path; resolve the existing skill manually before installing"
        ));
    }
    if target == "opencode" && scope == "project" {
        cleanup_legacy_project_opencode_mcp_client(&effective_project_root)?;
    }

    let default_config;
    let effective_mcp_config = if let Some(config) = mcp_config {
        config
    } else {
        default_config = McpServerConfig::default();
        &default_config
    };
    let applied = install_agent_skill_plan(
        &plan,
        &effective_project_root,
        &user_root,
        effective_mcp_config,
    )?;
    Ok(json!({
      "ok": true,
      "mode": "apply",
      "operation": "install",
      "skill": skill_manifest_summary(),
      "targetAgent": plan.target_agent,
      "scope": plan.scope,
      "installType": plan.install_type,
      "detectedState": "installed",
      "rootPath": plan.root_path,
      "resolvedPaths": applied,
      "writes": plan.writes,
      "deletes": [],
      "warnings": plan.warnings,
      "appliedPaths": applied
    }))
}

pub(crate) fn uninstall_agent_skill_payload(
    target: String,
    scope: Option<String>,
    mode: Option<String>,
    fallback_to_agents_dir: Option<bool>,
    project_root: Option<String>,
) -> Result<Value> {
    let mode = mode.as_deref().unwrap_or("dryRun");
    if mode != "dryRun" && mode != "apply" {
        return Err(anyhow!(
            "uninstall_agent_skill mode must be either dryRun or apply"
        ));
    }

    let scope = normalize_install_scope(scope.as_deref());
    let fallback_to_agents_dir = fallback_to_agents_dir.unwrap_or(true);
    let target = normalize_agent_target(&target);
    let user_root = resolve_user_home_dir()?;
    let effective_project_root = resolve_agent_project_root(project_root.as_deref())?;
    let plan = build_agent_skill_uninstall_plan(
        &target,
        scope,
        fallback_to_agents_dir,
        &effective_project_root,
        &user_root,
    )?;
    if mode == "dryRun" {
        return Ok(serde_json::to_value(plan)?);
    }
    if plan.detected_state.as_deref() == Some("conflict") {
        return Err(anyhow!(
            "skill conflict detected at target path; resolve the existing skill manually before uninstalling"
        ));
    }

    let deleted = uninstall_agent_skill_plan(&plan, &effective_project_root, &user_root)?;
    Ok(json!({
      "ok": true,
      "mode": "apply",
      "operation": "uninstall",
      "skill": skill_manifest_summary(),
      "targetAgent": plan.target_agent,
      "scope": plan.scope,
      "installType": plan.install_type,
      "detectedState": plan.detected_state.unwrap_or_else(|| "unknown".to_owned()),
      "rootPath": plan.root_path,
      "resolvedPaths": deleted,
      "writes": [],
      "deletes": plan.deletes,
      "warnings": plan.warnings,
      "deletedPaths": deleted
    }))
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
    state_rules_resource(engine, detail)
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
          "code": "PROXY_PATCH_TARGETING_HINT",
          "target": "proxy",
          "message": "For existing Proxy objects, prefer stable ids from wsl-bridge://state/proxy. Use listenerRef/routeRef/upstreamRef only for objects created earlier in the same patch."
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

#[derive(Default)]
struct DryRunAccumulator {
    warnings: Vec<Value>,
    conflicts: Vec<Value>,
    creates: Vec<Value>,
    updates: Vec<Value>,
    deletes: Vec<Value>,
    summary_counts: BTreeMap<String, usize>,
    runtime_restart_required: bool,
    requires_admin: bool,
    requires_confirmation: bool,
}

impl DryRunAccumulator {
    fn summary(&self) -> Vec<String> {
        self.summary_counts
            .iter()
            .map(|(key, count)| format!("{key} {count}"))
            .collect()
    }

    fn add_summary(&mut self, label: &str) {
        *self.summary_counts.entry(label.to_owned()).or_insert(0) += 1;
    }

    fn push_create(&mut self, resource: &str, target: &str, detail: Value) {
        self.creates.push(json!({
          "resource": resource,
          "target": target,
          "detail": detail
        }));
    }

    fn push_update(&mut self, resource: &str, target: &str, detail: Value) {
        self.updates.push(json!({
          "resource": resource,
          "target": target,
          "detail": detail
        }));
    }

    fn push_delete(&mut self, resource: &str, target: &str, detail: Value) {
        self.deletes.push(json!({
          "resource": resource,
          "target": target,
          "detail": detail
        }));
    }

    fn warning(&mut self, severity: &str, code: &str, target: &str, message: impl Into<String>) {
        self.warnings.push(json!({
          "severity": severity,
          "code": code,
          "target": target,
          "message": message.into()
        }));
    }

    fn conflict(&mut self, code: &str, target: &str, message: impl Into<String>) {
        self.conflicts.push(json!({
          "code": code,
          "target": target,
          "message": message.into()
        }));
    }
}

#[derive(Clone)]
struct PendingListener {
    reference: String,
    listen_host: String,
    listen_port: u16,
}

#[derive(Clone)]
struct PendingRoute {
    reference: String,
    listener_ref: String,
    server_names: Vec<String>,
    path_prefix: Option<String>,
    is_default: bool,
}

#[allow(dead_code)]
#[derive(Clone)]
struct PendingUpstream {
    reference: String,
    route_ref: Option<String>,
    target_kind: TargetKind,
    target_ref: Option<String>,
    target_host: Option<String>,
    target_port: u16,
    upstream_scheme: UpstreamScheme,
}

#[allow(dead_code)]
#[derive(Clone)]
struct PendingHostsGroup {
    reference: String,
    name: String,
}

#[allow(dead_code)]
#[derive(Clone)]
struct PendingHostsRecord {
    reference: String,
    group_ref: String,
    domain: String,
}

struct DryRunContext {
    listeners: HashMap<String, wsl_bridge_shared::ProxyListener>,
    routes: HashMap<String, wsl_bridge_shared::ProxyRoute>,
    upstreams: HashMap<String, wsl_bridge_shared::ProxyUpstream>,
    route_upstream_counts: HashMap<String, usize>,
    listener_route_counts: HashMap<String, usize>,
    groups: HashMap<String, wsl_bridge_shared::HostsGroup>,
    entries: HashMap<String, wsl_bridge_shared::HostsEntry>,
    group_entry_counts: HashMap<String, usize>,
    listener_bindings: HashMap<String, String>,
    route_keys: HashSet<String>,
    default_route_listeners: HashSet<String>,
    group_names: HashSet<String>,
    created_listeners: HashMap<String, PendingListener>,
    created_routes: HashMap<String, PendingRoute>,
    created_upstreams: HashMap<String, PendingUpstream>,
    created_groups: HashMap<String, PendingHostsGroup>,
    created_records: HashMap<String, PendingHostsRecord>,
}

impl DryRunContext {
    fn new(engine: &Arc<RuleEngine>) -> Self {
        let listeners = engine
            .list_proxy_listeners()
            .into_iter()
            .map(|item| (item.id.clone(), item))
            .collect::<HashMap<_, _>>();
        let mut routes = HashMap::new();
        let mut upstreams = HashMap::new();
        let mut route_upstream_counts = HashMap::new();
        let mut listener_route_counts = HashMap::new();
        let mut route_keys = HashSet::new();
        let mut default_route_listeners = HashSet::new();
        for listener in listeners.values() {
            let items = engine.list_proxy_routes(&listener.id).unwrap_or_default();
            listener_route_counts.insert(listener.id.clone(), items.len());
            for route in items {
                if route.is_default {
                    default_route_listeners.insert(route.listener_id.clone());
                }
                route_keys.insert(route_unique_key(
                    &route.listener_id,
                    &route.server_names,
                    route.path_prefix.as_deref(),
                ));
                let route_id = route.id.clone();
                let upstream_items = engine.list_proxy_upstreams(&route_id).unwrap_or_default();
                route_upstream_counts.insert(route_id.clone(), upstream_items.len());
                for upstream in upstream_items {
                    upstreams.insert(upstream.id.clone(), upstream);
                }
                routes.insert(route_id, route);
            }
        }

        let groups = engine
            .list_hosts_groups()
            .into_iter()
            .map(|item| (item.id.clone(), item))
            .collect::<HashMap<_, _>>();
        let mut entries = HashMap::new();
        let mut group_entry_counts = HashMap::new();
        let mut group_names = HashSet::new();
        for group in groups.values() {
            group_names.insert(group.name.trim().to_ascii_lowercase());
            let items = engine.list_hosts_entries(&group.id).unwrap_or_default();
            group_entry_counts.insert(group.id.clone(), items.len());
            for entry in items {
                entries.insert(entry.id.clone(), entry);
            }
        }

        Self {
            listener_bindings: listeners
                .values()
                .map(|item| {
                    (
                        listener_binding_key(&item.listen_host, item.listen_port),
                        item.id.clone(),
                    )
                })
                .collect(),
            listeners,
            routes,
            upstreams,
            route_upstream_counts,
            listener_route_counts,
            groups,
            entries,
            group_entry_counts,
            route_keys,
            default_route_listeners,
            group_names,
            created_listeners: HashMap::new(),
            created_routes: HashMap::new(),
            created_upstreams: HashMap::new(),
            created_groups: HashMap::new(),
            created_records: HashMap::new(),
        }
    }
}

fn dry_run_proxy_patch(
    ctx: &mut DryRunContext,
    acc: &mut DryRunAccumulator,
    patch: &ProxyPatchInput,
) {
    let declared_upstream_refs = patch
        .upstreams
        .as_ref()
        .map(|ops| {
            ops.create
                .iter()
                .enumerate()
                .map(|(index, item)| patch_reference(item.client_id.as_deref(), "upstream", index))
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();

    if let Some(listeners) = patch.listeners.as_ref() {
        for (index, item) in listeners.create.iter().enumerate() {
            let reference = patch_reference(item.client_id.as_deref(), "listener", index);
            let host = item.bind_address.trim();
            if item.name.trim().is_empty() {
                acc.conflict(
                    "LISTENER_NAME_REQUIRED",
                    &format!("proxy.listeners.create[{index}]"),
                    "Listener name is required.",
                );
                continue;
            }
            if host.is_empty() || item.port == 0 {
                acc.conflict(
                    "LISTENER_BIND_INVALID",
                    &format!("proxy.listeners.create[{index}]"),
                    "Listener bind address and port are required.",
                );
                continue;
            }
            let binding_key = listener_binding_key(host, item.port);
            if ctx.listener_bindings.contains_key(&binding_key) {
                acc.conflict(
                    "PORT_CONFLICT",
                    &format!("proxy.listeners.create[{index}]"),
                    format!("Port {} is already used by another listener.", item.port),
                );
                continue;
            }
            if host == "0.0.0.0" {
                acc.warning(
                    "warning",
                    "LAN_EXPOSURE",
                    &reference,
                    format!("{host}:{} will be accessible from LAN.", item.port),
                );
                acc.requires_confirmation = true;
            }
            ctx.listener_bindings.insert(binding_key, reference.clone());
            ctx.created_listeners.insert(
                reference.clone(),
                PendingListener {
                    reference: reference.clone(),
                    listen_host: host.to_owned(),
                    listen_port: item.port,
                },
            );
            acc.runtime_restart_required = true;
            acc.add_summary("Create listener");
            acc.push_create(
                "proxy.listener",
                &reference,
                json!({
                  "name": item.name.trim(),
                  "listenHost": host,
                  "listenPort": item.port,
                  "protocol": item.protocol
                }),
            );
        }

        for item in &listeners.update {
            let target = match resolve_listener_selector(
                ctx,
                item.id.as_deref(),
                item.listener_ref.as_deref(),
                item.listener_name.as_deref(),
            ) {
                SelectorResolution::Found(target) => target,
                SelectorResolution::Missing => {
                    acc.conflict(
                        "LISTENER_NOT_FOUND",
                        "proxy.listeners.update",
                        "Listener update target does not exist.",
                    );
                    continue;
                }
                SelectorResolution::Ambiguous(message) => {
                    acc.conflict("LISTENER_SELECTOR_AMBIGUOUS", "proxy.listeners.update", message);
                    continue;
                }
            };
            let base = if let Some(listener) = ctx.listeners.get(&target) {
                (
                    listener.name.clone(),
                    listener.listen_host.clone(),
                    listener.listen_port,
                    listener.protocol,
                )
            } else if let Some(listener) = ctx.created_listeners.get(&target) {
                (
                    listener.reference.clone(),
                    listener.listen_host.clone(),
                    listener.listen_port,
                    ProxyProtocol::Http,
                )
            } else {
                acc.conflict(
                    "LISTENER_NOT_FOUND",
                    &target,
                    "Listener update target does not exist.",
                );
                continue;
            };
            let next_host = item.bind_address.as_deref().unwrap_or(&base.1).trim().to_owned();
            let next_port = item.port.unwrap_or(base.2);
            let binding_key = listener_binding_key(&next_host, next_port);
            if let Some(owner) = ctx.listener_bindings.get(&binding_key) {
                if owner != &target {
                    acc.conflict(
                        "PORT_CONFLICT",
                        &target,
                        format!("Port {} is already used by another listener.", next_port),
                    );
                    continue;
                }
            }
            if next_host == "0.0.0.0" {
                acc.warning(
                    "warning",
                    "LAN_EXPOSURE",
                    &target,
                    format!("{next_host}:{next_port} will be accessible from LAN."),
                );
                acc.requires_confirmation = true;
            }
            acc.runtime_restart_required = true;
            acc.add_summary("Update listener");
            acc.push_update(
                "proxy.listener",
                &target,
                json!({
                  "name": item.name.as_deref().unwrap_or(&base.0),
                  "listenHost": next_host,
                  "listenPort": next_port,
                  "protocol": item.protocol.unwrap_or(base.3),
                  "enabled": item.enabled
                }),
            );
        }

        for item in &listeners.delete {
            let target = match resolve_listener_selector(
                ctx,
                item.id.as_deref(),
                item.listener_ref.as_deref(),
                item.listener_name.as_deref(),
            ) {
                SelectorResolution::Found(target) => target,
                SelectorResolution::Missing => {
                    acc.conflict(
                        "LISTENER_NOT_FOUND",
                        "proxy.listeners.delete",
                        "Listener delete target does not exist.",
                    );
                    continue;
                }
                SelectorResolution::Ambiguous(message) => {
                    acc.conflict("LISTENER_SELECTOR_AMBIGUOUS", "proxy.listeners.delete", message);
                    continue;
                }
            };
            let cascaded_routes = ctx.listener_route_counts.get(&target).copied().unwrap_or_default();
            if cascaded_routes > 0 {
                let cascaded_upstreams = ctx
                    .routes
                    .values()
                    .filter(|route| route.listener_id == target)
                    .map(|route| ctx.route_upstream_counts.get(&route.id).copied().unwrap_or_default())
                    .sum::<usize>();
                acc.warning(
                    "warning",
                    "LISTENER_DELETE_CASCADE",
                    &target,
                    format!(
                        "Deleting this listener will also remove {cascaded_routes} routes and {cascaded_upstreams} upstreams."
                    ),
                );
            }
            acc.runtime_restart_required = true;
            acc.requires_confirmation = true;
            acc.add_summary("Delete listener");
            acc.push_delete(
                "proxy.listener",
                &target,
                json!({
                  "cascadeRoutes": cascaded_routes
                }),
            );
        }
    }

    if let Some(routes) = patch.routes.as_ref() {
        for (index, item) in routes.create.iter().enumerate() {
            let reference = patch_reference(item.client_id.as_deref(), "route", index);
            let Some(listener_ref) = resolve_listener_ref(ctx, None, Some(&item.listener_ref)) else {
                acc.conflict(
                    "ROUTE_LISTENER_NOT_FOUND",
                    &format!("proxy.routes.create[{index}]"),
                    format!("Listener reference `{}` does not exist.", item.listener_ref),
                );
                continue;
            };
            let server_names = normalize_patch_server_names(&item.server_names);
            let path_prefix = normalize_patch_path(item.path_prefix.as_deref(), false, &reference, acc);
            let key = route_unique_key(&listener_ref, &server_names, path_prefix.as_deref());
            if ctx.route_keys.contains(&key) {
                acc.conflict(
                    "ROUTE_DUPLICATE",
                    &reference,
                    "A route with the same listener, server names and path prefix already exists.",
                );
                continue;
            }
            if item.is_default.unwrap_or(false) && ctx.default_route_listeners.contains(&listener_ref) {
                acc.conflict(
                    "DEFAULT_ROUTE_CONFLICT",
                    &reference,
                    "The listener already has a default route.",
                );
                continue;
            }
            if let Some(upstream_ref) = item.upstream_ref.as_deref() {
                let upstream_exists = ctx.upstreams.contains_key(upstream_ref)
                    || ctx.created_upstreams.contains_key(upstream_ref)
                    || declared_upstream_refs.contains(upstream_ref);
                if !upstream_exists {
                    acc.conflict(
                        "ROUTE_UPSTREAM_NOT_FOUND",
                        &reference,
                        format!("Upstream reference `{upstream_ref}` does not exist."),
                    );
                    continue;
                }
            }
            ctx.route_keys.insert(key);
            if item.is_default.unwrap_or(false) {
                ctx.default_route_listeners.insert(listener_ref.clone());
            }
            ctx.created_routes.insert(
                reference.clone(),
                PendingRoute {
                    reference: reference.clone(),
                    listener_ref: listener_ref.clone(),
                    server_names: server_names.clone(),
                    path_prefix: path_prefix.clone(),
                    is_default: item.is_default.unwrap_or(false),
                },
            );
            acc.runtime_restart_required = true;
            acc.add_summary("Create route");
            acc.push_create(
                "proxy.route",
                &reference,
                json!({
                  "listenerRef": listener_ref,
                  "serverNames": server_names,
                  "pathPrefix": path_prefix,
                  "isDefault": item.is_default.unwrap_or(false),
                  "enabled": item.enabled.unwrap_or(true)
                }),
            );
        }

        for item in &routes.update {
            let target = match resolve_route_selector(
                ctx,
                item.id.as_deref(),
                item.route_ref.as_deref(),
                item.match_listener_ref.as_deref(),
                item.match_listener_name.as_deref(),
                item.match_server_names.as_deref(),
                item.match_path_prefix.as_ref().map(|value| value.as_deref()),
                item.match_is_default,
            ) {
                SelectorResolution::Found(target) => target,
                SelectorResolution::Missing => {
                    acc.conflict("ROUTE_NOT_FOUND", "proxy.routes.update", "Route update target does not exist.");
                    continue;
                }
                SelectorResolution::Ambiguous(message) => {
                    acc.conflict("ROUTE_SELECTOR_AMBIGUOUS", "proxy.routes.update", message);
                    continue;
                }
            };
            let Some(route) = ctx.routes.get(&target).cloned().or_else(|| ctx.created_routes.get(&target).map(|pending| wsl_bridge_shared::ProxyRoute {
                id: pending.reference.clone(),
                listener_id: pending.listener_ref.clone(),
                server_names: pending.server_names.clone(),
                path_prefix: pending.path_prefix.clone(),
                is_default: pending.is_default,
                enabled: true,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            })) else {
                acc.conflict("ROUTE_NOT_FOUND", &target, "Route update target does not exist.");
                continue;
            };
            let server_names = item
                .server_names
                .as_ref()
                .map(|values| normalize_patch_server_names(values))
                .unwrap_or(route.server_names.clone());
            let path_prefix = item
                .path_prefix
                .as_ref()
                .map(|value| normalize_patch_path(value.as_deref(), true, &target, acc))
                .unwrap_or(route.path_prefix.clone());
            let key = route_unique_key(&route.listener_id, &server_names, path_prefix.as_deref());
            if ctx.route_keys.contains(&key)
                && route_unique_key(&route.listener_id, &route.server_names, route.path_prefix.as_deref()) != key
            {
                acc.conflict(
                    "ROUTE_DUPLICATE",
                    &target,
                    "A route with the same listener, server names and path prefix already exists.",
                );
                continue;
            }
            acc.runtime_restart_required = true;
            acc.add_summary("Update route");
            acc.push_update(
                "proxy.route",
                &target,
                json!({
                  "serverNames": server_names,
                  "pathPrefix": path_prefix,
                  "isDefault": item.is_default.unwrap_or(route.is_default),
                  "enabled": item.enabled
                }),
            );
        }

        for item in &routes.delete {
            let target = match resolve_route_selector(
                ctx,
                item.id.as_deref(),
                item.route_ref.as_deref(),
                item.match_listener_ref.as_deref(),
                item.match_listener_name.as_deref(),
                item.match_server_names.as_deref(),
                item.match_path_prefix.as_ref().map(|value| value.as_deref()),
                item.match_is_default,
            ) {
                SelectorResolution::Found(target) => target,
                SelectorResolution::Missing => {
                    acc.conflict("ROUTE_NOT_FOUND", "proxy.routes.delete", "Route delete target does not exist.");
                    continue;
                }
                SelectorResolution::Ambiguous(message) => {
                    acc.conflict("ROUTE_SELECTOR_AMBIGUOUS", "proxy.routes.delete", message);
                    continue;
                }
            };
            let cascaded_upstreams = ctx.route_upstream_counts.get(&target).copied().unwrap_or_default();
            if cascaded_upstreams > 0 {
                acc.warning(
                    "warning",
                    "ROUTE_DELETE_CASCADE",
                    &target,
                    format!("Deleting this route will also remove {cascaded_upstreams} upstreams."),
                );
            }
            acc.runtime_restart_required = true;
            acc.requires_confirmation = true;
            acc.add_summary("Delete route");
            acc.push_delete(
                "proxy.route",
                &target,
                json!({
                  "cascadeUpstreams": cascaded_upstreams
                }),
            );
        }
    }

    if let Some(upstreams) = patch.upstreams.as_ref() {
        let inferred_route_refs = patch
            .routes
            .as_ref()
            .map(|ops| {
                ops.create
                    .iter()
                    .enumerate()
                    .filter_map(|(index, route)| {
                        route.upstream_ref.as_ref().map(|upstream_ref| {
                            (
                                upstream_ref.clone(),
                                patch_reference(route.client_id.as_deref(), "route", index),
                            )
                        })
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();

        for (index, item) in upstreams.create.iter().enumerate() {
            let reference = patch_reference(item.client_id.as_deref(), "upstream", index);
            let route_ref = item
                .route_ref
                .clone()
                .or_else(|| inferred_route_refs.get(&reference).cloned());
            if let Some(route_ref) = route_ref.as_deref() {
                if resolve_route_ref(ctx, None, Some(route_ref)).is_none() {
                    acc.conflict(
                        "UPSTREAM_ROUTE_NOT_FOUND",
                        &reference,
                        format!("Route reference `{route_ref}` does not exist."),
                    );
                    continue;
                }
            }
            validate_upstream_target(
                &reference,
                item.target_kind,
                item.target_ref.as_deref(),
                item.target_host.as_deref(),
                item.target_port,
                acc,
            );
            ctx.created_upstreams.insert(
                reference.clone(),
                PendingUpstream {
                    reference: reference.clone(),
                    route_ref: route_ref.clone(),
                    target_kind: item.target_kind,
                    target_ref: item.target_ref.clone(),
                    target_host: item.target_host.clone(),
                    target_port: item.target_port,
                    upstream_scheme: item.upstream_scheme,
                },
            );
            acc.runtime_restart_required = true;
            acc.add_summary("Create upstream");
            acc.push_create(
                "proxy.upstream",
                &reference,
                json!({
                  "routeRef": route_ref,
                  "targetKind": item.target_kind,
                  "targetRef": item.target_ref,
                  "targetHost": item.target_host,
                  "targetPort": item.target_port,
                  "upstreamScheme": item.upstream_scheme
                }),
            );
        }

        for item in &upstreams.update {
            let target = match resolve_upstream_selector(
                ctx,
                item.id.as_deref(),
                item.upstream_ref.as_deref(),
                item.match_route_ref.as_deref(),
                item.match_listener_ref.as_deref(),
                item.match_listener_name.as_deref(),
                item.match_server_names.as_deref(),
                item.match_path_prefix.as_ref().map(|value| value.as_deref()),
                item.match_is_default,
                item.match_target_kind,
                item.match_target_ref.as_deref(),
                item.match_target_host.as_deref(),
                item.match_target_port,
                item.match_upstream_scheme,
            ) {
                SelectorResolution::Found(target) => target,
                SelectorResolution::Missing => {
                    acc.conflict("UPSTREAM_NOT_FOUND", "proxy.upstreams.update", "Upstream update target does not exist.");
                    continue;
                }
                SelectorResolution::Ambiguous(message) => {
                    acc.conflict("UPSTREAM_SELECTOR_AMBIGUOUS", "proxy.upstreams.update", message);
                    continue;
                }
            };
            let base = ctx.upstreams.get(&target).cloned().or_else(|| {
                ctx.created_upstreams.get(&target).map(|pending| wsl_bridge_shared::ProxyUpstream {
                    id: pending.reference.clone(),
                    route_id: pending.route_ref.clone().unwrap_or_default(),
                    target_kind: pending.target_kind,
                    target_ref: pending.target_ref.clone(),
                    target_host: pending.target_host.clone(),
                    target_port: pending.target_port,
                    upstream_scheme: pending.upstream_scheme,
                    path_rewrite_from: None,
                    path_rewrite_to: None,
                    enabled: true,
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                })
            });
            let Some(base_upstream) = base else {
                acc.conflict("UPSTREAM_NOT_FOUND", &target, "Upstream update target does not exist.");
                continue;
            };
            let target_kind = item.target_kind.unwrap_or(base_upstream.target_kind);
            let target_port = item.target_port.unwrap_or(base_upstream.target_port);
            validate_upstream_target(
                &target,
                target_kind,
                item.target_ref.as_deref().or(base_upstream.target_ref.as_deref()),
                item.target_host.as_deref().or(base_upstream.target_host.as_deref()),
                target_port,
                acc,
            );
            acc.runtime_restart_required = true;
            acc.add_summary("Update upstream");
            acc.push_update(
                "proxy.upstream",
                &target,
                json!({
                  "targetKind": target_kind,
                  "targetRef": item.target_ref.as_deref().or(base_upstream.target_ref.as_deref()),
                  "targetHost": item.target_host.as_deref().or(base_upstream.target_host.as_deref()),
                  "targetPort": target_port,
                  "upstreamScheme": item.upstream_scheme.unwrap_or(base_upstream.upstream_scheme)
                }),
            );
        }

        for item in &upstreams.delete {
            let target = match resolve_upstream_selector(
                ctx,
                item.id.as_deref(),
                item.upstream_ref.as_deref(),
                item.match_route_ref.as_deref(),
                item.match_listener_ref.as_deref(),
                item.match_listener_name.as_deref(),
                item.match_server_names.as_deref(),
                item.match_path_prefix.as_ref().map(|value| value.as_deref()),
                item.match_is_default,
                item.match_target_kind,
                item.match_target_ref.as_deref(),
                item.match_target_host.as_deref(),
                item.match_target_port,
                item.match_upstream_scheme,
            ) {
                SelectorResolution::Found(target) => target,
                SelectorResolution::Missing => {
                    acc.conflict("UPSTREAM_NOT_FOUND", "proxy.upstreams.delete", "Upstream delete target does not exist.");
                    continue;
                }
                SelectorResolution::Ambiguous(message) => {
                    acc.conflict("UPSTREAM_SELECTOR_AMBIGUOUS", "proxy.upstreams.delete", message);
                    continue;
                }
            };
            acc.runtime_restart_required = true;
            acc.requires_confirmation = true;
            acc.add_summary("Delete upstream");
            acc.push_delete("proxy.upstream", &target, json!({}));
        }
    }
}

fn dry_run_hosts_patch(
    ctx: &mut DryRunContext,
    acc: &mut DryRunAccumulator,
    patch: &HostsPatchInput,
) {
    if let Some(groups) = patch.groups.as_ref() {
        for (index, item) in groups.create.iter().enumerate() {
            let reference = patch_reference(item.client_id.as_deref(), "hosts-group", index);
            let name = item.name.trim();
            if name.is_empty() {
                acc.conflict(
                    "HOSTS_GROUP_NAME_REQUIRED",
                    &reference,
                    "Hosts group name is required.",
                );
                continue;
            }
            let normalized = name.to_ascii_lowercase();
            if ctx.group_names.contains(&normalized) {
                acc.warning(
                    "info",
                    "HOSTS_GROUP_NAME_DUPLICATE",
                    &reference,
                    format!("A hosts group named `{name}` already exists."),
                );
            }
            ctx.group_names.insert(normalized);
            ctx.created_groups.insert(
                reference.clone(),
                PendingHostsGroup {
                    reference: reference.clone(),
                    name: name.to_owned(),
                },
            );
            acc.add_summary("Create hosts group");
            acc.push_create(
                "hosts.group",
                &reference,
                json!({
                  "name": name,
                  "description": item.description
                }),
            );
        }

        for item in &groups.update {
            let Some(target) = resolve_hosts_group_ref(ctx, item.id.as_deref(), item.group_ref.as_deref()) else {
                acc.conflict("HOSTS_GROUP_NOT_FOUND", "hosts.groups.update", "Hosts group update target does not exist.");
                continue;
            };
            let name = item
                .name
                .as_deref()
                .or_else(|| ctx.groups.get(&target).map(|group| group.name.as_str()))
                .or_else(|| ctx.created_groups.get(&target).map(|group| group.name.as_str()))
                .unwrap_or_default()
                .trim()
                .to_owned();
            if name.is_empty() {
                acc.conflict(
                    "HOSTS_GROUP_NAME_REQUIRED",
                    &target,
                    "Hosts group name is required.",
                );
                continue;
            }
            acc.add_summary("Update hosts group");
            acc.push_update(
                "hosts.group",
                &target,
                json!({
                  "name": name,
                  "description": item.description
                }),
            );
        }

        for item in &groups.delete {
            let Some(target) = resolve_hosts_group_ref(ctx, item.id.as_deref(), item.group_ref.as_deref()) else {
                acc.conflict("HOSTS_GROUP_NOT_FOUND", "hosts.groups.delete", "Hosts group delete target does not exist.");
                continue;
            };
            if ctx.groups.get(&target).map(|group| group.is_active).unwrap_or(false) {
                acc.conflict(
                    "HOSTS_GROUP_ACTIVE_DELETE_BLOCKED",
                    &target,
                    "Active hosts group cannot be deleted.",
                );
                continue;
            }
            let entry_count = ctx.group_entry_counts.get(&target).copied().unwrap_or_default();
            acc.requires_confirmation = true;
            acc.add_summary("Delete hosts group");
            acc.push_delete(
                "hosts.group",
                &target,
                json!({
                  "cascadeEntries": entry_count
                }),
            );
        }

        if let Some(activate) = groups.activate.as_ref() {
            let Some(target) = resolve_hosts_group_ref(ctx, None, Some(&activate.group_ref)) else {
                acc.conflict("HOSTS_GROUP_NOT_FOUND", "hosts.groups.activate", "Hosts group activation target does not exist.");
                return;
            };
            acc.warning(
                "warning",
                "HOSTS_ACTIVATION_REQUIRES_ADMIN",
                &target,
                "Activating a hosts group writes the system hosts file and requires administrator privileges.",
            );
            acc.requires_admin = true;
            acc.requires_confirmation = true;
            acc.add_summary("Activate hosts group");
            acc.push_update("hosts.group", &target, json!({ "activate": true }));
        }
    }

    if let Some(records) = patch.records.as_ref() {
        for (index, item) in records.create.iter().enumerate() {
            let reference = patch_reference(item.client_id.as_deref(), "hosts-record", index);
            let Some(group_ref) = resolve_hosts_group_ref(ctx, None, Some(&item.group_ref)) else {
                acc.conflict(
                    "HOSTS_GROUP_NOT_FOUND",
                    &reference,
                    format!("Hosts group reference `{}` does not exist.", item.group_ref),
                );
                continue;
            };
            validate_hosts_record_input(&reference, &item.ip, &item.domain, acc);
            ctx.created_records.insert(
                reference.clone(),
                PendingHostsRecord {
                    reference: reference.clone(),
                    group_ref: group_ref.clone(),
                    domain: item.domain.trim().to_ascii_lowercase(),
                },
            );
            acc.add_summary("Create hosts record");
            acc.push_create(
                "hosts.record",
                &reference,
                json!({
                  "groupRef": group_ref,
                  "ip": item.ip.trim(),
                  "domain": item.domain.trim(),
                  "enabled": item.enabled.unwrap_or(true)
                }),
            );
        }

        for item in &records.update {
            let Some(target) = resolve_hosts_record_ref(ctx, item.id.as_deref(), item.record_ref.as_deref()) else {
                acc.conflict("HOSTS_RECORD_NOT_FOUND", "hosts.records.update", "Hosts record update target does not exist.");
                continue;
            };
            let current = ctx.entries.get(&target).cloned();
            let group_ref = item
                .group_ref
                .as_deref()
                .and_then(|value| resolve_hosts_group_ref(ctx, None, Some(value)))
                .or_else(|| current.as_ref().map(|entry| entry.group_id.clone()))
                .or_else(|| ctx.created_records.get(&target).map(|entry| entry.group_ref.clone()));
            let Some(group_ref) = group_ref else {
                acc.conflict("HOSTS_GROUP_NOT_FOUND", &target, "Hosts record group does not exist.");
                continue;
            };
            let ip = item
                .ip
                .as_deref()
                .or_else(|| current.as_ref().map(|entry| entry.ip.as_str()))
                .unwrap_or_default();
            let domain = item
                .domain
                .as_deref()
                .or_else(|| current.as_ref().map(|entry| entry.domain.as_str()))
                .unwrap_or_default();
            validate_hosts_record_input(&target, ip, domain, acc);
            acc.add_summary("Update hosts record");
            acc.push_update(
                "hosts.record",
                &target,
                json!({
                  "groupRef": group_ref,
                  "ip": ip,
                  "domain": domain,
                  "enabled": item.enabled
                }),
            );
        }

        for item in &records.delete {
            let Some(target) = resolve_hosts_record_ref(ctx, item.id.as_deref(), item.record_ref.as_deref()) else {
                acc.conflict("HOSTS_RECORD_NOT_FOUND", "hosts.records.delete", "Hosts record delete target does not exist.");
                continue;
            };
            acc.add_summary("Delete hosts record");
            acc.push_delete("hosts.record", &target, json!({}));
        }
    }
}

fn patch_reference(value: Option<&str>, prefix: &str, index: usize) -> String {
    value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{prefix}-{}", index + 1))
}

fn listener_binding_key(host: &str, port: u16) -> String {
    format!("{}:{port}", host.trim().to_ascii_lowercase())
}

fn route_unique_key(listener_ref: &str, server_names: &[String], path_prefix: Option<&str>) -> String {
    format!(
        "{}|{}|{}",
        listener_ref.trim().to_ascii_lowercase(),
        server_names.join(","),
        path_prefix.unwrap_or("/")
    )
}

fn normalize_patch_server_names(values: &[String]) -> Vec<String> {
    let mut items = values
        .iter()
        .map(|item| item.trim().to_ascii_lowercase())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    if items.is_empty() {
        items.push("*".to_owned());
    }
    items.sort();
    items.dedup();
    items
}

fn normalize_patch_path(
    value: Option<&str>,
    allow_none: bool,
    target: &str,
    acc: &mut DryRunAccumulator,
) -> Option<String> {
    let Some(value) = value.map(str::trim) else {
        return None;
    };
    if value.is_empty() {
        return None;
    }
    if !value.starts_with('/') {
        acc.conflict(
            "PATH_PREFIX_INVALID",
            target,
            "Path values must start with `/`.",
        );
        return None;
    }
    if allow_none && value == "/" {
        return Some("/".to_owned());
    }
    Some(value.to_owned())
}

fn validate_upstream_target(
    target: &str,
    target_kind: TargetKind,
    target_ref: Option<&str>,
    target_host: Option<&str>,
    target_port: u16,
    acc: &mut DryRunAccumulator,
) {
    if target_port == 0 {
        acc.conflict(
            "UPSTREAM_TARGET_PORT_REQUIRED",
            target,
            "Upstream target port must be > 0.",
        );
    }
    match target_kind {
        TargetKind::Static => {
            if target_host.map(str::trim).filter(|item| !item.is_empty()).is_none() {
                acc.conflict(
                    "UPSTREAM_TARGET_HOST_REQUIRED",
                    target,
                    "Static upstream requires targetHost.",
                );
            }
        }
        TargetKind::Wsl | TargetKind::Hyperv => {
            if target_ref.map(str::trim).filter(|item| !item.is_empty()).is_none() {
                acc.conflict(
                    "UPSTREAM_TARGET_REF_REQUIRED",
                    target,
                    "WSL / Hyper-V upstream requires targetRef.",
                );
            }
        }
    }
}

fn test_host_port_connectivity(host: &str, port: u16, timeout_ms: u64) -> Value {
    let host = host.trim();
    if host.is_empty() || port == 0 {
        return json!({
          "ok": false,
          "stage": "input",
          "status": "invalid",
          "message": "Host and port are required for host-port connectivity tests."
        });
    }

    match probe_tcp_connect(host, port, timeout_ms) {
        Ok(duration_ms) => json!({
          "ok": true,
          "stage": "host_port_connect",
          "status": "connected",
          "message": format!("Connected to {host}:{port}."),
          "latencyMs": duration_ms,
          "target": {
            "host": host,
            "port": port
          }
        }),
        Err(err) => json!({
          "ok": false,
          "stage": "host_port_connect",
          "status": "failed",
          "message": format!("Failed to connect to {host}:{port}: {err}"),
          "suggestions": [
            "Check whether the target service is running.",
            "Check whether the target host and port are correct.",
            "Check local firewall or network isolation."
          ]
        }),
    }
}

fn test_upstream_connectivity(
    engine: &Arc<RuleEngine>,
    id: Option<&str>,
    upstream_ref: Option<&str>,
    timeout_ms: u64,
) -> Value {
    let Some(upstream) = resolve_upstream_by_ref(engine, id, upstream_ref) else {
        return json!({
          "ok": false,
          "stage": "resolve_upstream",
          "status": "not_found",
          "message": "Upstream target was not found."
        });
    };
    let target_label = upstream_label(&upstream);
    if matches!(
        upstream.upstream_scheme,
        UpstreamScheme::Grpc | UpstreamScheme::Grpcs
    ) {
        return json!({
          "ok": false,
          "stage": "protocol",
          "status": "unsupported",
          "message": "gRPC connectivity probe is not implemented in the current build.",
          "target": {
            "upstreamId": upstream.id,
            "label": target_label
          }
        });
    }

    let Some((host, port)) = resolve_upstream_host_port(engine, &upstream) else {
        return json!({
          "ok": false,
          "stage": "resolve_target",
          "status": "failed",
          "message": format!("Unable to resolve upstream target for {}.", target_label),
          "suggestions": [
            "Check targetRef / targetHost configuration.",
            "Check whether the WSL distribution or Hyper-V VM is running."
          ]
        });
    };

    let tcp_result = match probe_tcp_connect(&host, port, timeout_ms) {
        Ok(duration_ms) => duration_ms,
        Err(err) => {
            return json!({
              "ok": false,
              "stage": "upstream_connect",
              "status": "failed",
              "message": format!("Resolved upstream target {host}:{port} but connection failed: {err}"),
              "target": {
                "upstreamId": upstream.id,
                "label": target_label,
                "host": host,
                "port": port
              },
              "suggestions": [
                "Check whether the target service is listening on the expected port.",
                "Check whether the WSL / Hyper-V target is running.",
                "Check local firewall or bridged network reachability."
              ]
            });
        }
    };

    let protocol_probe = match upstream.upstream_scheme {
        UpstreamScheme::Http => probe_http_request(&host, port, "/", None, timeout_ms),
        UpstreamScheme::Ws => probe_websocket_upgrade(&host, port, "/", None, timeout_ms),
        UpstreamScheme::Https | UpstreamScheme::Wss => Err("TLS-level probe is not implemented in the current build.".to_owned()),
        UpstreamScheme::Grpc | UpstreamScheme::Grpcs => unreachable!(),
    };

    match protocol_probe {
        Ok(detail) => json!({
          "ok": true,
          "stage": "response",
          "status": "reachable",
          "message": detail,
          "latencyMs": tcp_result,
          "target": {
            "upstreamId": upstream.id,
            "label": target_label,
            "host": host,
            "port": port,
            "scheme": upstream.upstream_scheme
          }
        }),
        Err(message) if matches!(upstream.upstream_scheme, UpstreamScheme::Https | UpstreamScheme::Wss) => json!({
          "ok": true,
          "stage": "upstream_connect",
          "status": "partial",
          "message": format!("TCP connectivity to {host}:{port} is healthy. {message}"),
          "latencyMs": tcp_result,
          "target": {
            "upstreamId": upstream.id,
            "label": target_label,
            "host": host,
            "port": port,
            "scheme": upstream.upstream_scheme
          }
        }),
        Err(message) => json!({
          "ok": false,
          "stage": "response",
          "status": "failed",
          "message": format!("Connected to {host}:{port} but protocol probe failed: {message}"),
          "latencyMs": tcp_result,
          "target": {
            "upstreamId": upstream.id,
            "label": target_label,
            "host": host,
            "port": port,
            "scheme": upstream.upstream_scheme
          }
        }),
    }
}

fn test_proxy_route_connectivity(
    engine: &Arc<RuleEngine>,
    id: Option<&str>,
    route_ref: Option<&str>,
    host: Option<&str>,
    path: Option<&str>,
    timeout_ms: u64,
) -> Value {
    let Some((listener, route)) = resolve_route_with_listener(engine, id, route_ref) else {
        return json!({
          "ok": false,
          "stage": "resolve_route",
          "status": "not_found",
          "message": "Proxy route target was not found."
        });
    };

    let runtime_items = engine.get_proxy_runtime_status();
    let Some(runtime) = runtime_items.iter().find(|item| item.listener_id == listener.id) else {
        return json!({
          "ok": false,
          "stage": "listener_runtime",
          "status": "failed",
          "message": "Listener runtime state is unavailable for the selected route."
        });
    };
    if !matches!(runtime.state, wsl_bridge_shared::RuntimeState::Running) {
        return json!({
          "ok": false,
          "stage": "listener_listen",
          "status": "failed",
          "message": runtime.last_error.clone().unwrap_or_else(|| "Listener is not running.".to_owned()),
          "target": {
            "listenerId": listener.id,
            "routeId": route.id
          }
        });
    }

    let connect_host = listener_probe_host(&listener.listen_host);
    if let Err(err) = probe_tcp_connect(&connect_host, listener.listen_port, timeout_ms) {
        return json!({
          "ok": false,
          "stage": "listener_listen",
          "status": "failed",
          "message": format!("Listener runtime is marked running but {}:{} is not reachable: {err}", connect_host, listener.listen_port),
          "target": {
            "listenerId": listener.id,
            "routeId": route.id
          }
        });
    }

    let route_host = host
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| route.server_names.first().cloned().unwrap_or_else(|| "localhost".to_owned()));
    let route_path = path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| route.path_prefix.as_deref().unwrap_or("/"));
    let listener_routes = engine.list_proxy_routes(&listener.id).unwrap_or_default();
    let matched_route = select_proxy_route_for_test(&listener_routes, &route_host, route_path);
    let Some(matched_route) = matched_route else {
        return json!({
          "ok": false,
          "stage": "route_match",
          "status": "failed",
          "message": format!("No enabled route on listener `{}` matches host `{}` and path `{}`.", listener.name, route_host, route_path)
        });
    };
    if matched_route.id != route.id {
        return json!({
          "ok": false,
          "stage": "route_match",
          "status": "failed",
          "message": format!("Host `{}` and path `{}` are matched by route `{}` instead of the requested route `{}`.", route_host, route_path, matched_route.id, route.id)
        });
    }

    let upstreams = engine.list_proxy_upstreams(&route.id).unwrap_or_default();
    let Some(upstream) = select_upstream_for_test(&upstreams) else {
        return json!({
          "ok": false,
          "stage": "upstream_select",
          "status": "failed",
          "message": "No enabled upstream is available for the selected route."
        });
    };

    let upstream_result = test_upstream_connectivity(engine, Some(&upstream.id), None, timeout_ms);
    if upstream_result.get("ok").and_then(Value::as_bool) == Some(true) {
        json!({
          "ok": true,
          "stage": "upstream_connect",
          "status": "reachable",
          "message": format!("Listener, route match, and upstream probe all succeeded for route `{}`.", route.id),
          "target": {
            "listenerId": listener.id,
            "routeId": route.id,
            "host": route_host,
            "path": route_path
          },
          "upstream": upstream_result
        })
    } else {
        json!({
          "ok": false,
          "stage": "upstream_connect",
          "status": "failed",
          "message": "Listener is running and route matches, but upstream connectivity failed.",
          "target": {
            "listenerId": listener.id,
            "routeId": route.id,
            "host": route_host,
            "path": route_path
          },
          "upstream": upstream_result
        })
    }
}

fn test_url_connectivity(url: &str, timeout_ms: u64) -> Value {
    let Some(parsed) = parse_connectivity_url(url) else {
        return json!({
          "ok": false,
          "stage": "input",
          "status": "invalid",
          "message": "URL is invalid or unsupported."
        });
    };

    let tcp = match probe_tcp_connect(&parsed.host, parsed.port, timeout_ms) {
        Ok(duration_ms) => duration_ms,
        Err(err) => {
            return json!({
              "ok": false,
              "stage": "host_port_connect",
              "status": "failed",
              "message": format!("Failed to connect to {}:{}: {err}", parsed.host, parsed.port),
              "target": {
                "url": url
              }
            });
        }
    };

    match parsed.scheme.as_str() {
        "http" => match probe_http_request(&parsed.host, parsed.port, &parsed.path, Some(&parsed.host), timeout_ms) {
            Ok(message) => json!({
              "ok": true,
              "stage": "response",
              "status": "reachable",
              "message": message,
              "latencyMs": tcp,
              "target": {
                "url": url
              }
            }),
            Err(err) => json!({
              "ok": false,
              "stage": "response",
              "status": "failed",
              "message": err,
              "target": {
                "url": url
              }
            }),
        },
        "ws" => match probe_websocket_upgrade(&parsed.host, parsed.port, &parsed.path, Some(&parsed.host), timeout_ms) {
            Ok(message) => json!({
              "ok": true,
              "stage": "response",
              "status": "reachable",
              "message": message,
              "latencyMs": tcp,
              "target": {
                "url": url
              }
            }),
            Err(err) => json!({
              "ok": false,
              "stage": "response",
              "status": "failed",
              "message": err,
              "target": {
                "url": url
              }
            }),
        },
        "https" | "wss" => json!({
          "ok": true,
          "stage": "host_port_connect",
          "status": "partial",
          "message": format!("TCP connectivity to {}:{} succeeded. TLS-level probe for {} is not implemented in the current build.", parsed.host, parsed.port, parsed.scheme),
          "latencyMs": tcp,
          "target": {
            "url": url
          }
        }),
        "grpc" | "grpcs" => json!({
          "ok": false,
          "stage": "protocol",
          "status": "unsupported",
          "message": "gRPC connectivity probe is not implemented in the current build.",
          "target": {
            "url": url
          }
        }),
        _ => json!({
          "ok": false,
          "stage": "protocol",
          "status": "unsupported",
          "message": format!("Unsupported URL scheme: {}", parsed.scheme),
          "target": {
            "url": url
          }
        }),
    }
}

fn probe_tcp_connect(host: &str, port: u16, timeout_ms: u64) -> Result<u128, String> {
    let timeout = Duration::from_millis(timeout_ms.max(200));
    let mut last_error = None;
    let start = Instant::now();
    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|err| format!("resolve address failed: {err}"))?;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => {
                let _ = stream.shutdown(Shutdown::Both);
                return Ok(start.elapsed().as_millis());
            }
            Err(err) => {
                last_error = Some(err.to_string());
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "no reachable address candidates".to_owned()))
}

fn probe_http_request(
    host: &str,
    port: u16,
    path: &str,
    host_header: Option<&str>,
    timeout_ms: u64,
) -> Result<String, String> {
    let timeout = Duration::from_millis(timeout_ms.max(200));
    let addr = first_socket_addr(host, port)?;
    let mut stream =
        TcpStream::connect_timeout(&addr, timeout).map_err(|err| format!("connect failed: {err}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|err| format!("set read timeout failed: {err}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|err| format!("set write timeout failed: {err}"))?;
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        normalize_probe_path(path),
        host_header.unwrap_or(host)
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|err| format!("write request failed: {err}"))?;
    let mut buf = [0u8; 256];
    let read = stream
        .read(&mut buf)
        .map_err(|err| format!("read response failed: {err}"))?;
    let text = String::from_utf8_lossy(&buf[..read]);
    let line = text.lines().next().unwrap_or_default();
    if line.starts_with("HTTP/") {
        Ok(format!("HTTP probe succeeded with response `{line}`."))
    } else {
        Err("HTTP probe did not receive a valid HTTP status line.".to_owned())
    }
}

fn probe_websocket_upgrade(
    host: &str,
    port: u16,
    path: &str,
    host_header: Option<&str>,
    timeout_ms: u64,
) -> Result<String, String> {
    let timeout = Duration::from_millis(timeout_ms.max(200));
    let addr = first_socket_addr(host, port)?;
    let mut stream =
        TcpStream::connect_timeout(&addr, timeout).map_err(|err| format!("connect failed: {err}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|err| format!("set read timeout failed: {err}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|err| format!("set write timeout failed: {err}"))?;
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Key: test-key\r\nSec-WebSocket-Version: 13\r\n\r\n",
        normalize_probe_path(path),
        host_header.unwrap_or(host)
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|err| format!("write websocket upgrade failed: {err}"))?;
    let mut buf = [0u8; 256];
    let read = stream
        .read(&mut buf)
        .map_err(|err| format!("read websocket response failed: {err}"))?;
    let text = String::from_utf8_lossy(&buf[..read]);
    let line = text.lines().next().unwrap_or_default();
    if line.contains("101") {
        Ok("WebSocket upgrade probe succeeded with HTTP 101 Switching Protocols.".to_owned())
    } else {
        Err(format!("WebSocket upgrade probe did not receive 101 response. First line: `{line}`"))
    }
}

fn first_socket_addr(host: &str, port: u16) -> Result<std::net::SocketAddr, String> {
    (host, port)
        .to_socket_addrs()
        .map_err(|err| format!("resolve address failed: {err}"))?
        .next()
        .ok_or_else(|| "no socket address candidates found".to_owned())
}

fn normalize_probe_path(path: &str) -> &str {
    let trimmed = path.trim();
    if trimmed.is_empty() { "/" } else { trimmed }
}

fn resolve_upstream_by_ref(
    engine: &Arc<RuleEngine>,
    id: Option<&str>,
    upstream_ref: Option<&str>,
) -> Option<wsl_bridge_shared::ProxyUpstream> {
    let target = id.or(upstream_ref)?.trim();
    engine
        .list_proxy_listeners()
        .into_iter()
        .flat_map(|listener| engine.list_proxy_routes(&listener.id).unwrap_or_default())
        .flat_map(|route| engine.list_proxy_upstreams(&route.id).unwrap_or_default())
        .find(|upstream| upstream.id == target)
}

fn resolve_route_with_listener(
    engine: &Arc<RuleEngine>,
    id: Option<&str>,
    route_ref: Option<&str>,
) -> Option<(wsl_bridge_shared::ProxyListener, wsl_bridge_shared::ProxyRoute)> {
    let target = id.or(route_ref)?.trim();
    for listener in engine.list_proxy_listeners() {
        if let Some(route) = engine
            .list_proxy_routes(&listener.id)
            .unwrap_or_default()
            .into_iter()
            .find(|route| route.id == target)
        {
            return Some((listener, route));
        }
    }
    None
}

fn resolve_upstream_host_port(
    engine: &Arc<RuleEngine>,
    upstream: &wsl_bridge_shared::ProxyUpstream,
) -> Option<(String, u16)> {
    let port = upstream.target_port;
    let host = match upstream.target_kind {
        TargetKind::Static => upstream.target_host.as_ref().map(|value| value.trim().to_owned()),
        TargetKind::Wsl => {
            let key = upstream.target_ref.as_deref().unwrap_or("").trim();
            if key.is_empty() {
                upstream.target_host.as_ref().map(|value| value.trim().to_owned())
            } else {
                engine
                    .scan_topology()
                    .wsl
                    .into_iter()
                    .find(|item| item.distro.eq_ignore_ascii_case(key))
                    .and_then(|item| item.ip)
                    .or_else(|| upstream.target_host.as_ref().map(|value| value.trim().to_owned()))
            }
        }
        TargetKind::Hyperv => {
            let key = upstream.target_ref.as_deref().unwrap_or("").trim();
            if key.is_empty() {
                upstream.target_host.as_ref().map(|value| value.trim().to_owned())
            } else {
                engine
                    .scan_topology()
                    .hyperv
                    .into_iter()
                    .find(|item| item.vm_name.eq_ignore_ascii_case(key))
                    .and_then(|item| item.ip)
                    .or_else(|| upstream.target_host.as_ref().map(|value| value.trim().to_owned()))
            }
        }
    }?;
    Some((host, port))
}

fn listener_probe_host(listen_host: &str) -> String {
    match listen_host.trim() {
        "0.0.0.0" => "127.0.0.1".to_owned(),
        "::" | "[::]" => "::1".to_owned(),
        value => value.to_owned(),
    }
}

fn select_proxy_route_for_test(
    routes: &[wsl_bridge_shared::ProxyRoute],
    host: &str,
    path: &str,
) -> Option<wsl_bridge_shared::ProxyRoute> {
    let host = normalize_route_host(host);
    let mut best: Option<(u8, usize, chrono::DateTime<chrono::Utc>, wsl_bridge_shared::ProxyRoute)> =
        None;
    for route in routes.iter().filter(|route| route.enabled) {
        let Some(score) = classify_route_for_test(route, &host, path) else {
            continue;
        };
        let candidate = (
            score,
            route.path_prefix.as_deref().unwrap_or("").len(),
            route.created_at,
            route.clone(),
        );
        let is_better = best.as_ref().map(|current| {
            candidate.0 > current.0
                || (candidate.0 == current.0
                    && (candidate.1 > current.1
                        || (candidate.1 == current.1 && candidate.2 > current.2)))
        });
        if is_better.unwrap_or(true) {
            best = Some(candidate);
        }
    }
    best.map(|(_, _, _, route)| route)
}

fn classify_route_for_test(
    route: &wsl_bridge_shared::ProxyRoute,
    host: &str,
    path: &str,
) -> Option<u8> {
    if let Some(prefix) = route.path_prefix.as_deref() {
        if prefix != "/" && !path.starts_with(prefix) {
            return None;
        }
    }
    if route.is_default {
        return Some(1);
    }
    let mut best = 0u8;
    for pattern in &route.server_names {
        let pattern = normalize_route_host(pattern);
        if pattern == host {
            best = best.max(3);
        } else if let Some(suffix) = pattern.strip_prefix("*.") {
            if host.ends_with(&format!(".{suffix}")) {
                best = best.max(2);
            }
        } else if let Some(suffix) = pattern.strip_prefix('.') {
            if host == suffix || host.ends_with(&format!(".{suffix}")) {
                best = best.max(2);
            }
        }
    }
    (best > 0).then_some(best)
}

fn normalize_route_host(value: &str) -> String {
    value
        .trim()
        .trim_end_matches('.')
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn select_upstream_for_test(
    upstreams: &[wsl_bridge_shared::ProxyUpstream],
) -> Option<wsl_bridge_shared::ProxyUpstream> {
    upstreams
        .iter()
        .filter(|upstream| upstream.enabled)
        .max_by(|left, right| left.created_at.cmp(&right.created_at))
        .cloned()
}

fn upstream_label(upstream: &wsl_bridge_shared::ProxyUpstream) -> String {
    match upstream.target_kind {
        TargetKind::Static => format!(
            "{}:{}",
            upstream.target_host.as_deref().unwrap_or(""),
            upstream.target_port
        ),
        TargetKind::Wsl | TargetKind::Hyperv => format!(
            "{}:{}",
            upstream.target_ref.as_deref().unwrap_or(""),
            upstream.target_port
        ),
    }
}

struct ParsedConnectivityUrl {
    scheme: String,
    host: String,
    port: u16,
    path: String,
}

fn parse_connectivity_url(url: &str) -> Option<ParsedConnectivityUrl> {
    let trimmed = url.trim();
    let (scheme, rest) = trimmed.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let (authority, path) = match rest.split_once('/') {
        Some((authority, tail)) => (authority, format!("/{}", tail)),
        None => (rest, "/".to_owned()),
    };
    let default_port = match scheme.as_str() {
        "http" | "ws" => 80,
        "https" | "wss" | "grpcs" => 443,
        "grpc" => 80,
        _ => return None,
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port_text)) if !host.contains(']') || host.ends_with(']') => {
            let port = port_text.parse::<u16>().ok()?;
            (host.trim_matches(['[', ']']), port)
        }
        _ => (authority.trim_matches(['[', ']']), default_port),
    };
    if host.trim().is_empty() {
        return None;
    }
    Some(ParsedConnectivityUrl {
        scheme,
        host: host.to_owned(),
        port,
        path,
    })
}

fn validate_hosts_record_input(
    target: &str,
    ip: &str,
    domain: &str,
    acc: &mut DryRunAccumulator,
) {
    if ip.trim().parse::<IpAddr>().is_err() {
        acc.conflict("HOSTS_IP_INVALID", target, "Hosts record IP must be a valid IPv4 or IPv6 address.");
    }
    if domain.trim().is_empty() {
        acc.conflict("HOSTS_DOMAIN_REQUIRED", target, "Hosts record domain is required.");
    }
}

fn resolve_listener_ref(
    ctx: &DryRunContext,
    id: Option<&str>,
    listener_ref: Option<&str>,
) -> Option<String> {
    resolve_reference(ctx.listeners.keys(), ctx.created_listeners.keys(), id, listener_ref)
}

enum SelectorResolution {
    Found(String),
    Missing,
    Ambiguous(String),
}

fn normalize_selector_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| item.to_ascii_lowercase())
}

fn normalize_selector_path(value: Option<Option<&str>>) -> Option<Option<String>> {
    value.map(|item| {
        item.map(str::trim)
            .filter(|path| !path.is_empty())
            .map(str::to_owned)
    })
}

fn route_selector_matches(
    listener_id: &str,
    server_names: &[String],
    path_prefix: Option<&str>,
    is_default: bool,
    match_listener_id: Option<&str>,
    match_server_names: Option<&[String]>,
    match_path_prefix: Option<Option<&str>>,
    match_is_default: Option<bool>,
) -> bool {
    if let Some(expected_listener_id) = normalize_selector_text(match_listener_id) {
        if listener_id.trim().to_ascii_lowercase() != expected_listener_id {
            return false;
        }
    }
    if let Some(expected_server_names) = match_server_names {
        if normalize_patch_server_names(server_names) != normalize_patch_server_names(expected_server_names) {
            return false;
        }
    }
    if let Some(expected_path_prefix) = normalize_selector_path(match_path_prefix) {
        let current = path_prefix.map(str::to_owned);
        if current != expected_path_prefix {
            return false;
        }
    }
    if let Some(expected_is_default) = match_is_default {
        if is_default != expected_is_default {
            return false;
        }
    }
    true
}

fn upstream_selector_matches(
    upstream: &wsl_bridge_shared::ProxyUpstream,
    match_target_kind: Option<TargetKind>,
    match_target_ref: Option<&str>,
    match_target_host: Option<&str>,
    match_target_port: Option<u16>,
    match_upstream_scheme: Option<UpstreamScheme>,
) -> bool {
    if let Some(expected_target_kind) = match_target_kind {
        if upstream.target_kind != expected_target_kind {
            return false;
        }
    }
    if let Some(expected_target_ref) = normalize_selector_text(match_target_ref) {
        if normalize_selector_text(upstream.target_ref.as_deref()) != Some(expected_target_ref) {
            return false;
        }
    }
    if let Some(expected_target_host) = normalize_selector_text(match_target_host) {
        if normalize_selector_text(upstream.target_host.as_deref()) != Some(expected_target_host) {
            return false;
        }
    }
    if let Some(expected_target_port) = match_target_port {
        if upstream.target_port != expected_target_port {
            return false;
        }
    }
    if let Some(expected_scheme) = match_upstream_scheme {
        if upstream.upstream_scheme != expected_scheme {
            return false;
        }
    }
    true
}

fn resolve_listener_selector(
    ctx: &DryRunContext,
    id: Option<&str>,
    listener_ref: Option<&str>,
    listener_name: Option<&str>,
) -> SelectorResolution {
    if let Some(target) = resolve_reference(ctx.listeners.keys(), ctx.created_listeners.keys(), id, listener_ref) {
        return SelectorResolution::Found(target);
    }
    let Some(listener_name) = normalize_selector_text(listener_name) else {
        return SelectorResolution::Missing;
    };
    let matches = ctx
        .listeners
        .values()
        .filter(|listener| listener.name.trim().eq_ignore_ascii_case(&listener_name))
        .map(|listener| listener.id.clone())
        .collect::<Vec<_>>();
    match matches.len() {
        1 => SelectorResolution::Found(matches[0].clone()),
        0 => SelectorResolution::Missing,
        _ => SelectorResolution::Ambiguous(format!(
            "Multiple listeners match name `{}`. Use id for an exact target.",
            listener_name
        )),
    }
}

fn resolve_route_selector(
    ctx: &DryRunContext,
    id: Option<&str>,
    route_ref: Option<&str>,
    match_listener_ref: Option<&str>,
    match_listener_name: Option<&str>,
    match_server_names: Option<&[String]>,
    match_path_prefix: Option<Option<&str>>,
    match_is_default: Option<bool>,
) -> SelectorResolution {
    if let Some(target) = resolve_reference(ctx.routes.keys(), ctx.created_routes.keys(), id, route_ref) {
        return SelectorResolution::Found(target);
    }

    let listener_id = match resolve_listener_selector(ctx, None, match_listener_ref, match_listener_name) {
        SelectorResolution::Found(value) => Some(value),
        SelectorResolution::Missing => None,
        SelectorResolution::Ambiguous(message) => return SelectorResolution::Ambiguous(message),
    };
    let matches = ctx
        .routes
        .values()
        .filter(|route| {
            route_selector_matches(
                &route.listener_id,
                &route.server_names,
                route.path_prefix.as_deref(),
                route.is_default,
                listener_id.as_deref(),
                match_server_names,
                match_path_prefix,
                match_is_default,
            )
        })
        .map(|route| route.id.clone())
        .collect::<Vec<_>>();
    match matches.len() {
        1 => SelectorResolution::Found(matches[0].clone()),
        0 => SelectorResolution::Missing,
        _ => SelectorResolution::Ambiguous(
            "Multiple routes match the provided selector. Use id for an exact target.".to_owned(),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_upstream_selector(
    ctx: &DryRunContext,
    id: Option<&str>,
    upstream_ref: Option<&str>,
    match_route_ref: Option<&str>,
    match_listener_ref: Option<&str>,
    match_listener_name: Option<&str>,
    match_server_names: Option<&[String]>,
    match_path_prefix: Option<Option<&str>>,
    match_is_default: Option<bool>,
    match_target_kind: Option<TargetKind>,
    match_target_ref: Option<&str>,
    match_target_host: Option<&str>,
    match_target_port: Option<u16>,
    match_upstream_scheme: Option<UpstreamScheme>,
) -> SelectorResolution {
    if let Some(target) = resolve_reference(ctx.upstreams.keys(), ctx.created_upstreams.keys(), id, upstream_ref) {
        return SelectorResolution::Found(target);
    }

    let route_id = match resolve_route_selector(
        ctx,
        None,
        match_route_ref,
        match_listener_ref,
        match_listener_name,
        match_server_names,
        match_path_prefix,
        match_is_default,
    ) {
        SelectorResolution::Found(value) => Some(value),
        SelectorResolution::Missing => None,
        SelectorResolution::Ambiguous(message) => return SelectorResolution::Ambiguous(message),
    };
    let matches = ctx
        .upstreams
        .values()
        .filter(|upstream| {
            if let Some(expected_route_id) = route_id.as_deref() {
                if upstream.route_id != expected_route_id {
                    return false;
                }
            }
            upstream_selector_matches(
                upstream,
                match_target_kind,
                match_target_ref,
                match_target_host,
                match_target_port,
                match_upstream_scheme,
            )
        })
        .map(|upstream| upstream.id.clone())
        .collect::<Vec<_>>();
    match matches.len() {
        1 => SelectorResolution::Found(matches[0].clone()),
        0 => SelectorResolution::Missing,
        _ => SelectorResolution::Ambiguous(
            "Multiple upstreams match the provided selector. Use id for an exact target.".to_owned(),
        ),
    }
}

fn resolve_route_ref(
    ctx: &DryRunContext,
    id: Option<&str>,
    route_ref: Option<&str>,
) -> Option<String> {
    resolve_reference(ctx.routes.keys(), ctx.created_routes.keys(), id, route_ref)
}

#[allow(dead_code)]
fn resolve_upstream_ref(
    ctx: &DryRunContext,
    id: Option<&str>,
    upstream_ref: Option<&str>,
) -> Option<String> {
    resolve_reference(ctx.upstreams.keys(), ctx.created_upstreams.keys(), id, upstream_ref)
}

fn resolve_hosts_group_ref(
    ctx: &DryRunContext,
    id: Option<&str>,
    group_ref: Option<&str>,
) -> Option<String> {
    resolve_reference(ctx.groups.keys(), ctx.created_groups.keys(), id, group_ref)
}

fn resolve_hosts_record_ref(
    ctx: &DryRunContext,
    id: Option<&str>,
    record_ref: Option<&str>,
) -> Option<String> {
    resolve_reference(ctx.entries.keys(), ctx.created_records.keys(), id, record_ref)
}

fn resolve_reference<'a, I, J>(
    existing: I,
    created: J,
    id: Option<&str>,
    patch_ref: Option<&str>,
) -> Option<String>
where
    I: IntoIterator<Item = &'a String>,
    J: IntoIterator<Item = &'a String>,
{
    let candidate = id.or(patch_ref)?.trim();
    if candidate.is_empty() {
        return None;
    }
    if existing.into_iter().any(|item| item == candidate)
        || created.into_iter().any(|item| item == candidate)
    {
        Some(candidate.to_owned())
    } else {
        None
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
        "cursor" => "cursor".to_owned(),
        "codex" => "codex".to_owned(),
        _ => "generic".to_owned(),
    }
}

fn agent_target_descriptor(
    target: &str,
    scope: &str,
    project_root: Option<&Path>,
    user_root: Option<&Path>,
    mcp_config: Option<&McpServerConfig>,
) -> Value {
    let install_type = "skill-directory";
    let fallback_to_agents_dir = target == "generic";
    let apply_supported = true;
    let (detected, global_path) = if let (Some(project_root), Some(user_root)) = (project_root, user_root) {
        let plan = build_agent_skill_install_plan(target, "user", fallback_to_agents_dir, project_root, user_root);
        match plan.and_then(|plan| detect_agent_skill_installation(&plan, project_root, user_root)) {
            Ok(detection) => (
                detection.state,
                detection
                    .files
                    .first()
                    .map(|item| item.destination.display().to_string())
                    .unwrap_or_default(),
            ),
            Err(_) => ("unknown".to_owned(), String::new()),
        }
    } else {
        ("unknown".to_owned(), String::new())
    };
    let mcp_client = if let (Some(user_root), Some(mcp_config)) = (user_root, mcp_config) {
        detect_agent_mcp_client_state(target, user_root, mcp_config).unwrap_or(AgentMcpClientState {
            target_agent: target.to_owned(),
            install_supported: false,
            detected_state: "unknown".to_owned(),
            path: None,
        })
    } else {
        AgentMcpClientState {
            target_agent: target.to_owned(),
            install_supported: false,
            detected_state: "unknown".to_owned(),
            path: None,
        }
    };

    json!({
      "id": target,
      "displayName": agent_target_display_name(target),
      "scope": scope,
      "detected": detected,
      "mcpDetected": mcp_client.detected_state,
      "mcpInstallSupported": mcp_client.install_supported,
      "mcpGlobalPath": mcp_client.path,
      "supportsNativeSkill": target == "claude-code",
      "supportsProjectInstall": true,
      "supportsUserInstall": true,
      "installType": install_type,
      "fallbackToAgentsDir": fallback_to_agents_dir,
      "globalPath": global_path,
      "dryRunSupported": true,
      "applySupported": apply_supported
    })
}

fn agent_target_display_name(target: &str) -> &'static str {
    match target {
        "claude-code" => "Claude Code",
        "codex" => "Codex",
        "cursor" => "Cursor",
        "copilot" => "Copilot",
        "opencode" => "OpenCode",
        _ => "Generic .agents",
    }
}

fn build_agent_skill_install_plan(
    target: &str,
    scope: &str,
    fallback_to_agents_dir: bool,
    project_root: &Path,
    user_root: &Path,
) -> Result<AgentSkillInstallPlan> {
    let install_type = "skill-directory";
    let base = agent_skill_base_dir(target, scope, fallback_to_agents_dir)?;
    let writes = canonical_skill_file_paths(&format!("{base}/skills/wsl-bridge-operator"));
    let root_path = resolve_agent_install_destination(&base, scope, install_type, project_root, user_root)
        .display()
        .to_string();
    let resolved_paths = writes
        .iter()
        .map(|write| {
            resolve_agent_install_destination(
                &write.path,
                scope,
                install_type,
                project_root,
                user_root,
            )
            .display()
            .to_string()
        })
        .collect::<Vec<_>>();

    Ok(AgentSkillInstallPlan {
        ok: true,
        mode: "dryRun".to_owned(),
        operation: "install".to_owned(),
        skill: skill_manifest_summary(),
        target_agent: target.to_owned(),
        scope: scope.to_owned(),
        install_type: install_type.to_owned(),
        detected_state: None,
        root_path: Some(root_path),
        resolved_paths,
        writes,
        deletes: Vec::new(),
        warnings: agent_install_warnings(target, scope, install_type),
    })
}

fn build_agent_skill_uninstall_plan(
    target: &str,
    scope: &str,
    fallback_to_agents_dir: bool,
    project_root: &Path,
    user_root: &Path,
) -> Result<AgentSkillInstallPlan> {
    if scope != "user" {
        return Err(anyhow!("uninstall_agent_skill only supports user scope"));
    }
    let install_plan = build_agent_skill_install_plan(
        target,
        scope,
        fallback_to_agents_dir,
        project_root,
        user_root,
    )?;
    let detection = detect_agent_skill_installation(&install_plan, project_root, user_root)?;
    let mut warnings = agent_uninstall_warnings(
        target,
        scope,
        &install_plan.install_type,
        &detection,
    );
    let deletes = detection
        .files
        .iter()
        .filter(|item| item.managed)
        .map(|item| AgentSkillInstallWrite {
            path: item.write.path.clone(),
            action: "delete".to_owned(),
            source: "managed-install".to_owned(),
        })
        .collect::<Vec<_>>();
    if detection.state == "not_installed" {
        warnings.push(AgentSkillInstallWarning {
            severity: "info".to_owned(),
            code: "NOT_INSTALLED".to_owned(),
            message: "No managed Skill files were detected for the selected target.".to_owned(),
        });
    }

    Ok(AgentSkillInstallPlan {
        ok: true,
        mode: "dryRun".to_owned(),
        operation: "uninstall".to_owned(),
        skill: skill_manifest_summary(),
        target_agent: target.to_owned(),
        scope: scope.to_owned(),
        install_type: install_plan.install_type,
        detected_state: Some(detection.state),
        root_path: install_plan.root_path,
        resolved_paths: detection
            .files
            .iter()
            .filter(|item| item.managed)
            .map(|item| item.destination.display().to_string())
            .collect(),
        writes: install_plan.writes,
        deletes,
        warnings,
    })
}

fn canonical_skill_file_paths(base: &str) -> Vec<AgentSkillInstallWrite> {
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
    .map(|path| AgentSkillInstallWrite {
        path: format!("{base}/{path}"),
        action: "create-or-update".to_owned(),
        source: format!("skills/wsl-bridge-operator/{path}"),
    })
    .collect()
}

fn agent_mcp_client_path(target: &str, user_root: &Path) -> Option<PathBuf> {
    match target {
        "opencode" => Some(
            resolve_agent_install_destination(
                "~/.config/opencode/opencode.json",
                "user",
                "mcp-client",
                Path::new("."),
                user_root,
            ),
        ),
        _ => None,
    }
}

fn detect_agent_mcp_client_state(
    target: &str,
    user_root: &Path,
    mcp_config: &McpServerConfig,
) -> Result<AgentMcpClientState> {
    let Some(path) = agent_mcp_client_path(target, user_root) else {
        return Ok(AgentMcpClientState {
            target_agent: target.to_owned(),
            install_supported: false,
            detected_state: "unsupported".to_owned(),
            path: None,
        });
    };

    let detected_state = if !path.exists() {
        "not_installed".to_owned()
    } else {
        match detect_opencode_mcp_client_state(&path, mcp_config) {
            Ok(state) => state,
            Err(_) => "unknown".to_owned(),
        }
    };

    Ok(AgentMcpClientState {
        target_agent: target.to_owned(),
        install_supported: true,
        detected_state,
        path: Some(path.display().to_string()),
    })
}

fn detect_opencode_mcp_client_state(path: &Path, mcp_config: &McpServerConfig) -> Result<String> {
    let text = fs::read_to_string(path)?;
    let value = serde_json::from_str::<Value>(&text)?;
    let entry = value
        .get("mcp")
        .and_then(Value::as_object)
        .and_then(|mcp| mcp.get(&mcp_config.server_name));
    let sidecar_path = opencode_client_config_sidecar_path(path);
    let managed_names = read_managed_opencode_entry_names(
        &sidecar_path,
        "wsl-bridge-operator",
        "skill-directory",
    )?;

    match entry {
        Some(entry_value) => {
            let managed = managed_names.contains(&mcp_config.server_name)
                || is_legacy_managed_opencode_entry_value(
                    entry_value,
                    "wsl-bridge-operator",
                    "skill-directory",
                );
            if managed {
                Ok("installed".to_owned())
            } else {
                Ok("conflict".to_owned())
            }
        }
        None => {
            if managed_names.is_empty() {
                Ok("not_installed".to_owned())
            } else {
                Ok("conflict".to_owned())
            }
        }
    }
}

fn agent_install_warnings(
    target: &str,
    scope: &str,
    _install_type: &str,
) -> Vec<AgentSkillInstallWarning> {
    let mut warnings = Vec::new();
    warnings.push(AgentSkillInstallWarning {
        severity: "info".to_owned(),
        code: "SENSITIVE_INSTALL".to_owned(),
        message:
            "Installing Agent Skill writes project or user files and should only run with explicit confirmation."
                .to_owned(),
    });
    if scope == "user" {
        warnings.push(AgentSkillInstallWarning {
            severity: "warning".to_owned(),
            code: "USER_SCOPE_AFFECTS_ALL_PROJECTS".to_owned(),
            message:
                "User-scope installation can affect multiple projects used by the target Agent."
                    .to_owned(),
        });
    }
    if is_generic_agent_target(target) {
        warnings.push(AgentSkillInstallWarning {
            severity: "info".to_owned(),
            code: "GENERIC_SKILL_FALLBACK".to_owned(),
            message:
                "This target uses the generic .agents fallback path."
                    .to_owned(),
        });
    }
    warnings
}

fn agent_uninstall_warnings(
    target: &str,
    scope: &str,
    _install_type: &str,
    detection: &AgentSkillDetection,
) -> Vec<AgentSkillInstallWarning> {
    let mut warnings = Vec::new();
    warnings.push(AgentSkillInstallWarning {
        severity: "info".to_owned(),
        code: "SENSITIVE_UNINSTALL".to_owned(),
        message:
            "Uninstall only removes files that are still marked as managed by wsl-bridge."
                .to_owned(),
    });
    if scope == "user" {
        warnings.push(AgentSkillInstallWarning {
            severity: "warning".to_owned(),
            code: "USER_SCOPE_AFFECTS_ALL_PROJECTS".to_owned(),
            message:
                "User-scope uninstall can affect multiple projects used by the target Agent."
                    .to_owned(),
        });
    }
    if detection.state == "conflict" {
        let unmanaged = detection
            .files
            .iter()
            .filter(|item| item.exists && !item.managed)
            .map(|item| item.write.path.clone())
            .collect::<Vec<_>>();
        if !unmanaged.is_empty() {
            warnings.push(AgentSkillInstallWarning {
                severity: "warning".to_owned(),
                code: "UNMANAGED_FILES_SKIPPED".to_owned(),
                message: format!(
                    "Some files exist at the target path but are not managed by wsl-bridge and will be kept: {}",
                    unmanaged.join(", ")
                ),
            });
        }
    }
    if is_generic_agent_target(target) {
        warnings.push(AgentSkillInstallWarning {
            severity: "info".to_owned(),
            code: "GENERIC_SKILL_FALLBACK".to_owned(),
            message:
                "This target uses the generic .agents fallback path.".to_owned(),
        });
    }
    warnings
}

fn install_agent_skill_plan(
    plan: &AgentSkillInstallPlan,
    project_root: &Path,
    user_root: &Path,
    mcp_config: &McpServerConfig,
) -> Result<Vec<String>> {
    let canonical_root = resolve_canonical_skill_root(project_root)?;
    let mut applied_paths = Vec::new();
    for write in &plan.writes {
        let destination = resolve_agent_install_destination(
            &write.path,
            &plan.scope,
            &plan.install_type,
            project_root,
            user_root,
        );
        write_agent_install_file(
            &canonical_root,
            &destination,
            write,
            &plan.install_type,
            plan.skill
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("wsl-bridge-operator"),
            plan.skill
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("0.1.0"),
            mcp_config,
        )?;
        applied_paths.push(destination.display().to_string());
    }
    Ok(applied_paths)
}

fn uninstall_agent_skill_plan(
    plan: &AgentSkillInstallPlan,
    project_root: &Path,
    user_root: &Path,
) -> Result<Vec<String>> {
    let resolved = resolve_agent_skill_plan_files(plan, project_root, user_root)?;
    let stop_root = resolve_agent_install_root(&plan.scope, &plan.install_type, project_root, user_root);
    let mut deleted_paths = Vec::new();

    for item in resolved.into_iter().filter(|item| item.exists && item.managed) {
        if item.write.source == "rendered-opencode-client-config" {
            if remove_managed_opencode_client_config(
                &item.destination,
                plan.skill
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("wsl-bridge-operator"),
                &plan.install_type,
            )? {
                deleted_paths.push(item.destination.display().to_string());
                prune_empty_agent_install_dirs(item.destination.parent(), &stop_root)?;
            }
            continue;
        }
        if item.write.source == "rendered-opencode-client-config-meta" {
            if item.destination.exists() {
                fs::remove_file(&item.destination)?;
                deleted_paths.push(item.destination.display().to_string());
                prune_empty_agent_install_dirs(item.destination.parent(), &stop_root)?;
            }
            continue;
        }
        if item.destination.exists()
            && !is_managed_agent_install_file(
                &item.destination,
                &item.write.source,
                plan.skill.get("id").and_then(Value::as_str).unwrap_or("wsl-bridge-operator"),
                &plan.install_type,
            )?
        {
            continue;
        }
        fs::remove_file(&item.destination)?;
        deleted_paths.push(item.destination.display().to_string());
        prune_empty_agent_install_dirs(item.destination.parent(), &stop_root)?;
    }

    Ok(deleted_paths)
}

fn resolve_canonical_skill_root(project_root: &Path) -> Result<PathBuf> {
    let candidates = [
        project_root.join("skills").join("wsl-bridge-operator"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("skills")
            .join("wsl-bridge-operator"),
    ];
    candidates
        .into_iter()
        .find(|path| path.join("SKILL.md").exists() && path.join("manifest.json").exists())
        .ok_or_else(|| anyhow!("canonical skill package not found"))
}

fn resolve_user_home_dir() -> Result<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("resolve user home directory failed"))
}

fn resolve_agent_project_root(project_root: Option<&str>) -> Result<PathBuf> {
    if let Some(project_root) = project_root.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(project_root));
    }
    std::env::current_dir().map_err(|err| anyhow!("resolve current_dir failed: {err}"))
}

fn is_generic_agent_target(target: &str) -> bool {
    target == "generic"
}

fn agent_skill_base_dir(target: &str, scope: &str, fallback_to_agents_dir: bool) -> Result<String> {
    if scope == "project" {
        let path = match target {
            "claude-code" => ".claude",
            "codex" => ".codex",
            "cursor" => ".cursor",
            "copilot" => ".copilot",
            "opencode" => ".opencode",
            "generic" if fallback_to_agents_dir => ".agents",
            _ => ".agents",
        };
        return Ok(path.to_owned());
    }

    let path = match target {
        "claude-code" => "~/.claude",
        "codex" => "~/.codex",
        "cursor" => "~/.cursor",
        "copilot" => "~/.copilot",
        "opencode" => "~/.config/opencode",
        "generic" if fallback_to_agents_dir => "~/.agents",
        _ => "~/.agents",
    };
    Ok(path.to_owned())
}

fn resolve_agent_install_destination(
    relative_path: &str,
    scope: &str,
    _install_type: &str,
    project_root: &Path,
    user_root: &Path,
) -> PathBuf {
    let trimmed = relative_path.replace('/', "\\");
    if scope == "user" {
        let without_home = trimmed
            .trim_start_matches("~\\")
            .trim_start_matches("~")
            .trim_start_matches(".\\");
        user_root.join(without_home)
    } else {
        project_root.join(trimmed.trim_start_matches(".\\"))
    }
}

fn resolve_agent_install_root(
    scope: &str,
    _install_type: &str,
    project_root: &Path,
    user_root: &Path,
) -> PathBuf {
    if scope == "user" {
        user_root.to_path_buf()
    } else {
        project_root.to_path_buf()
    }
}

fn resolve_agent_skill_plan_files(
    plan: &AgentSkillInstallPlan,
    project_root: &Path,
    user_root: &Path,
) -> Result<Vec<AgentSkillResolvedWrite>> {
    let skill_id = plan
        .skill
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("wsl-bridge-operator");
    plan.writes
        .iter()
        .cloned()
        .map(|write| {
            let destination = resolve_agent_install_destination(
                &write.path,
                &plan.scope,
                &plan.install_type,
                project_root,
                user_root,
            );
            let exists = destination.exists();
            let managed = if exists {
                is_managed_agent_install_file(
                    &destination,
                    &write.source,
                    skill_id,
                    &plan.install_type,
                )?
            } else {
                false
            };
            Ok(AgentSkillResolvedWrite {
                write,
                destination,
                exists,
                managed,
            })
        })
        .collect()
}

fn is_primary_agent_skill_write(source: &str) -> bool {
    source.starts_with("skills/wsl-bridge-operator/")
}

fn detect_agent_skill_installation(
    plan: &AgentSkillInstallPlan,
    project_root: &Path,
    user_root: &Path,
) -> Result<AgentSkillDetection> {
    let files = resolve_agent_skill_plan_files(plan, project_root, user_root)?;
    let primary_total = files
        .iter()
        .filter(|item| is_primary_agent_skill_write(&item.write.source))
        .count();
    let primary_managed = files
        .iter()
        .filter(|item| is_primary_agent_skill_write(&item.write.source) && item.managed)
        .count();
    let primary_existing = files
        .iter()
        .filter(|item| is_primary_agent_skill_write(&item.write.source) && item.exists)
        .count();
    let auxiliary_total = files.len().saturating_sub(primary_total);
    let auxiliary_managed = files
        .iter()
        .filter(|item| !is_primary_agent_skill_write(&item.write.source) && item.managed)
        .count();
    let auxiliary_existing = files
        .iter()
        .filter(|item| !is_primary_agent_skill_write(&item.write.source) && item.exists)
        .count();
    let state = if primary_total == 0 {
        if auxiliary_managed == files.len() && !files.is_empty() {
            "installed"
        } else if files.iter().all(|item| !item.exists) {
            "not_installed"
        } else {
            "conflict"
        }
    } else if primary_managed == primary_total {
        if auxiliary_total == 0 || auxiliary_existing == 0 || auxiliary_managed == auxiliary_total {
            "installed"
        } else {
            "conflict"
        }
    } else if primary_existing == 0 && auxiliary_managed == 0 {
        "not_installed"
    } else {
        "conflict"
    };
    Ok(AgentSkillDetection {
        state: state.to_owned(),
        files,
    })
}

#[cfg(test)]
fn detect_agent_skill_install_state(
    target: &str,
    scope: &str,
    fallback_to_agents_dir: bool,
    project_root: &Path,
    user_root: &Path,
) -> Result<String> {
    let plan = build_agent_skill_install_plan(
        target,
        scope,
        fallback_to_agents_dir,
        project_root,
        user_root,
    )?;
    Ok(detect_agent_skill_installation(&plan, project_root, user_root)?.state)
}

fn is_managed_agent_install_file(
    path: &Path,
    source: &str,
    skill_id: &str,
    install_type: &str,
) -> Result<bool> {
    if source == "rendered-opencode-client-config" {
        return has_managed_opencode_client_config(path, skill_id, install_type);
    }
    let text = fs::read_to_string(path)?;
    if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
        let value = serde_json::from_str::<Value>(&text)?;
        let Some(object) = value.as_object() else {
            return Ok(false);
        };
        return Ok(
            object.get("_managedBy").and_then(Value::as_str) == Some("wsl-bridge")
                && object.get("_skillId").and_then(Value::as_str) == Some(skill_id)
                && object.get("_installType").and_then(Value::as_str) == Some(install_type),
        );
    }

    Ok(
        text.contains("<!-- managed-by: wsl-bridge -->")
            && text.contains(&format!("<!-- skill-id: {skill_id} -->"))
            && text.contains(&format!("<!-- install-type: {install_type} -->")),
    )
}

fn prune_empty_agent_install_dirs(start: Option<&Path>, stop_root: &Path) -> Result<()> {
    let mut current = start.map(Path::to_path_buf);
    while let Some(path) = current {
        if path == stop_root || !path.starts_with(stop_root) {
            break;
        }
        if fs::read_dir(&path)?.next().is_some() {
            break;
        }
        fs::remove_dir(&path)?;
        current = path.parent().map(Path::to_path_buf);
    }
    Ok(())
}

fn write_agent_install_file(
    canonical_root: &Path,
    destination: &Path,
    write: &AgentSkillInstallWrite,
    install_type: &str,
    skill_id: &str,
    skill_version: &str,
    mcp_config: &McpServerConfig,
) -> Result<()> {
    let content = match write.source.as_str() {
        "rendered-cursor-rule" => render_cursor_rule_content(skill_id, skill_version, install_type),
        "rendered-copilot-instructions" => {
            render_copilot_instructions_content(skill_id, skill_version, install_type)
        }
        "rendered-opencode-client-config" => render_opencode_client_config(
            destination,
            mcp_config,
            skill_id,
            skill_version,
            install_type,
        )?,
        "rendered-opencode-client-config-meta" => render_opencode_client_config_metadata(
            mcp_config,
            skill_id,
            skill_version,
            install_type,
        )?,
        source if source.starts_with("skills/wsl-bridge-operator/") => {
            let relative = source.trim_start_matches("skills/wsl-bridge-operator/");
            render_skill_package_file(canonical_root, relative, skill_id, skill_version, install_type)?
        }
        other => {
            return Err(anyhow!("unsupported agent skill install source: {other}"));
        }
    };

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(destination, content)?;
    Ok(())
}

fn render_skill_package_file(
    canonical_root: &Path,
    relative_path: &str,
    skill_id: &str,
    skill_version: &str,
    install_type: &str,
) -> Result<String> {
    let source_path = canonical_root.join(relative_path.replace('/', "\\"));
    let text = fs::read_to_string(&source_path)
        .map_err(|err| anyhow!("read canonical skill file failed for {}: {err}", source_path.display()))?;
    if relative_path.ends_with(".json") {
        let mut value = serde_json::from_str::<Value>(&text)
            .map_err(|err| anyhow!("parse canonical manifest failed: {err}"))?;
        if let Some(object) = value.as_object_mut() {
            object.insert("_managedBy".to_owned(), json!("wsl-bridge"));
            object.insert("_skillId".to_owned(), json!(skill_id));
            object.insert("_skillVersion".to_owned(), json!(skill_version));
            object.insert("_installType".to_owned(), json!(install_type));
        }
        Ok(format!("{}\n", serde_json::to_string_pretty(&value)?))
    } else {
        Ok(inject_managed_marker_into_skill_text(
            &text,
            &managed_marker_block(skill_id, skill_version, install_type),
        ))
    }
}

fn render_opencode_client_config(
    destination: &Path,
    mcp_config: &McpServerConfig,
    skill_id: &str,
    skill_version: &str,
    install_type: &str,
) -> Result<String> {
    let mut root = if destination.exists() {
        let text = fs::read_to_string(destination)?;
        serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };
    let root_object = ensure_json_object(&mut root)?;
    let mcp_value = root_object
        .entry("mcp".to_owned())
        .or_insert_with(|| json!({}));
    let mcp_object = ensure_json_object(mcp_value)?;

    let mut managed_names = read_managed_opencode_entry_names(
        &opencode_client_config_sidecar_path(destination),
        skill_id,
        install_type,
    )?;
    managed_names.extend(legacy_managed_opencode_entry_names(mcp_object, skill_id, install_type));
    managed_names.insert(mcp_config.server_name.clone());
    mcp_object.retain(|name, _| !managed_names.contains(name));
    mcp_object.insert(
        mcp_config.server_name.clone(),
        build_managed_opencode_entry(mcp_config, skill_id, skill_version, install_type),
    );

    Ok(format!("{}\n", serde_json::to_string_pretty(&root)?))
}

fn build_managed_opencode_entry(
    mcp_config: &McpServerConfig,
    _skill_id: &str,
    _skill_version: &str,
    _install_type: &str,
) -> Value {
    json!({
      "type": "remote",
      "url": format!("http://127.0.0.1:{}{}", mcp_config.listen_port, MCP_PATH),
      "enabled": true
    })
}

fn render_opencode_client_config_metadata(
    mcp_config: &McpServerConfig,
    skill_id: &str,
    skill_version: &str,
    install_type: &str,
) -> Result<String> {
    Ok(format!(
        "{}\n",
        serde_json::to_string_pretty(&json!({
          "_managedBy": "wsl-bridge",
          "_skillId": skill_id,
          "_skillVersion": skill_version,
          "_installType": install_type,
          "entries": [mcp_config.server_name]
        }))?
    ))
}

fn ensure_json_object(value: &mut Value) -> Result<&mut serde_json::Map<String, Value>> {
    if !value.is_object() {
        *value = json!({});
    }
    value
        .as_object_mut()
        .ok_or_else(|| anyhow!("expected JSON object"))
}

fn opencode_client_config_sidecar_path(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".wsl-bridge-opencode-managed.json")
}

fn read_managed_opencode_entry_names(
    metadata_path: &Path,
    skill_id: &str,
    install_type: &str,
) -> Result<HashSet<String>> {
    if !metadata_path.exists() {
        return Ok(HashSet::new());
    }
    let text = fs::read_to_string(metadata_path)?;
    let value = serde_json::from_str::<Value>(&text)?;
    let Some(object) = value.as_object() else {
        return Ok(HashSet::new());
    };
    if object.get("_managedBy").and_then(Value::as_str) != Some("wsl-bridge")
        || object.get("_skillId").and_then(Value::as_str) != Some(skill_id)
        || object.get("_installType").and_then(Value::as_str) != Some(install_type)
    {
        return Ok(HashSet::new());
    }
    Ok(object
        .get("entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect())
}

fn legacy_managed_opencode_entry_names(
    mcp_object: &serde_json::Map<String, Value>,
    skill_id: &str,
    install_type: &str,
) -> HashSet<String> {
    mcp_object
        .iter()
        .filter_map(|(name, value)| {
            if is_legacy_managed_opencode_entry_value(value, skill_id, install_type) {
                Some(name.to_owned())
            } else {
                None
            }
        })
        .collect()
}

fn has_managed_opencode_client_config(
    path: &Path,
    skill_id: &str,
    install_type: &str,
) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let text = fs::read_to_string(path)?;
    let value = serde_json::from_str::<Value>(&text)?;
    let metadata_names =
        read_managed_opencode_entry_names(&opencode_client_config_sidecar_path(path), skill_id, install_type)?;
    Ok(value
        .get("mcp")
        .and_then(Value::as_object)
        .map(|entries| {
            entries.iter().any(|(name, entry)| {
                metadata_names.contains(name)
                    || is_legacy_managed_opencode_entry_value(entry, skill_id, install_type)
            })
        })
        .unwrap_or(false))
}

fn remove_managed_opencode_client_config(
    path: &Path,
    skill_id: &str,
    install_type: &str,
) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let text = fs::read_to_string(path)?;
    let mut value = serde_json::from_str::<Value>(&text)?;
    let mut removed = false;
    let metadata_path = opencode_client_config_sidecar_path(path);
    let metadata_names = read_managed_opencode_entry_names(&metadata_path, skill_id, install_type)?;

    if let Some(root) = value.as_object_mut() {
        if let Some(mcp) = root.get_mut("mcp").and_then(Value::as_object_mut) {
            let original_len = mcp.len();
            mcp.retain(|name, entry| {
                !(metadata_names.contains(name)
                    || is_legacy_managed_opencode_entry_value(entry, skill_id, install_type))
            });
            removed = mcp.len() != original_len;
            if mcp.is_empty() {
                root.remove("mcp");
            }
        }

        if removed {
            if root.is_empty() {
                fs::remove_file(path)?;
            } else {
                fs::write(path, format!("{}\n", serde_json::to_string_pretty(&value)?))?;
            }
        }
    }

    if metadata_path.exists() {
        fs::remove_file(metadata_path)?;
    }

    Ok(removed)
}

fn is_legacy_managed_opencode_entry_value(entry: &Value, skill_id: &str, install_type: &str) -> bool {
    entry
        .as_object()
        .map(|object| {
            object.get("_managedBy").and_then(Value::as_str) == Some("wsl-bridge")
                && object.get("_skillId").and_then(Value::as_str) == Some(skill_id)
                && object.get("_installType").and_then(Value::as_str) == Some(install_type)
        })
        .unwrap_or(false)
}

fn managed_marker_block(skill_id: &str, skill_version: &str, install_type: &str) -> String {
    [
        "<!-- managed-by: wsl-bridge -->".to_owned(),
        format!("<!-- skill-id: {skill_id} -->"),
        format!("<!-- skill-version: {skill_version} -->"),
        format!("<!-- install-type: {install_type} -->"),
        String::new(),
    ]
    .join("\n")
}

fn inject_managed_marker_into_skill_text(text: &str, marker_block: &str) -> String {
    for (prefix, delimiter) in [("---\r\n", "\r\n---\r\n"), ("---\n", "\n---\n")] {
        if text.starts_with(prefix) {
            let search_start = prefix.len();
            if let Some(relative_idx) = text[search_start..].find(delimiter) {
                let insert_at = search_start + relative_idx + delimiter.len();
                let body = text[insert_at..].trim_start_matches(['\r', '\n']);
                return format!("{}{}{}", &text[..insert_at], marker_block, body);
            }
        }
    }

    format!("{marker_block}\n{text}")
}

fn render_cursor_rule_content(skill_id: &str, skill_version: &str, install_type: &str) -> String {
    format!(
        "---\ndescription: wsl-bridge operator workflow\nglobs:\nalwaysApply: false\n---\n{}\nUse this rule when working on wsl-bridge configuration, diagnostics, or MCP-driven changes.\n\n1. Read `skills/wsl-bridge-operator/SKILL.md` when available.\n2. Read `wsl-bridge://ai-guide`, `wsl-bridge://capabilities`, and `wsl-bridge://state/summary` before editing.\n3. Convert complex writes into `ConfigPatch`, dry-run first, then apply after confirmation.\n4. Validate with `validate_config` or `test_connectivity` after changes.\n",
        managed_marker_block(skill_id, skill_version, install_type)
    )
}

fn render_copilot_instructions_content(
    skill_id: &str,
    skill_version: &str,
    install_type: &str,
) -> String {
    format!(
        "{}# wsl-bridge Operator\n\nWhen working on wsl-bridge, follow this workflow:\n\n1. Read `skills/wsl-bridge-operator/SKILL.md` and the referenced documents when available.\n2. Inspect current app state through MCP resources before making assumptions.\n3. Represent non-trivial writes as `ConfigPatch` and dry-run before apply.\n4. Treat system hosts writes, listener `0.0.0.0` exposure, certificate changes, and Agent Skill installation as sensitive operations.\n5. Run validation or connectivity checks after configuration changes.\n",
        managed_marker_block(skill_id, skill_version, install_type)
    )
}

fn ai_guide_resource() -> &'static str {
    r#"# wsl-bridge AI Guide

wsl-bridge is a Windows desktop app for managing WSL / Hyper-V bridge rules, reverse proxy configuration, structured Hosts groups, runtime status, and traffic monitoring.

Recommended AI workflow:

1. Read `wsl-bridge://capabilities` and `wsl-bridge://state/summary`.
2. Read `wsl-bridge://state/proxy`, `wsl-bridge://state/hosts`, or `wsl-bridge://state/rules` according to the task scope.
3. Read `wsl-bridge://state/traffic` and `wsl-bridge://logs/recent` when diagnosis or runtime behavior matters.
4. Read `wsl-bridge://schemas/state` when stable parsing contracts are required.
5. Read `wsl-bridge://schemas/config-patch` before writing a ConfigPatch.
6. For existing Proxy / Hosts objects, prefer stable `id` values from state resources.
7. Use `*Ref` fields only for objects created earlier in the same patch.
8. Dry-run patches before apply.
9. Explain warnings to the user.
10. Validate configuration and connectivity after changes.

Current Phase3 AI API status:

- Resources are available for discovery and read-only context.
- Existing legacy MCP tools remain available according to the user's exposed capability toggles.
- `ConfigPatch` supports dry-run plus transactional apply for Proxy and Hosts in this build.
- Existing Proxy objects can be updated or deleted through `ConfigPatch` when targeted by stable ids from `wsl-bridge://state/proxy`.
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
        "wsl-bridge://state/rules",
        "wsl-bridge://state/traffic",
        "wsl-bridge://logs/recent",
        "wsl-bridge://schemas/config-patch",
        "wsl-bridge://schemas/state"
      ],
        "tools": {
        "legacyTools": build_tool_definitions(config),
        "configPatch": {
          "dryRun": true,
          "apply": true,
          "status": "proxy-hosts-transactional"
        },
        "configExchange": {
          "export": true,
          "importDryRun": true,
          "importApply": true
        },
        "agentSkill": {
          "listTargets": true,
          "installDryRun": true,
          "installApply": true,
          "uninstallDryRun": true,
          "uninstallApply": true,
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
          "agent-skill-uninstall",
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
        "allowedCreateTypes": ["udp_fwd", "socks5_proxy"],
        "resource": "wsl-bridge://state/rules"
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
        "dryRun": "available",
        "apply": "proxy-hosts-available"
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

fn state_rules_resource(engine: &Arc<RuleEngine>, detail: &str) -> Value {
    let rules = engine.list_rules();
    let migrations = engine.list_rule_migrations();
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
    let socks5_proxy = rules
        .iter()
        .filter(|rule| matches!(rule.rule_type, RuleType::Socks5Proxy))
        .count();
    let summary = json!({
      "legacyMode": true,
      "total": total,
      "enabled": enabled,
      "disabled": total.saturating_sub(enabled),
      "byType": {
        "tcp_fwd": tcp_fwd,
        "udp_fwd": udp_fwd,
        "http_proxy": http_proxy,
        "socks5_proxy": socks5_proxy
      },
      "allowedCreateTypes": ["udp_fwd", "socks5_proxy"],
      "blockedCreateTypes": ["tcp_fwd", "http_proxy"],
      "migratableTypes": ["tcp_fwd", "http_proxy"],
      "migrationRecords": migrations.len()
    });

    if detail != "full" && detail != "diagnostic" {
        return summary;
    }

    json!({
      "summary": summary,
      "items": rules,
      "migrations": migrations,
      "notes": [
        "Rules is in legacy mode.",
        "New tcp_fwd and http_proxy rules should be migrated to Proxy instead of being created in Rules."
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
    serde_json::from_str(&format!(
        r#"{{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "wsl-bridge ConfigPatch",
  "type": "object",
  "required": ["version"],
  "description": "Structured change set for Proxy and Hosts. Dry-run before apply. Existing objects should be targeted by stable ids from state resources; *Ref fields are for linking objects created earlier in the same patch.",
  "properties": {{
    "version": {{
      "const": "{version}"
    }},
    "reason": {{
      "type": "string"
    }},
    "proxy": {{
      "type": "object",
      "additionalProperties": false,
      "properties": {{
        "listeners": {{
          "type": "object",
          "additionalProperties": false,
          "properties": {{
            "create": {{
              "type": "array",
              "items": {{
                "type": "object",
                "required": ["name", "bindAddress", "port", "protocol"],
                "additionalProperties": false,
                "properties": {{
                  "clientId": {{ "type": "string" }},
                  "name": {{ "type": "string" }},
                  "bindAddress": {{ "type": "string" }},
                  "port": {{ "type": "integer", "minimum": 1, "maximum": 65535 }},
                  "protocol": {{ "type": "string", "enum": ["http", "https"] }},
                  "tlsMode": {{ "type": "string", "enum": ["disabled", "manual_cert", "local_ca"] }},
                  "certId": {{ "type": "string" }},
                  "bindMode": {{ "type": "string", "enum": ["all_nics", "single_nic"] }},
                  "nicId": {{ "type": "string" }},
                  "enabled": {{ "type": "boolean" }}
                }}
              }}
            }},
            "update": {{
              "type": "array",
              "items": {{
                "type": "object",
                "additionalProperties": false,
                "properties": {{
                  "id": {{ "type": "string", "description": "Stable listener id from wsl-bridge://state/proxy." }},
                  "listenerRef": {{ "type": "string", "description": "Reference to a listener created earlier in the same patch." }},
                  "name": {{ "type": "string" }},
                  "bindAddress": {{ "type": "string" }},
                  "port": {{ "type": "integer", "minimum": 1, "maximum": 65535 }},
                  "protocol": {{ "type": "string", "enum": ["http", "https"] }},
                  "tlsMode": {{ "type": "string", "enum": ["disabled", "manual_cert", "local_ca"] }},
                  "certId": {{ "type": "string" }},
                  "bindMode": {{ "type": "string", "enum": ["all_nics", "single_nic"] }},
                  "nicId": {{ "type": "string" }},
                  "enabled": {{ "type": "boolean" }}
                }},
                "anyOf": [
                  {{ "required": ["id"] }},
                  {{ "required": ["listenerRef"] }}
                ]
              }}
            }},
            "delete": {{
              "type": "array",
              "items": {{
                "type": "object",
                "additionalProperties": false,
                "properties": {{
                  "id": {{ "type": "string", "description": "Stable listener id from wsl-bridge://state/proxy." }},
                  "listenerRef": {{ "type": "string", "description": "Reference to a listener created earlier in the same patch." }}
                }},
                "anyOf": [
                  {{ "required": ["id"] }},
                  {{ "required": ["listenerRef"] }}
                ]
              }}
            }}
          }}
        }},
        "routes": {{
          "type": "object",
          "additionalProperties": false,
          "properties": {{
            "create": {{
              "type": "array",
              "items": {{
                "type": "object",
                "required": ["listenerRef"],
                "additionalProperties": false,
                "properties": {{
                  "clientId": {{ "type": "string" }},
                  "listenerRef": {{ "type": "string", "description": "Listener id from state/proxy, or a listenerRef created earlier in the same patch." }},
                  "serverNames": {{ "type": "array", "items": {{ "type": "string" }} }},
                  "pathPrefix": {{ "type": "string" }},
                  "isDefault": {{ "type": "boolean" }},
                  "enabled": {{ "type": "boolean" }},
                  "upstreamRef": {{ "type": "string", "description": "Optional upstream reference created in the same patch." }}
                }}
              }}
            }},
            "update": {{
              "type": "array",
              "items": {{
                "type": "object",
                "additionalProperties": false,
                "properties": {{
                  "id": {{ "type": "string", "description": "Stable route id from wsl-bridge://state/proxy." }},
                  "routeRef": {{ "type": "string", "description": "Reference to a route created earlier in the same patch." }},
                  "serverNames": {{ "type": "array", "items": {{ "type": "string" }} }},
                  "pathPrefix": {{
                    "oneOf": [
                      {{ "type": "string" }},
                      {{ "type": "null" }}
                    ]
                  }},
                  "isDefault": {{ "type": "boolean" }},
                  "enabled": {{ "type": "boolean" }}
                }},
                "anyOf": [
                  {{ "required": ["id"] }},
                  {{ "required": ["routeRef"] }}
                ]
              }}
            }},
            "delete": {{
              "type": "array",
              "items": {{
                "type": "object",
                "additionalProperties": false,
                "properties": {{
                  "id": {{ "type": "string", "description": "Stable route id from wsl-bridge://state/proxy." }},
                  "routeRef": {{ "type": "string", "description": "Reference to a route created earlier in the same patch." }}
                }},
                "anyOf": [
                  {{ "required": ["id"] }},
                  {{ "required": ["routeRef"] }}
                ]
              }}
            }}
          }}
        }},
        "upstreams": {{
          "type": "object",
          "additionalProperties": false,
          "properties": {{
            "create": {{
              "type": "array",
              "items": {{
                "type": "object",
                "required": ["targetType", "targetPort", "protocol"],
                "additionalProperties": false,
                "properties": {{
                  "clientId": {{ "type": "string" }},
                  "routeRef": {{ "type": "string", "description": "Optional route id from state/proxy, or a routeRef created earlier in the same patch." }},
                  "targetType": {{ "type": "string", "enum": ["static", "wsl", "hyperv"] }},
                  "targetRef": {{ "type": "string" }},
                  "targetHost": {{ "type": "string" }},
                  "targetPort": {{ "type": "integer", "minimum": 1, "maximum": 65535 }},
                  "protocol": {{ "type": "string", "enum": ["http", "https", "ws", "wss", "grpc", "grpcs"] }},
                  "pathRewriteFrom": {{
                    "oneOf": [
                      {{ "type": "string" }},
                      {{ "type": "null" }}
                    ]
                  }},
                  "pathRewriteTo": {{
                    "oneOf": [
                      {{ "type": "string" }},
                      {{ "type": "null" }}
                    ]
                  }},
                  "enabled": {{ "type": "boolean" }}
                }}
              }}
            }},
            "update": {{
              "type": "array",
              "items": {{
                "type": "object",
                "additionalProperties": false,
                "properties": {{
                  "id": {{ "type": "string", "description": "Stable upstream id from wsl-bridge://state/proxy." }},
                  "upstreamRef": {{ "type": "string", "description": "Reference to an upstream created earlier in the same patch." }},
                  "routeRef": {{ "type": "string" }},
                  "targetKind": {{ "type": "string", "enum": ["static", "wsl", "hyperv"] }},
                  "targetRef": {{ "type": "string" }},
                  "targetHost": {{ "type": "string" }},
                  "targetPort": {{ "type": "integer", "minimum": 1, "maximum": 65535 }},
                  "protocol": {{ "type": "string", "enum": ["http", "https", "ws", "wss", "grpc", "grpcs"] }},
                  "pathRewriteFrom": {{
                    "oneOf": [
                      {{ "type": "string" }},
                      {{ "type": "null" }}
                    ]
                  }},
                  "pathRewriteTo": {{
                    "oneOf": [
                      {{ "type": "string" }},
                      {{ "type": "null" }}
                    ]
                  }},
                  "enabled": {{ "type": "boolean" }}
                }},
                "anyOf": [
                  {{ "required": ["id"] }},
                  {{ "required": ["upstreamRef"] }}
                ]
              }}
            }},
            "delete": {{
              "type": "array",
              "items": {{
                "type": "object",
                "additionalProperties": false,
                "properties": {{
                  "id": {{ "type": "string", "description": "Stable upstream id from wsl-bridge://state/proxy." }},
                  "upstreamRef": {{ "type": "string", "description": "Reference to an upstream created earlier in the same patch." }}
                }},
                "anyOf": [
                  {{ "required": ["id"] }},
                  {{ "required": ["upstreamRef"] }}
                ]
              }}
            }}
          }}
        }}
      }}
    }},
    "hosts": {{
      "type": "object",
      "description": "Hosts group / record changes. Existing groups and records should be targeted by stable ids from wsl-bridge://state/hosts."
    }},
    "rules": {{
      "type": "object",
      "description": "Legacy Rules migration and limited legacy-rule changes are not supported through ConfigPatch in this build."
    }},
    "settings": {{
      "type": "object",
      "description": "Application or AI integration settings are not supported through ConfigPatch in this build."
    }}
  }},
  "examples": [
    {{
      "version": "{version}",
      "reason": "Rename an existing listener by selector",
      "proxy": {{
        "listeners": {{
          "update": [
            {{
              "listenerName": "Demo Listener",
              "name": "Renamed Listener"
            }}
          ]
        }}
      }}
    }},
    {{
      "version": "{version}",
      "reason": "Delete an existing route by selector",
      "proxy": {{
        "routes": {{
          "delete": [
            {{
              "matchListenerName": "Demo Listener",
              "matchServerNames": ["demo.local"],
              "matchPathPrefix": "/"
            }}
          ]
        }}
      }}
    }}
  ],
  "additionalProperties": false
}}"#,
        version = CONFIG_PATCH_VERSION
    ))
    .expect("config patch schema json")
}

fn state_schema_resource() -> Value {
    json!({
      "$schema": "https://json-schema.org/draft/2020-12/schema",
      "title": "wsl-bridge State Resources",
      "type": "object",
      "properties": {
        "summary": {
          "resource": "wsl-bridge://state/summary",
          "shape": {
            "app": "object",
            "mcp": "object",
            "rules": "object",
            "proxy": "object",
            "hosts": "object",
            "configPatch": "object"
          }
        },
        "proxy": {
          "resource": "wsl-bridge://state/proxy",
          "shape": {
            "summary": "object",
            "certificates": "array",
            "topology": "array"
          }
        },
        "hosts": {
          "resource": "wsl-bridge://state/hosts",
          "shape": {
            "summary": "object",
            "groups": "array",
            "notes": "array"
          }
        },
        "rules": {
          "resource": "wsl-bridge://state/rules",
          "shape": {
            "summary": "object",
            "items": "array",
            "migrations": "array",
            "notes": "array"
          }
        },
        "traffic": {
          "resource": "wsl-bridge://state/traffic",
          "shape": {
            "summary": "object",
            "entities": "array",
            "series": "array"
          }
        },
        "logs": {
          "resource": "wsl-bridge://logs/recent",
          "shape": {
            "summary": "object",
            "events": "array"
          }
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
      "name": "apply_config_patch",
      "description": "Dry-run or transactionally apply a structured ConfigPatch for Proxy and Hosts changes, with rollback on failure.",
      "inputSchema": {
        "type": "object",
        "required": ["mode", "patch"],
        "properties": {
          "mode": {
            "type": "string",
            "enum": ["dryRun", "apply"]
          },
          "patch": {
            "type": "object",
            "description": "Structured ConfigPatch draft."
          },
          "idempotencyKey": {
            "type": "string"
          }
        }
      }
    }));
    tools.push(json!({
      "name": "export_config",
      "description": "Export Proxy, Hosts, and Legacy Rules configuration as structured JSON or canonical hosts-file text.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "modules": {
            "type": "array",
            "items": {
              "type": "string",
              "enum": ["proxy", "hosts", "rules"]
            }
          },
          "format": {
            "type": "string",
            "enum": ["json", "hosts-file"]
          },
          "groupRef": {
            "type": "string",
            "description": "Optional Hosts group id. Only used for format=hosts-file."
          }
        }
      }
    }));
    tools.push(json!({
      "name": "import_config",
      "description": "Import Hosts or Proxy configuration content, generate a structured ConfigPatch, and dry-run or apply it.",
      "inputSchema": {
        "type": "object",
        "required": ["module", "content", "mode"],
        "properties": {
          "module": {
            "type": "string",
            "enum": ["hosts", "proxy"]
          },
          "content": {
            "type": "string"
          },
          "mode": {
            "type": "string",
            "enum": ["dryRun", "apply"]
          }
        }
      }
    }));
    tools.push(json!({
      "name": "test_connectivity",
      "description": "Probe listener, route, upstream, host-port, or URL connectivity and report the failing stage.",
      "inputSchema": {
        "type": "object",
        "required": ["target"],
        "properties": {
          "target": {
            "type": "object",
            "required": ["type", "value"],
            "properties": {
              "type": {
                "type": "string",
                "enum": ["proxy-route", "upstream", "host-port", "url"]
              },
              "value": {
                "type": "object"
              }
            }
          }
        }
      }
    }));
    tools.push(json!({
      "name": "list_agent_targets",
      "description": "List supported Agent skill installation targets together with dry-run/apply support and fallback behavior.",
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
      "description": "Preview or apply installation of the wsl-bridge-operator skill for a target Agent or fallback path.",
      "inputSchema": {
        "type": "object",
        "required": ["target"],
        "properties": {
          "target": {
            "type": "string",
            "enum": ["claude-code", "codex", "cursor", "copilot", "opencode", "generic"]
          },
          "scope": {
            "type": "string",
            "enum": ["project", "user"]
          },
          "mode": {
            "type": "string",
            "enum": ["dryRun", "apply"]
          },
          "fallbackToAgentsDir": {
            "type": "boolean",
            "description": "Use project-level .agents/skills fallback when native installation is unavailable."
          },
          "projectRoot": {
            "type": "string",
            "description": "Optional project root path used when scope=project."
          }
        }
      }
    }));
    tools.push(json!({
      "name": "uninstall_agent_skill",
      "description": "Preview or apply uninstallation of wsl-bridge-managed skill files for a target Agent.",
      "inputSchema": {
        "type": "object",
        "required": ["target"],
        "properties": {
          "target": {
            "type": "string",
            "enum": ["claude-code", "codex", "cursor", "copilot", "opencode", "generic"]
          },
          "scope": {
            "type": "string",
            "enum": ["project", "user"]
          },
          "mode": {
            "type": "string",
            "enum": ["dryRun", "apply"]
          },
          "fallbackToAgentsDir": {
            "type": "boolean",
            "description": "Use project-level .agents/skills fallback when native installation is unavailable."
          },
          "projectRoot": {
            "type": "string",
            "description": "Optional project root path used when scope=project."
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
            name: "apply_config_patch".to_owned(),
            description_key: "mcpToolApplyConfigPatch".to_owned(),
            enabled: true,
        },
        McpToolDescriptor {
            name: "export_config".to_owned(),
            description_key: "mcpToolExportConfig".to_owned(),
            enabled: true,
        },
        McpToolDescriptor {
            name: "import_config".to_owned(),
            description_key: "mcpToolImportConfig".to_owned(),
            enabled: true,
        },
        McpToolDescriptor {
            name: "test_connectivity".to_owned(),
            description_key: "mcpToolTestConnectivity".to_owned(),
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
            name: "uninstall_agent_skill".to_owned(),
            description_key: "mcpToolUninstallAgentSkill".to_owned(),
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
    vec![
        McpClientPreset {
            id: "claude-code".to_owned(),
            label: "Claude Code".to_owned(),
            format: "bash".to_owned(),
            content: format!(
                "claude mcp add --scope user --transport http {name} {url}",
                name = config.server_name,
                url = base_url
            ),
        },
        McpClientPreset {
            id: "codex".to_owned(),
            label: "Codex".to_owned(),
            format: "toml".to_owned(),
            content: format!(
                "[mcp_servers.{name}]\nurl = \"{url}\"",
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
                  "url": base_url
                }
              }
            }))
            .unwrap_or_else(|_| "{}".to_owned()),
        },
    ]
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

    use serde_json::{json, Value};

    use super::MCP_PATH;
    use crate::state::AppState;
    use wsl_bridge_shared::{
        BindMode, CreateHostsGroupRequest, CreateProxyListenerRequest, CreateProxyRouteRequest,
        CreateProxyUpstreamRequest, HostsEntryInput, McpServerConfig, ProxyProtocol,
        ProxyTlsMode, SaveHostsEntriesRequest, TargetKind, UpstreamScheme,
    };

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

    fn test_mcp_config() -> McpServerConfig {
        McpServerConfig {
            enabled: true,
            server_name: "wsl-bridge".to_owned(),
            listen_port: 13746,
            expose_topology_read: true,
            expose_rule_config: true,
            expose_traffic_stats: true,
        }
    }

    fn send_http_request(port: u16, body: serde_json::Value) -> (u16, String) {
        let payload = serde_json::to_string(&body).expect("serialize body");
        let mut request = format!(
            "POST {MCP_PATH} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
            payload.len()
        );
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
            listen_port: blocked_port,
            ..test_mcp_config()
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
    fn mcp_http_localhost_requests_work_without_token() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind temp");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        let (state, path) = setup_state("mcp-auth");
        let config = McpServerConfig {
            listen_port: port,
            ..test_mcp_config()
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

        let (status, body) = send_http_request(actual_port, request);
        assert_eq!(status, 200);
        assert!(body.contains("\"result\""));
        assert!(body.contains("read_virtualization_topology"));

        cleanup_state(&state, path);
    }

    #[test]
    fn apply_config_patch_dry_run_detects_listener_conflict_without_side_effects() {
        let (state, path) = setup_state("mcp-apply-config-patch-conflict");
        state
            .engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "Existing".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port: 18081,
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create existing listener");

        let result = super::execute_apply_config_patch(
            &state.engine,
            json!({
              "mode": "dryRun",
              "patch": {
                "version": "phase3.ai-patch.v1",
                "proxy": {
                  "listeners": {
                    "create": [
                      {
                        "clientId": "listener-1",
                        "name": "Conflict",
                        "bindAddress": "127.0.0.1",
                        "port": 18081,
                        "protocol": "http"
                      }
                    ]
                  }
                }
              }
            }),
        )
        .expect("dry run");

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(false));
        assert_eq!(
            result
                .get("conflicts")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(state.engine.list_proxy_listeners().len(), 1);

        cleanup_state(&state, path);
    }

    #[test]
    fn apply_config_patch_dry_run_hosts_activate_returns_warning_without_writing() {
        let (state, path) = setup_state("mcp-apply-config-patch-hosts");
        let group_id = state
            .engine
            .create_hosts_group(CreateHostsGroupRequest {
                name: "dev".to_owned(),
                description: Some("Development".to_owned()),
            })
            .expect("create hosts group");

        let result = super::execute_apply_config_patch(
            &state.engine,
            json!({
              "mode": "dryRun",
              "patch": {
                "version": "phase3.ai-patch.v1",
                "hosts": {
                  "groups": {
                    "activate": {
                      "groupRef": group_id
                    }
                  }
                }
              }
            }),
        )
        .expect("dry run");

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        let effects = result
            .get("effects")
            .and_then(Value::as_object)
            .expect("effects object");
        assert_eq!(
            effects.get("requiresAdmin").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            effects
                .get("requiresConfirmation")
                .and_then(Value::as_bool),
            Some(true)
        );
        let groups = state.engine.list_hosts_groups();
        assert!(groups.iter().all(|group| !group.is_active));

        cleanup_state(&state, path);
    }

    #[test]
    fn apply_config_patch_apply_creates_proxy_chain_and_records_references() {
        let (state, path) = setup_state("mcp-apply-config-patch-apply-success");

        let result = super::execute_apply_config_patch(
            &state.engine,
            json!({
              "mode": "apply",
              "idempotencyKey": "apply-success-1",
              "patch": {
                "version": "phase3.ai-patch.v1",
                "reason": "create proxy chain",
                "proxy": {
                  "listeners": {
                    "create": [
                      {
                        "clientId": "listener-1",
                        "name": "Demo Listener",
                        "bindAddress": "127.0.0.1",
                        "port": 19091,
                        "protocol": "http"
                      }
                    ]
                  },
                  "routes": {
                    "create": [
                      {
                        "clientId": "route-1",
                        "listenerRef": "listener-1",
                        "serverNames": ["demo.local"],
                        "pathPrefix": "/",
                        "isDefault": true,
                        "upstreamRef": "upstream-1"
                      }
                    ]
                  },
                  "upstreams": {
                    "create": [
                      {
                        "clientId": "upstream-1",
                        "targetType": "static",
                        "targetHost": "127.0.0.1",
                        "targetPort": 3000,
                        "protocol": "http"
                      }
                    ]
                  }
                }
              }
            }),
        )
        .expect("apply patch");

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        let references = result
            .get("references")
            .and_then(Value::as_object)
            .expect("references");
        assert_eq!(
            references
                .get("listeners")
                .and_then(Value::as_object)
                .and_then(|items| items.get("listener-1"))
                .and_then(Value::as_str)
                .is_some(),
            true
        );

        let listeners = state.engine.list_proxy_listeners();
        assert_eq!(listeners.len(), 1);
        let routes = state
            .engine
            .list_proxy_routes(&listeners[0].id)
            .expect("routes");
        assert_eq!(routes.len(), 1);
        let upstreams = state
            .engine
            .list_proxy_upstreams(&routes[0].id)
            .expect("upstreams");
        assert_eq!(upstreams.len(), 1);

        cleanup_state(&state, path);
    }

    #[test]
    fn apply_config_patch_apply_rolls_back_when_later_hosts_delete_fails() {
        let (state, path) = setup_state("mcp-apply-config-patch-rollback");
        let group_id = state
            .engine
            .create_hosts_group(CreateHostsGroupRequest {
                name: "active-group".to_owned(),
                description: None,
            })
            .expect("create hosts group");
        let mut snapshot = state.engine.capture_snapshot();
        snapshot
            .hosts_groups
            .get_mut(&group_id)
            .expect("group in snapshot")
            .is_active = true;
        state
            .engine
            .restore_snapshot(snapshot)
            .expect("restore active group snapshot");

        let result = super::execute_apply_config_patch(
            &state.engine,
            json!({
              "mode": "apply",
              "patch": {
                "version": "phase3.ai-patch.v1",
                "proxy": {
                  "listeners": {
                    "create": [
                      {
                        "clientId": "listener-rollback",
                        "name": "Rollback Listener",
                        "bindAddress": "127.0.0.1",
                        "port": 19101,
                        "protocol": "http"
                      }
                    ]
                  }
                },
                "hosts": {
                  "groups": {
                    "delete": [
                      {
                        "groupRef": group_id
                      }
                    ]
                  }
                }
              }
            }),
        )
        .expect("apply patch");

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(false));
        assert_eq!(state.engine.list_proxy_listeners().len(), 0);
        let groups = state.engine.list_hosts_groups();
        assert_eq!(groups.len(), 1);
        assert!(groups[0].is_active);

        cleanup_state(&state, path);
    }

    #[test]
    fn apply_config_patch_apply_skips_duplicate_idempotency_key() {
        let (state, path) = setup_state("mcp-apply-config-patch-idempotency");
        let request = json!({
          "mode": "apply",
          "idempotencyKey": "same-key",
          "patch": {
            "version": "phase3.ai-patch.v1",
            "proxy": {
              "listeners": {
                "create": [
                  {
                    "clientId": "listener-1",
                    "name": "One Shot",
                    "bindAddress": "127.0.0.1",
                    "port": 19111,
                    "protocol": "http"
                  }
                ]
              }
            }
          }
        });

        let first = super::execute_apply_config_patch(&state.engine, request.clone())
            .expect("first apply");
        let second = super::execute_apply_config_patch(&state.engine, request)
            .expect("second apply");

        assert_eq!(first.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(second.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(state.engine.list_proxy_listeners().len(), 1);
        assert_eq!(
            second
                .get("effects")
                .and_then(Value::as_object)
                .and_then(|effects| effects.get("skippedDuplicate"))
                .and_then(Value::as_bool),
            Some(true)
        );

        cleanup_state(&state, path);
    }

    #[test]
    fn apply_config_patch_apply_updates_existing_listener_name_by_id() {
        let (state, path) = setup_state("mcp-apply-config-patch-update-listener");
        let listener_id = state
            .engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "Before Rename".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port: 19121,
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");

        let result = super::execute_apply_config_patch(
            &state.engine,
            json!({
              "mode": "apply",
              "patch": {
                "version": "phase3.ai-patch.v1",
                "reason": "rename listener",
                "proxy": {
                  "listeners": {
                    "update": [
                      {
                        "id": listener_id,
                        "name": "After Rename"
                      }
                    ]
                  }
                }
              }
            }),
        )
        .expect("apply patch");

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        let listeners = state.engine.list_proxy_listeners();
        assert_eq!(listeners.len(), 1);
        assert_eq!(listeners[0].name, "After Rename");
        assert_eq!(listeners[0].listen_port, 19121);

        cleanup_state(&state, path);
    }

    #[test]
    fn apply_config_patch_apply_updates_existing_listener_name_by_selector() {
        let (state, path) = setup_state("mcp-apply-config-patch-update-listener-selector");
        state
            .engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "Selector Target".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port: 19122,
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");

        let result = super::execute_apply_config_patch(
            &state.engine,
            json!({
              "mode": "apply",
              "patch": {
                "version": "phase3.ai-patch.v1",
                "reason": "rename listener by selector",
                "proxy": {
                  "listeners": {
                    "update": [
                      {
                        "listenerName": "Selector Target",
                        "name": "Selector Target Updated"
                      }
                    ]
                  }
                }
              }
            }),
        )
        .expect("apply patch");

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        let listeners = state.engine.list_proxy_listeners();
        assert_eq!(listeners.len(), 1);
        assert_eq!(listeners[0].name, "Selector Target Updated");

        cleanup_state(&state, path);
    }

    #[test]
    fn apply_config_patch_apply_deletes_existing_route_by_id() {
        let (state, path) = setup_state("mcp-apply-config-patch-delete-route");
        let listener_id = state
            .engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "Delete Route Listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port: 19131,
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");
        let route_id = state
            .engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: vec!["delete.local".to_owned()],
                path_prefix: Some("/".to_owned()),
                is_default: true,
                enabled: true,
            })
            .expect("create route");
        state
            .engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id: route_id.clone(),
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port: 3000,
                upstream_scheme: UpstreamScheme::Http,
                path_rewrite_from: None,
                path_rewrite_to: None,
                enabled: true,
            })
            .expect("create upstream");

        let result = super::execute_apply_config_patch(
            &state.engine,
            json!({
              "mode": "apply",
              "patch": {
                "version": "phase3.ai-patch.v1",
                "reason": "delete route",
                "proxy": {
                  "routes": {
                    "delete": [
                      {
                        "id": route_id
                      }
                    ]
                  }
                }
              }
            }),
        )
        .expect("apply patch");

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        let listeners = state.engine.list_proxy_listeners();
        assert_eq!(listeners.len(), 1);
        let routes = state.engine.list_proxy_routes(&listener_id).expect("list routes");
        assert_eq!(routes.len(), 0);

        cleanup_state(&state, path);
    }

    #[test]
    fn apply_config_patch_apply_deletes_existing_route_by_selector() {
        let (state, path) = setup_state("mcp-apply-config-patch-delete-route-selector");
        let listener_id = state
            .engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "Delete Route Selector Listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port: 19132,
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");
        let route_id = state
            .engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: vec!["selector.local".to_owned()],
                path_prefix: Some("/api".to_owned()),
                is_default: false,
                enabled: true,
            })
            .expect("create route");
        state
            .engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id: route_id.clone(),
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port: 3001,
                upstream_scheme: UpstreamScheme::Http,
                path_rewrite_from: None,
                path_rewrite_to: None,
                enabled: true,
            })
            .expect("create upstream");

        let result = super::execute_apply_config_patch(
            &state.engine,
            json!({
              "mode": "apply",
              "patch": {
                "version": "phase3.ai-patch.v1",
                "reason": "delete route by selector",
                "proxy": {
                  "routes": {
                    "delete": [
                      {
                        "matchListenerName": "Delete Route Selector Listener",
                        "matchServerNames": ["selector.local"],
                        "matchPathPrefix": "/api"
                      }
                    ]
                  }
                }
              }
            }),
        )
        .expect("apply patch");

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        let routes = state.engine.list_proxy_routes(&listener_id).expect("list routes");
        assert_eq!(routes.len(), 0);

        cleanup_state(&state, path);
    }

    #[test]
    fn export_config_json_returns_requested_proxy_module() {
        let (state, path) = setup_state("mcp-export-config-json");
        let listener_id = state
            .engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "Export Listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port: 19121,
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");
        let route_id = state
            .engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id,
                server_names: vec!["export.local".to_owned()],
                path_prefix: Some("/".to_owned()),
                is_default: true,
                enabled: true,
            })
            .expect("create route");
        state
            .engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port: 3001,
                upstream_scheme: UpstreamScheme::Http,
                path_rewrite_from: None,
                path_rewrite_to: None,
                enabled: true,
            })
            .expect("create upstream");

        let result = super::execute_export_config(
            &state.engine,
            json!({
              "modules": ["proxy"],
              "format": "json"
            }),
        )
        .expect("export config");

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        let modules = result
            .get("modules")
            .and_then(Value::as_array)
            .expect("modules");
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].as_str(), Some("proxy"));
        let listeners = result
            .get("content")
            .and_then(|value| value.get("content"))
            .and_then(|value| value.get("proxy"))
            .and_then(|value| value.get("topology"))
            .and_then(Value::as_array)
            .expect("proxy topology");
        assert_eq!(listeners.len(), 1);

        cleanup_state(&state, path);
    }

    #[test]
    fn export_config_hosts_file_uses_requested_group() {
        let (state, path) = setup_state("mcp-export-config-hosts-file");
        let group_id = state
            .engine
            .create_hosts_group(CreateHostsGroupRequest {
                name: "export-group".to_owned(),
                description: Some("for hosts export".to_owned()),
            })
            .expect("create group");
        state
            .engine
            .save_hosts_entries(SaveHostsEntriesRequest {
                group_id: group_id.clone(),
                entries: vec![
                    HostsEntryInput {
                        id: None,
                        ip: "127.0.0.1".to_owned(),
                        domain: "local.test".to_owned(),
                        comment: Some("enabled".to_owned()),
                        enabled: true,
                        order_index: 0,
                    },
                    HostsEntryInput {
                        id: None,
                        ip: "::1".to_owned(),
                        domain: "disabled.test".to_owned(),
                        comment: None,
                        enabled: false,
                        order_index: 1,
                    },
                ],
            })
            .expect("save hosts entries");

        let result = super::execute_export_config(
            &state.engine,
            json!({
              "modules": ["hosts"],
              "format": "hosts-file",
              "groupRef": group_id
            }),
        )
        .expect("export hosts file");

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        let content = result.get("content").and_then(Value::as_str).expect("content");
        assert!(content.contains("127.0.0.1 local.test # enabled"));
        assert!(!content.contains("disabled.test"));

        cleanup_state(&state, path);
    }

    #[test]
    fn import_config_hosts_file_dry_run_builds_hosts_patch_without_side_effects() {
        let (state, path) = setup_state("mcp-import-config-hosts-dry-run");

        let result = super::execute_import_config(
            &state.engine,
            json!({
              "module": "hosts",
              "mode": "dryRun",
              "content": "127.0.0.1 import.local # comment\n::1 ipv6.local\n"
            }),
        )
        .expect("import config");

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result.get("importKind").and_then(Value::as_str),
            Some("hosts-file")
        );
        assert_eq!(state.engine.list_hosts_groups().len(), 0);
        let effects = result
            .get("effects")
            .and_then(Value::as_object)
            .expect("effects");
        assert_eq!(
            effects
                .get("creates")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(3)
        );

        cleanup_state(&state, path);
    }

    #[test]
    fn import_config_proxy_json_apply_creates_proxy_topology() {
        let (state, path) = setup_state("mcp-import-config-proxy-apply");

        let result = super::execute_import_config(
            &state.engine,
            json!({
              "module": "proxy",
              "mode": "apply",
              "content": serde_json::to_string(&json!({
                "proxy": {
                  "certificates": [],
                  "topology": [
                    {
                      "listener": {
                        "name": "Imported Listener",
                        "listen_host": "127.0.0.1",
                        "listen_port": 19131,
                        "protocol": "http",
                        "tls_mode": "disabled",
                        "cert_id": Value::Null,
                        "bind_mode": "all_nics",
                        "nic_id": Value::Null,
                        "enabled": true
                      },
                      "routes": [
                        {
                          "route": {
                            "server_names": ["import.proxy.local"],
                            "path_prefix": "/api",
                            "is_default": false,
                            "enabled": true
                          },
                          "upstreams": [
                            {
                              "target_kind": "static",
                              "target_ref": Value::Null,
                              "target_host": "127.0.0.1",
                              "target_port": 3002,
                              "upstream_scheme": "http",
                              "path_rewrite_from": "/api",
                              "path_rewrite_to": "/",
                              "enabled": true
                            }
                          ]
                        }
                      ]
                    }
                  ]
                }
              })).expect("serialize proxy import")
            }),
        )
        .expect("import config");

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result.get("importKind").and_then(Value::as_str),
            Some("proxy-json")
        );
        let listeners = state.engine.list_proxy_listeners();
        assert_eq!(listeners.len(), 1);
        let routes = state
            .engine
            .list_proxy_routes(&listeners[0].id)
            .expect("routes");
        assert_eq!(routes.len(), 1);
        let upstreams = state
            .engine
            .list_proxy_upstreams(&routes[0].id)
            .expect("upstreams");
        assert_eq!(upstreams.len(), 1);

        cleanup_state(&state, path);
    }

    #[test]
    fn install_agent_skill_apply_generic_project_writes_managed_skill_files() {
        let project_root = std::env::temp_dir().join(format!(
            "wsl-bridge-agent-install-project-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("duration")
                .as_nanos()
        ));
        let user_root = std::env::temp_dir().join(format!(
            "wsl-bridge-agent-install-user-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("duration")
                .as_nanos()
        ));
        fs::create_dir_all(&project_root).expect("create project root");
        fs::create_dir_all(&user_root).expect("create user root");

        let plan = super::build_agent_skill_install_plan(
            "generic",
            "project",
            true,
            &project_root,
            &user_root,
        )
            .expect("build install plan");
        let applied = super::install_agent_skill_plan(
            &plan,
            &project_root,
            &user_root,
            &test_mcp_config(),
        )
            .expect("install skill");

        assert!(!applied.is_empty());
        let skill_path = project_root
            .join(".agents")
            .join("skills")
            .join("wsl-bridge-operator")
            .join("SKILL.md");
        let manifest_path = project_root
            .join(".agents")
            .join("skills")
            .join("wsl-bridge-operator")
            .join("manifest.json");
        let skill_text = fs::read_to_string(&skill_path).expect("read installed skill");
        assert!(skill_text.contains("<!-- managed-by: wsl-bridge -->"));
        let manifest_text = fs::read_to_string(&manifest_path).expect("read installed manifest");
        assert!(manifest_text.contains("\"_managedBy\": \"wsl-bridge\""));

        let _ = fs::remove_dir_all(project_root);
        let _ = fs::remove_dir_all(user_root);
    }

    #[test]
    fn install_agent_skill_apply_cursor_writes_skill_directory() {
        let project_root = std::env::temp_dir().join(format!(
            "wsl-bridge-agent-cursor-project-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("duration")
                .as_nanos()
        ));
        let user_root = std::env::temp_dir().join(format!(
            "wsl-bridge-agent-cursor-user-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("duration")
                .as_nanos()
        ));
        fs::create_dir_all(&project_root).expect("create project root");
        fs::create_dir_all(&user_root).expect("create user root");

        let plan = super::build_agent_skill_install_plan(
            "cursor",
            "project",
            true,
            &project_root,
            &user_root,
        )
            .expect("build install plan");
        let applied = super::install_agent_skill_plan(
            &plan,
            &project_root,
            &user_root,
            &test_mcp_config(),
        )
            .expect("install cursor skill");

        assert!(!applied.is_empty());
        let skill_path = project_root
            .join(".cursor")
            .join("skills")
            .join("wsl-bridge-operator")
            .join("SKILL.md");
        let skill_text = fs::read_to_string(&skill_path).expect("read cursor skill");
        assert!(skill_text.contains("managed-by: wsl-bridge"));

        let _ = fs::remove_dir_all(project_root);
        let _ = fs::remove_dir_all(user_root);
    }

    #[test]
    fn install_agent_skill_apply_opencode_preserves_skill_frontmatter() {
        let project_root = std::env::temp_dir().join(format!(
            "wsl-bridge-agent-opencode-project-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("duration")
                .as_nanos()
        ));
        let user_root = std::env::temp_dir().join(format!(
            "wsl-bridge-agent-opencode-user-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("duration")
                .as_nanos()
        ));
        fs::create_dir_all(&project_root).expect("create project root");
        fs::create_dir_all(&user_root).expect("create user root");

        let plan = super::build_agent_skill_install_plan(
            "opencode",
            "project",
            true,
            &project_root,
            &user_root,
        )
        .expect("build install plan");
        let applied = super::install_agent_skill_plan(
            &plan,
            &project_root,
            &user_root,
            &test_mcp_config(),
        )
            .expect("install opencode skill");

        assert!(!applied.is_empty());
        let skill_path = project_root
            .join(".opencode")
            .join("skills")
            .join("wsl-bridge-operator")
            .join("SKILL.md");
        let skill_text = fs::read_to_string(&skill_path).expect("read opencode skill");
        assert!(skill_text.starts_with("---\n"));
        assert!(skill_text.contains("<!-- managed-by: wsl-bridge -->"));
        let frontmatter_end = skill_text[4..]
            .find("\n---\n")
            .map(|idx| idx + 9)
            .expect("frontmatter terminator");
        assert!(frontmatter_end <= skill_text.find("<!-- managed-by: wsl-bridge -->").expect("marker"));
        assert!(!project_root.join("opencode.json").exists());
        assert!(!project_root.join(".wsl-bridge-opencode-managed.json").exists());

        let _ = fs::remove_dir_all(project_root);
        let _ = fs::remove_dir_all(user_root);
    }

    #[test]
    fn install_and_uninstall_opencode_mcp_client_manage_only_owned_entry() {
        let project_root = std::env::temp_dir().join(format!(
            "wsl-bridge-agent-opencode-merge-project-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("duration")
                .as_nanos()
        ));
        let user_root = std::env::temp_dir().join(format!(
            "wsl-bridge-agent-opencode-merge-user-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("duration")
                .as_nanos()
        ));
        fs::create_dir_all(&project_root).expect("create project root");
        fs::create_dir_all(&user_root).expect("create user root");

        let opencode_config_path = user_root
            .join(".config")
            .join("opencode")
            .join("opencode.json");
        fs::create_dir_all(
            opencode_config_path
                .parent()
                .expect("opencode config parent"),
        )
        .expect("create opencode config parent");
        fs::write(
            &opencode_config_path,
            serde_json::to_string_pretty(&json!({
              "mcp": {
                "user-service": {
                  "type": "remote",
                  "url": "http://127.0.0.1:4000/mcp",
                  "enabled": true
                }
              },
              "theme": "dark"
            }))
            .expect("serialize opencode config"),
        )
        .expect("write existing opencode config");

        let (installed_path, metadata_installed_path) = super::install_agent_mcp_client_for_user_root(
            "opencode",
            &user_root,
            &test_mcp_config(),
        )
        .expect("install opencode mcp client");
        assert!(installed_path.exists());
        assert!(metadata_installed_path.exists());

        let installed_config = fs::read_to_string(&opencode_config_path).expect("read installed config");
        assert!(installed_config.contains("\"user-service\""));
        assert!(installed_config.contains("\"theme\": \"dark\""));
        assert!(installed_config.contains("\"wsl-bridge\""));
        assert!(!installed_config.contains("\"_managedBy\": \"wsl-bridge\""));
        let metadata_path = user_root
            .join(".config")
            .join("opencode")
            .join(".wsl-bridge-opencode-managed.json");
        let metadata_text = fs::read_to_string(&metadata_path).expect("read sidecar metadata");
        assert!(metadata_text.contains("\"_managedBy\": \"wsl-bridge\""));

        let (uninstalled_path, removed) = super::uninstall_agent_mcp_client_for_user_root(
            "opencode",
            &user_root,
            &test_mcp_config(),
        )
        .expect("uninstall opencode mcp client");
        assert!(removed);
        assert_eq!(uninstalled_path, opencode_config_path);

        let final_config = fs::read_to_string(&opencode_config_path).expect("read final config");
        assert!(final_config.contains("\"user-service\""));
        assert!(final_config.contains("\"theme\": \"dark\""));
        assert!(!final_config.contains("\"wsl-bridge\""));
        assert!(!final_config.contains("\"_managedBy\": \"wsl-bridge\""));
        assert!(!metadata_path.exists());

        let _ = fs::remove_dir_all(project_root);
        let _ = fs::remove_dir_all(user_root);
    }

    #[test]
    fn detect_opencode_without_skill_but_with_user_config_as_not_installed() {
        let project_root = std::env::temp_dir().join(format!(
            "wsl-bridge-agent-opencode-detect-project-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("duration")
                .as_nanos()
        ));
        let user_root = std::env::temp_dir().join(format!(
            "wsl-bridge-agent-opencode-detect-user-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("duration")
                .as_nanos()
        ));
        fs::create_dir_all(&project_root).expect("create project root");
        let opencode_config_path = user_root
            .join(".config")
            .join("opencode");
        fs::create_dir_all(&opencode_config_path).expect("create opencode config dir");
        fs::write(
            opencode_config_path.join("opencode.json"),
            serde_json::to_string_pretty(&json!({
              "mcp": {
                "user-service": {
                  "type": "remote",
                  "url": "http://127.0.0.1:4000/mcp",
                  "enabled": true
                }
              }
            }))
            .expect("serialize config"),
        )
        .expect("write opencode config");

        let detected = super::detect_agent_skill_install_state(
            "opencode",
            "user",
            true,
            &project_root,
            &user_root,
        )
        .expect("detect opencode install state");
        assert_eq!(detected, "not_installed");

        let _ = fs::remove_dir_all(project_root);
        let _ = fs::remove_dir_all(user_root);
    }

    #[test]
    fn detect_agent_skill_install_state_reports_installed_after_apply() {
        let project_root = std::env::temp_dir().join(format!(
            "wsl-bridge-agent-detect-project-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("duration")
                .as_nanos()
        ));
        let user_root = std::env::temp_dir().join(format!(
            "wsl-bridge-agent-detect-user-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("duration")
                .as_nanos()
        ));
        fs::create_dir_all(&project_root).expect("create project root");
        fs::create_dir_all(&user_root).expect("create user root");

        let plan = super::build_agent_skill_install_plan(
            "generic",
            "project",
            true,
            &project_root,
            &user_root,
        )
            .expect("build install plan");
        super::install_agent_skill_plan(
            &plan,
            &project_root,
            &user_root,
            &test_mcp_config(),
        )
            .expect("install generic skill");

        let detected = super::detect_agent_skill_install_state(
            "generic",
            "project",
            true,
            &project_root,
            &user_root,
        )
        .expect("detect install state");
        assert_eq!(detected, "installed");

        let _ = fs::remove_dir_all(project_root);
        let _ = fs::remove_dir_all(user_root);
    }

    #[test]
    fn uninstall_agent_skill_apply_removes_only_managed_files() {
        let project_root = std::env::temp_dir().join(format!(
            "wsl-bridge-agent-uninstall-project-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("duration")
                .as_nanos()
        ));
        let user_root = std::env::temp_dir().join(format!(
            "wsl-bridge-agent-uninstall-user-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("duration")
                .as_nanos()
        ));
        fs::create_dir_all(&project_root).expect("create project root");
        fs::create_dir_all(&user_root).expect("create user root");

        let plan = super::build_agent_skill_install_plan(
            "generic",
            "user",
            true,
            &project_root,
            &user_root,
        )
            .expect("build install plan");
        super::install_agent_skill_plan(
            &plan,
            &project_root,
            &user_root,
            &test_mcp_config(),
        )
            .expect("install generic skill");

        let managed_manifest = user_root
            .join(".agents")
            .join("skills")
            .join("wsl-bridge-operator")
            .join("manifest.json");
        let unmanaged_note = user_root
            .join(".agents")
            .join("skills")
            .join("wsl-bridge-operator")
            .join("notes.md");
        fs::write(&unmanaged_note, "user-owned").expect("write unmanaged note");
        assert!(managed_manifest.exists());
        assert!(unmanaged_note.exists());

        let uninstall_plan = super::build_agent_skill_uninstall_plan(
            "generic",
            "user",
            true,
            &project_root,
            &user_root,
        )
        .expect("build uninstall plan");
        let deleted = super::uninstall_agent_skill_plan(&uninstall_plan, &project_root, &user_root)
            .expect("uninstall managed files");

        assert!(!deleted.is_empty());
        assert!(!managed_manifest.exists());
        assert!(unmanaged_note.exists());

        let detected = super::detect_agent_skill_install_state(
            "generic",
            "user",
            true,
            &project_root,
            &user_root,
        )
        .expect("detect final state");
        assert_eq!(detected, "not_installed");

        let _ = fs::remove_dir_all(project_root);
        let _ = fs::remove_dir_all(user_root);
    }

    #[test]
    fn test_connectivity_host_port_reports_connected() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind temp listener");
        let port = listener.local_addr().expect("local addr").port();
        let (state, path) = setup_state("mcp-test-connectivity-host-port");

        let result = super::execute_test_connectivity(
            &state.engine,
            json!({
              "target": {
                "type": "host-port",
                "value": {
                  "host": "127.0.0.1",
                  "port": port
                }
              }
            }),
        )
        .expect("connectivity probe");

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result.get("stage").and_then(Value::as_str),
            Some("host_port_connect")
        );

        drop(listener);
        cleanup_state(&state, path);
    }

    #[test]
    fn test_connectivity_proxy_route_reports_upstream_failure_stage() {
        let (state, path) = setup_state("mcp-test-connectivity-proxy-route");
        let listener_id = state
            .engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "HTTP".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port: 19081,
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");
        let route_id = state
            .engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: vec!["a.test".to_owned()],
                path_prefix: Some("/api".to_owned()),
                is_default: false,
                enabled: true,
            })
            .expect("create route");
        state
            .engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id: route_id.clone(),
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port: 6551,
                upstream_scheme: UpstreamScheme::Http,
                path_rewrite_from: None,
                path_rewrite_to: None,
                enabled: true,
            })
            .expect("create upstream");
        thread::sleep(Duration::from_millis(120));

        let result = super::execute_test_connectivity(
            &state.engine,
            json!({
              "target": {
                "type": "proxy-route",
                "value": {
                  "routeRef": route_id,
                  "host": "a.test",
                  "path": "/api"
                }
              }
            }),
        )
        .expect("connectivity probe");

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(false));
        assert_eq!(
            result.get("stage").and_then(Value::as_str),
            Some("upstream_connect")
        );

        cleanup_state(&state, path);
    }
}
