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
