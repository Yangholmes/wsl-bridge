use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuleType {
    TcpFwd,
    UdpFwd,
    HttpProxy,
    Socks5Proxy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Wsl,
    Hyperv,
    Static,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BindMode {
    SingleNic,
    AllNics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxyRule {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub rule_type: RuleType,
    pub listen_host: String,
    pub listen_port: u16,
    pub target_kind: TargetKind,
    pub target_ref: Option<String>,
    pub target_host: Option<String>,
    pub target_port: Option<u16>,
    pub bind_mode: BindMode,
    pub nic_id: Option<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FirewallPolicy {
    pub rule_id: String,
    pub allow_domain: bool,
    pub allow_private: bool,
    pub allow_public: bool,
    pub direction: String,
    pub action: String,
}

impl FirewallPolicy {
    pub fn default_allow(rule_id: String) -> Self {
        Self {
            rule_id,
            allow_domain: true,
            allow_private: true,
            allow_public: false,
            direction: "inbound".to_owned(),
            action: "allow".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
    Running,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeStatusItem {
    pub rule_id: String,
    pub state: RuntimeState,
    pub last_error: Option<String>,
    pub last_apply_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditLog {
    pub id: u64,
    pub time: DateTime<Utc>,
    pub level: String,
    pub module: String,
    pub event: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterInfo {
    pub id: String,
    pub name: String,
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WslInfo {
    pub distro: String,
    pub networking_mode: String,
    pub ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HyperVVmInfo {
    pub vm_name: String,
    pub v_switch: Option<String>,
    pub ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TopologySnapshot {
    pub adapters: Vec<AdapterInfo>,
    pub wsl: Vec<WslInfo>,
    pub hyperv: Vec<HyperVVmInfo>,
    pub hyperv_error: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct McpServerConfig {
    pub enabled: bool,
    pub server_name: String,
    pub listen_port: u16,
    pub api_token: String,
    pub expose_topology_read: bool,
    pub expose_rule_config: bool,
    pub expose_traffic_stats: bool,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            server_name: "wsl-bridge".to_owned(),
            listen_port: 13746,
            api_token: String::new(),
            expose_topology_read: true,
            expose_rule_config: true,
            expose_traffic_stats: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpToolDescriptor {
    pub name: String,
    pub description: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpServerStatus {
    pub config: McpServerConfig,
    pub base_url: String,
    pub running: bool,
    pub last_error: Option<String>,
    pub tools: Vec<McpToolDescriptor>,
    pub client_presets: Vec<McpClientPreset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpClientPreset {
    pub id: String,
    pub label: String,
    pub format: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuildFlavor {
    Standard,
    Su,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppRuntimeStatus {
    pub build_flavor: BuildFlavor,
    pub is_admin: bool,
    pub admin_features_available: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CloseBehavior {
    #[default]
    Ask,
    Minimize,
    Exit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AppSettings {
    pub close_behavior: CloseBehavior,
    pub show_tray_on_start: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            close_behavior: CloseBehavior::Ask,
            show_tray_on_start: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateRuleRequest {
    pub rule: NewProxyRule,
    pub firewall: Option<NewFirewallPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewProxyRule {
    pub name: String,
    #[serde(rename = "type")]
    pub rule_type: RuleType,
    pub listen_host: String,
    pub listen_port: u16,
    pub target_kind: TargetKind,
    pub target_ref: Option<String>,
    pub target_host: Option<String>,
    pub target_port: Option<u16>,
    pub bind_mode: BindMode,
    pub nic_id: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RulePatch {
    pub name: Option<String>,
    pub listen_host: Option<String>,
    pub listen_port: Option<u16>,
    pub target_ref: Option<Option<String>>,
    pub target_host: Option<Option<String>>,
    pub target_port: Option<Option<u16>>,
    pub bind_mode: Option<BindMode>,
    pub nic_id: Option<Option<String>>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct NewFirewallPolicy {
    pub allow_domain: bool,
    pub allow_private: bool,
    pub allow_public: bool,
    pub direction: Option<String>,
    pub action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplyRulesResult {
    pub applied: usize,
    pub failed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StopRulesResult {
    pub stopped: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TailLogsResult {
    pub events: Vec<AuditLog>,
    pub next_cursor: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct LogQueryRequest {
    pub level: Option<String>,
    pub module: Option<String>,
    pub rule_id: Option<String>,
    pub keyword: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
    pub newest_first: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogQueryResult {
    pub total: usize,
    pub events: Vec<AuditLog>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RuleLogStatsRequest {
    pub rule_ids: Option<Vec<String>>,
    pub since_minutes: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleLogStatsItem {
    pub rule_id: String,
    pub total: usize,
    pub errors: usize,
    pub last_time: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrafficSample {
    pub timestamp: i64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub connections: u64,
    pub total_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrafficWindowData {
    pub rule_id: String,
    pub samples: Vec<TrafficSample>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TrafficStatsInterval {
    #[default]
    Minute,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct QueryTrafficStatsRequest {
    pub rule_id: String,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub interval: Option<TrafficStatsInterval>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrafficStatsPoint {
    pub time_bucket: i64,
    pub rule_id: String,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub connections: u64,
    pub requests: u64,
    pub total_duration_ms: u64,
    pub avg_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryTrafficStatsResult {
    pub stats: Vec<TrafficStatsPoint>,
    pub total_bytes_in: u64,
    pub total_bytes_out: u64,
    pub total_connections: u64,
}
