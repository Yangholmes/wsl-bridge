use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::{IpAddr, SocketAddr, TcpListener, ToSocketAddrs, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use parking_lot::{Mutex, RwLock};
use rcgen::{BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair};
use serde_json::json;
use thiserror::Error;
use tracing::warn;
use uuid::Uuid;
use wsl_bridge_shared::{
    AppSettings, ApplyRulesResult, AuditLog, BindMode, CopyHostsGroupRequest,
    CreateHostsGroupRequest, CreateProxyCertificateRequest, CreateProxyListenerRequest,
    CreateProxyRouteRequest, CreateProxyUpstreamRequest, CreateRuleRequest,
    ExportHostsGroupRequest, FirewallPolicy, HostsEntry, HostsEntryInput, HostsGroup,
    HostsGroupSourceType, ImportHostsGroupRequest, LogQueryRequest, LogQueryResult,
    McpServerConfig, NewFirewallPolicy, NewProxyRule, ProxyCertificate, ProxyCertificateSourceType,
    ProxyListener, ProxyProtocol, ProxyRoute, ProxyRouteRuntimeItem, ProxyRule,
    ProxyRuntimeStatusItem, ProxyTlsMode, ProxyUpstream, ProxyUpstreamRuntimeItem,
    QueryTrafficStatsRequest, QueryTrafficStatsResult, RuleLogStatsItem, RuleLogStatsRequest,
    RuleMigrationRecord, RuleMigrationStatus, RulePatch, RuleType, RuntimeState, RuntimeStatusItem,
    SaveHostsEntriesRequest, StopRulesResult, TailLogsResult, TargetKind, TopologySnapshot,
    TrafficEntityType, TrafficMonitorEntity, TrafficWindowData, TrafficWindowQueryEntity,
    UpdateHostsGroupRequest, UpdateProxyCertificateRequest, UpdateProxyListenerRequest,
    UpdateProxyRouteRequest, UpdateProxyUpstreamRequest, UpstreamScheme,
};

use crate::app_logs::{AppLogger, ErrorLogEntry};
use crate::firewall::{apply_firewall, cleanup_firewall, FirewallMode, FirewallRuleRuntime};
use crate::forwarder::{
    spawn as spawn_forwarder, spawn_http_reverse_proxy, spawn_https_reverse_proxy, ForwarderHandle,
    ForwarderKind,
};
use crate::hosts::{
    read_hosts_file, render_hosts_text, resolve_system_hosts_path, write_hosts_file,
};
use crate::proxy_metrics::ProxyMetricsTracker;
use crate::sqlite_store::{Snapshot, SqliteStore};
use crate::topology::{
    debug_hyperv_probe, list_adapters, list_wsl_instances, resolve_dynamic_target_candidates,
    resolve_dynamic_target_host, resolve_nic_ip, scan_hyperv, HyperVProbeDebug,
};
use crate::traffic::TrafficTracker;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("rule not found: {0}")]
    RuleNotFound(String),
    #[error("hosts group not found: {0}")]
    HostsGroupNotFound(String),
    #[error("proxy listener not found: {0}")]
    ProxyListenerNotFound(String),
    #[error("proxy route not found: {0}")]
    ProxyRouteNotFound(String),
    #[error("proxy upstream not found: {0}")]
    ProxyUpstreamNotFound(String),
    #[error("proxy certificate not found: {0}")]
    ProxyCertificateNotFound(String),
    #[error("invalid rule: {0}")]
    InvalidRule(String),
    #[error("invalid hosts data: {0}")]
    InvalidHosts(String),
    #[error("invalid proxy data: {0}")]
    InvalidProxy(String),
    #[error("storage error: {0}")]
    Storage(String),
}

#[derive(Debug, Clone, Copy)]
pub struct EngineOptions {
    pub firewall_mode: FirewallMode,
}

impl Default for EngineOptions {
    fn default() -> Self {
        Self {
            firewall_mode: FirewallMode::Disabled,
        }
    }
}

#[derive(Debug, Default)]
struct EngineStore {
    rules: HashMap<String, ProxyRule>,
    firewalls: HashMap<String, FirewallPolicy>,
    runtime: HashMap<String, RuntimeStatusItem>,
    hosts_groups: HashMap<String, HostsGroup>,
    hosts_entries: HashMap<String, Vec<HostsEntry>>,
    proxy_listeners: HashMap<String, ProxyListener>,
    proxy_routes: HashMap<String, Vec<ProxyRoute>>,
    proxy_upstreams: HashMap<String, Vec<ProxyUpstream>>,
    proxy_certificates: HashMap<String, ProxyCertificate>,
    proxy_runtime: HashMap<String, ProxyRuntimeStatusItem>,
    rule_migrations: HashMap<String, RuleMigrationRecord>,
    logs: Vec<AuditLog>,
    log_seq: u64,
    mcp_config: McpServerConfig,
    app_settings: AppSettings,
}

#[derive(Debug)]
struct ActiveRuleRuntime {
    forwarder: ForwarderHandle,
    firewall: FirewallRuleRuntime,
    rule_type: RuleType,
    listen_addr: SocketAddr,
    target_addr: Option<SocketAddr>,
}

#[derive(Debug)]
pub struct RuleEngine {
    store: RwLock<EngineStore>,
    sqlite: Option<Arc<SqliteStore>>,
    traffic: Arc<TrafficTracker>,
    proxy_metrics: Arc<ProxyMetricsTracker>,
    logger: Arc<AppLogger>,
    options: EngineOptions,
    active: Mutex<HashMap<String, ActiveRuleRuntime>>,
    active_proxy: Mutex<HashMap<String, ForwarderHandle>>,
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new_with_options(EngineOptions::default())
    }
}

impl Drop for RuleEngine {
    fn drop(&mut self) {
        self.stop_all_active_rules();
        self.stop_all_active_proxy_listeners();
    }
}

impl RuleEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_with_options(options: EngineOptions) -> Self {
        Self::new_with_options_and_logger(options, Arc::new(AppLogger::disabled()))
    }

    pub fn new_with_options_and_log_dir(
        options: EngineOptions,
        log_dir: impl AsRef<Path>,
    ) -> Result<Self, EngineError> {
        let logger =
            Arc::new(AppLogger::new(log_dir).map_err(|err| EngineError::Storage(err.to_string()))?);
        Ok(Self::new_with_options_and_logger(options, logger))
    }

    fn new_with_options_and_logger(options: EngineOptions, logger: Arc<AppLogger>) -> Self {
        let sqlite = None;
        Self {
            store: RwLock::new(EngineStore::default()),
            traffic: Arc::new(TrafficTracker::new(sqlite.clone())),
            proxy_metrics: Arc::new(ProxyMetricsTracker::new()),
            logger,
            sqlite,
            options,
            active: Mutex::new(HashMap::new()),
            active_proxy: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_sqlite(path: impl AsRef<Path>) -> Result<Self, EngineError> {
        Self::with_sqlite_and_options(path, EngineOptions::default())
    }

    pub fn with_sqlite_and_options(
        path: impl AsRef<Path>,
        options: EngineOptions,
    ) -> Result<Self, EngineError> {
        Self::with_sqlite_and_options_and_log_dir(path, options, None)
    }

    pub fn with_sqlite_and_options_and_log_dir(
        path: impl AsRef<Path>,
        options: EngineOptions,
        log_dir: Option<std::path::PathBuf>,
    ) -> Result<Self, EngineError> {
        let sqlite = Arc::new(SqliteStore::open(path)?);
        let snapshot = sqlite.load_snapshot()?;
        let logger = match log_dir {
            Some(path) => {
                Arc::new(AppLogger::new(path).map_err(|err| EngineError::Storage(err.to_string()))?)
            }
            None => Arc::new(AppLogger::disabled()),
        };
        let store = RwLock::new(EngineStore {
            rules: snapshot.rules,
            firewalls: snapshot.firewalls,
            runtime: snapshot.runtime,
            hosts_groups: snapshot.hosts_groups,
            hosts_entries: snapshot.hosts_entries,
            proxy_listeners: snapshot.proxy_listeners,
            proxy_routes: snapshot.proxy_routes,
            proxy_upstreams: snapshot.proxy_upstreams,
            proxy_certificates: snapshot.proxy_certificates,
            proxy_runtime: HashMap::new(),
            rule_migrations: snapshot.rule_migrations,
            logs: snapshot.logs,
            log_seq: snapshot.log_seq,
            mcp_config: snapshot.mcp_config,
            app_settings: snapshot.app_settings,
        });
        Ok(Self {
            store,
            traffic: Arc::new(TrafficTracker::new(Some(Arc::clone(&sqlite)))),
            proxy_metrics: Arc::new(ProxyMetricsTracker::new()),
            logger,
            sqlite: Some(sqlite),
            options,
            active: Mutex::new(HashMap::new()),
            active_proxy: Mutex::new(HashMap::new()),
        })
    }

    pub fn sqlite_path(&self) -> Option<&Path> {
        self.sqlite.as_ref().map(|store| store.path())
    }

    pub fn capture_snapshot(&self) -> Snapshot {
        let store = self.store.read();
        snapshot_from_store(&store)
    }

    pub fn restore_snapshot(&self, snapshot: Snapshot) -> Result<(), EngineError> {
        if let Some(sqlite) = &self.sqlite {
            sqlite.save_snapshot(&snapshot)?;
        }

        self.stop_all_active_proxy_listeners();

        {
            let mut store = self.store.write();
            store.rules = snapshot.rules;
            store.firewalls = snapshot.firewalls;
            store.runtime = snapshot.runtime;
            store.hosts_groups = snapshot.hosts_groups;
            store.hosts_entries = snapshot.hosts_entries;
            store.proxy_listeners = snapshot.proxy_listeners;
            store.proxy_routes = snapshot.proxy_routes;
            store.proxy_upstreams = snapshot.proxy_upstreams;
            store.proxy_certificates = snapshot.proxy_certificates;
            store.proxy_runtime = HashMap::new();
            store.rule_migrations = snapshot.rule_migrations;
            store.logs = snapshot.logs;
            store.log_seq = snapshot.log_seq;
            store.mcp_config = snapshot.mcp_config;
            store.app_settings = snapshot.app_settings;
        }

        self.apply_proxy_listeners();
        Ok(())
    }

    pub fn append_audit_log(&self, level: &str, module: &str, event: &str, detail: &str) {
        self.append_engine_log(level, module, event, detail);
    }

    pub fn scan_topology(&self) -> TopologySnapshot {
        let hyperv = scan_hyperv();
        if let Some(error) = hyperv.error.clone() {
            self.logger.log_error(
                ErrorLogEntry::new("topology_error", error).with_detail(json!({
                  "source": "hyperv_scan"
                })),
            );
        }
        TopologySnapshot {
            adapters: list_adapters(),
            wsl: list_wsl_instances(),
            hyperv: hyperv.items,
            hyperv_error: hyperv.error,
            timestamp: Utc::now(),
        }
    }

    pub fn debug_hyperv_probe(&self) -> HyperVProbeDebug {
        debug_hyperv_probe()
    }

    pub fn list_rules(&self) -> Vec<ProxyRule> {
        let store = self.store.read();
        let mut rules = store.rules.values().cloned().collect::<Vec<_>>();
        rules.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        rules
    }

    pub fn list_forward_rules_with_firewall(&self) -> Vec<(ProxyRule, FirewallPolicy)> {
        let store = self.store.read();
        let mut items = store
            .rules
            .values()
            .filter(|rule| matches!(rule.rule_type, RuleType::TcpFwd | RuleType::UdpFwd))
            .filter_map(|rule| {
                store
                    .firewalls
                    .get(&rule.id)
                    .cloned()
                    .map(|firewall| (rule.clone(), firewall))
            })
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.0.created_at.cmp(&b.0.created_at));
        items
    }

    pub fn get_mcp_config(&self) -> McpServerConfig {
        self.store.read().mcp_config.clone()
    }

    pub fn list_hosts_groups(&self) -> Vec<HostsGroup> {
        let store = self.store.read();
        let mut items = store.hosts_groups.values().cloned().collect::<Vec<_>>();
        items.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        items
    }

    pub fn list_hosts_entries(&self, group_id: &str) -> Result<Vec<HostsEntry>, EngineError> {
        let store = self.store.read();
        if !store.hosts_groups.contains_key(group_id) {
            return Err(EngineError::HostsGroupNotFound(group_id.to_owned()));
        }
        let mut items = store
            .hosts_entries
            .get(group_id)
            .cloned()
            .unwrap_or_default();
        items.sort_by_key(|entry| entry.order_index);
        Ok(items)
    }

    pub fn list_proxy_listeners(&self) -> Vec<ProxyListener> {
        let store = self.store.read();
        let mut items = store.proxy_listeners.values().cloned().collect::<Vec<_>>();
        items.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        items
    }

    pub fn list_proxy_certificates(&self) -> Vec<ProxyCertificate> {
        let store = self.store.read();
        let mut items = store
            .proxy_certificates
            .values()
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        items
    }

    pub fn list_proxy_routes(&self, listener_id: &str) -> Result<Vec<ProxyRoute>, EngineError> {
        let store = self.store.read();
        if !store.proxy_listeners.contains_key(listener_id) {
            return Err(EngineError::ProxyListenerNotFound(listener_id.to_owned()));
        }
        let mut items = store
            .proxy_routes
            .get(listener_id)
            .cloned()
            .unwrap_or_default();
        items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(items)
    }

    pub fn list_proxy_upstreams(&self, route_id: &str) -> Result<Vec<ProxyUpstream>, EngineError> {
        let store = self.store.read();
        if !store
            .proxy_routes
            .values()
            .any(|routes| routes.iter().any(|route| route.id == route_id))
        {
            return Err(EngineError::ProxyRouteNotFound(route_id.to_owned()));
        }
        let mut items = store
            .proxy_upstreams
            .get(route_id)
            .cloned()
            .unwrap_or_default();
        items.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(items)
    }

    pub fn get_proxy_runtime_status(&self) -> Vec<ProxyRuntimeStatusItem> {
        let store = self.store.read();
        let mut items = store.proxy_runtime.values().cloned().collect::<Vec<_>>();
        items.sort_by(|a, b| a.listener_id.cmp(&b.listener_id));
        items
    }

    pub fn list_proxy_route_runtime(&self, listener_id: &str) -> Vec<ProxyRouteRuntimeItem> {
        self.proxy_metrics.list_route_runtime(listener_id)
    }

    pub fn list_proxy_upstream_runtime(&self, route_id: &str) -> Vec<ProxyUpstreamRuntimeItem> {
        self.proxy_metrics.list_upstream_runtime(route_id)
    }

    pub fn list_rule_migrations(&self) -> Vec<RuleMigrationRecord> {
        let store = self.store.read();
        let mut items = store.rule_migrations.values().cloned().collect::<Vec<_>>();
        items.sort_by(|a, b| b.migrated_at.cmp(&a.migrated_at));
        items
    }

    pub fn get_app_settings(&self) -> AppSettings {
        self.store.read().app_settings.clone()
    }

    pub fn bootstrap_default_hosts_group(&self) -> Result<HostsGroup, EngineError> {
        let path = resolve_system_hosts_path();
        self.bootstrap_default_hosts_group_from_path(&path)
    }

    pub fn create_hosts_group(&self, req: CreateHostsGroupRequest) -> Result<String, EngineError> {
        let name = req.name.trim();
        if name.is_empty() {
            return Err(EngineError::InvalidHosts(
                "hosts group name is required".to_owned(),
            ));
        }

        let now = Utc::now();
        let group_id = Uuid::new_v4().to_string();
        let group = HostsGroup {
            id: group_id.clone(),
            name: name.to_owned(),
            description: clean_optional_text(req.description),
            source_type: HostsGroupSourceType::Manual,
            is_active: false,
            created_at: now,
            updated_at: now,
        };

        let mut store = self.store.write();
        store.hosts_groups.insert(group_id.clone(), group);
        store.hosts_entries.insert(group_id.clone(), Vec::new());
        append_log(
            &mut store,
            "info",
            "hosts",
            "hosts_group_created",
            &format!("group_id={group_id}"),
        );
        self.persist_store(&store);
        Ok(group_id)
    }

    pub fn update_hosts_group(
        &self,
        id: &str,
        req: UpdateHostsGroupRequest,
    ) -> Result<(), EngineError> {
        let name = req.name.trim();
        if name.is_empty() {
            return Err(EngineError::InvalidHosts(
                "hosts group name is required".to_owned(),
            ));
        }

        let mut store = self.store.write();
        let group = store
            .hosts_groups
            .get_mut(id)
            .ok_or_else(|| EngineError::HostsGroupNotFound(id.to_owned()))?;
        group.name = name.to_owned();
        group.description = clean_optional_text(req.description);
        group.updated_at = Utc::now();
        append_log(
            &mut store,
            "info",
            "hosts",
            "hosts_group_updated",
            &format!("group_id={id}"),
        );
        self.persist_store(&store);
        Ok(())
    }

    pub fn delete_hosts_group(&self, id: &str) -> Result<(), EngineError> {
        let mut store = self.store.write();
        let Some(group) = store.hosts_groups.get(id) else {
            return Err(EngineError::HostsGroupNotFound(id.to_owned()));
        };
        if group.is_active {
            return Err(EngineError::InvalidHosts(
                "active hosts group cannot be deleted".to_owned(),
            ));
        }
        store.hosts_groups.remove(id);
        store.hosts_entries.remove(id);
        append_log(
            &mut store,
            "info",
            "hosts",
            "hosts_group_deleted",
            &format!("group_id={id}"),
        );
        self.persist_store(&store);
        Ok(())
    }

    pub fn copy_hosts_group(&self, req: CopyHostsGroupRequest) -> Result<String, EngineError> {
        let name = req.name.trim();
        if name.is_empty() {
            return Err(EngineError::InvalidHosts(
                "copied hosts group name is required".to_owned(),
            ));
        }

        let mut store = self.store.write();
        let source_group = store
            .hosts_groups
            .get(&req.source_group_id)
            .cloned()
            .ok_or_else(|| EngineError::HostsGroupNotFound(req.source_group_id.clone()))?;
        let source_entries = store
            .hosts_entries
            .get(&req.source_group_id)
            .cloned()
            .unwrap_or_default();

        let now = Utc::now();
        let group_id = Uuid::new_v4().to_string();
        store.hosts_groups.insert(
            group_id.clone(),
            HostsGroup {
                id: group_id.clone(),
                name: name.to_owned(),
                description: clean_optional_text(req.description).or(source_group.description),
                source_type: HostsGroupSourceType::Copied,
                is_active: false,
                created_at: now,
                updated_at: now,
            },
        );
        store.hosts_entries.insert(
            group_id.clone(),
            source_entries
                .into_iter()
                .enumerate()
                .map(|(index, entry)| HostsEntry {
                    id: Uuid::new_v4().to_string(),
                    group_id: group_id.clone(),
                    ip: entry.ip,
                    domain: entry.domain,
                    comment: entry.comment,
                    enabled: entry.enabled,
                    order_index: index as u32,
                    created_at: now,
                    updated_at: now,
                })
                .collect(),
        );
        append_log(
            &mut store,
            "info",
            "hosts",
            "hosts_group_copied",
            &format!(
                "group_id={group_id},source_group_id={}",
                req.source_group_id
            ),
        );
        self.persist_store(&store);
        Ok(group_id)
    }

    pub fn save_hosts_entries(&self, req: SaveHostsEntriesRequest) -> Result<(), EngineError> {
        let mut store = self.store.write();
        if !store.hosts_groups.contains_key(&req.group_id) {
            return Err(EngineError::HostsGroupNotFound(req.group_id));
        }

        let now = Utc::now();
        let mut entries = Vec::with_capacity(req.entries.len());
        for (index, entry) in req.entries.into_iter().enumerate() {
            validate_hosts_entry_input(&entry)?;
            entries.push(HostsEntry {
                id: entry.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
                group_id: req.group_id.clone(),
                ip: entry.ip.trim().to_owned(),
                domain: entry.domain.trim().to_owned(),
                comment: clean_optional_text(entry.comment),
                enabled: entry.enabled,
                order_index: index as u32,
                created_at: now,
                updated_at: now,
            });
        }

        store.hosts_entries.insert(req.group_id.clone(), entries);
        if let Some(group) = store.hosts_groups.get_mut(&req.group_id) {
            group.updated_at = now;
        }
        append_log(
            &mut store,
            "info",
            "hosts",
            "hosts_entries_saved",
            &format!("group_id={}", req.group_id),
        );
        self.persist_store(&store);
        Ok(())
    }

    pub fn import_hosts_group(&self, req: ImportHostsGroupRequest) -> Result<String, EngineError> {
        let path = PathBuf::from(req.path.trim());
        let parsed = read_hosts_file(&path)
            .map_err(|err| EngineError::Storage(format!("read hosts import file failed: {err}")))?;
        let group_name = req
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("imported-hosts")
                    .to_owned()
            });

        let now = Utc::now();
        let group_id = Uuid::new_v4().to_string();
        let entries = parsed
            .into_iter()
            .enumerate()
            .map(|(index, item)| HostsEntry {
                id: Uuid::new_v4().to_string(),
                group_id: group_id.clone(),
                ip: item.ip,
                domain: item.domain,
                comment: clean_optional_text(item.comment),
                enabled: true,
                order_index: index as u32,
                created_at: now,
                updated_at: now,
            })
            .collect::<Vec<_>>();

        let mut store = self.store.write();
        store.hosts_groups.insert(
            group_id.clone(),
            HostsGroup {
                id: group_id.clone(),
                name: group_name,
                description: clean_optional_text(req.description),
                source_type: HostsGroupSourceType::FileImported,
                is_active: false,
                created_at: now,
                updated_at: now,
            },
        );
        store.hosts_entries.insert(group_id.clone(), entries);
        append_log(
            &mut store,
            "info",
            "hosts",
            "hosts_group_imported",
            &format!("group_id={group_id},path={}", path.display()),
        );
        self.persist_store(&store);
        Ok(group_id)
    }

    pub fn preview_hosts_entries_from_file(
        &self,
        path: &str,
    ) -> Result<Vec<HostsEntryInput>, EngineError> {
        let parsed = read_hosts_file(&PathBuf::from(path.trim()))
            .map_err(|err| EngineError::Storage(format!("read hosts import file failed: {err}")))?;
        Ok(parsed
            .into_iter()
            .enumerate()
            .map(|(index, item)| HostsEntryInput {
                id: None,
                ip: item.ip,
                domain: item.domain,
                comment: clean_optional_text(item.comment),
                enabled: true,
                order_index: index as u32,
            })
            .collect())
    }

    pub fn export_hosts_group(&self, req: ExportHostsGroupRequest) -> Result<(), EngineError> {
        let entries = self.list_hosts_entries(&req.group_id)?;
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
        let path = PathBuf::from(req.path.trim());
        write_hosts_file(&path, &content).map_err(|err| {
            EngineError::Storage(format!("write hosts export file failed: {err}"))
        })?;
        let mut store = self.store.write();
        append_log(
            &mut store,
            "info",
            "hosts",
            "hosts_group_exported",
            &format!("group_id={},path={}", req.group_id, path.display()),
        );
        self.persist_store(&store);
        Ok(())
    }

    pub fn activate_hosts_group(&self, group_id: &str) -> Result<(), EngineError> {
        let path = resolve_system_hosts_path();
        self.activate_hosts_group_to_path(group_id, &path)
    }

    pub fn update_app_settings(&self, settings: AppSettings) -> Result<(), EngineError> {
        let mut store = self.store.write();
        store.app_settings = settings;
        let detail = format!(
            "close_behavior={:?},show_tray_on_start={}",
            store.app_settings.close_behavior, store.app_settings.show_tray_on_start
        );
        append_log(
            &mut store,
            "info",
            "engine",
            "app_settings_updated",
            &detail,
        );
        self.persist_store(&store);
        Ok(())
    }

    pub fn update_mcp_config(&self, config: McpServerConfig) -> Result<(), EngineError> {
        let server_name = config.server_name.trim();
        if server_name.is_empty() {
            return Err(EngineError::InvalidRule(
                "mcp server_name is required".to_owned(),
            ));
        }
        if server_name
            .chars()
            .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'))
        {
            return Err(EngineError::InvalidRule(
                "mcp server_name only supports letters, numbers, - and _".to_owned(),
            ));
        }
        if config.listen_port == 0 {
            return Err(EngineError::InvalidRule(
                "mcp listen_port must be > 0".to_owned(),
            ));
        }

        let mut store = self.store.write();
        store.mcp_config = McpServerConfig {
            server_name: server_name.to_owned(),
            ..config
        };
        let detail = format!(
            "enabled={},server_name={},listen_port={}",
            store.mcp_config.enabled, store.mcp_config.server_name, store.mcp_config.listen_port
        );
        append_log(&mut store, "info", "engine", "mcp_config_updated", &detail);
        self.persist_store(&store);
        Ok(())
    }

    pub fn apply_proxy_listeners(&self) {
        self.stop_all_active_proxy_listeners();

        let (listeners, routes_map, upstreams_map, certificates_map) = {
            let store = self.store.read();
            (
                store.proxy_listeners.values().cloned().collect::<Vec<_>>(),
                store.proxy_routes.clone(),
                store.proxy_upstreams.clone(),
                store.proxy_certificates.clone(),
            )
        };

        let mut new_active = HashMap::new();
        let mut runtime_updates = Vec::new();
        let now = Utc::now();

        for listener in listeners {
            if !listener.enabled {
                runtime_updates.push(ProxyRuntimeStatusItem {
                    listener_id: listener.id.clone(),
                    state: RuntimeState::Stopped,
                    last_error: None,
                    last_apply_at: Some(now),
                });
                continue;
            }
            let listen_addr = match self.resolve_proxy_listen_addr(&listener) {
                Ok(addr) => addr,
                Err(err) => {
                    self.append_engine_log(
                        "error",
                        "proxy",
                        "proxy_listener_start_failed",
                        &format!("listener_id={},reason={err}", listener.id),
                    );
                    self.logger.log_error(
                        ErrorLogEntry::new("proxy_listen_resolve_failed", err.clone())
                            .with_rule_id(listener.id.clone()),
                    );
                    runtime_updates.push(ProxyRuntimeStatusItem {
                        listener_id: listener.id.clone(),
                        state: RuntimeState::Error,
                        last_error: Some(err),
                        last_apply_at: Some(now),
                    });
                    continue;
                }
            };

            let mut runtime_routes = Vec::new();
            let mut runtime_upstreams = HashMap::<String, Vec<ProxyUpstream>>::new();

            for route in routes_map
                .get(&listener.id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|route| route.enabled)
            {
                let mut resolved_upstreams = Vec::new();
                for mut upstream in upstreams_map
                    .get(&route.id)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|upstream| upstream.enabled)
                {
                    let resolved_candidates = match upstream.target_kind {
                        TargetKind::Static => upstream
                            .target_host
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(|value| vec![value.to_owned()])
                            .unwrap_or_default(),
                        TargetKind::Wsl | TargetKind::Hyperv => resolve_dynamic_target_candidates(
                            upstream.target_kind,
                            upstream.target_ref.as_deref().unwrap_or_default(),
                            upstream.target_host.as_deref(),
                        ),
                    };

                    if let Some(host) = resolved_candidates.first().cloned() {
                        if upstream
                            .target_host
                            .as_deref()
                            .map(str::trim)
                            .is_none_or(str::is_empty)
                        {
                            upstream.target_host = Some(host);
                        }
                        resolved_upstreams.push(upstream);
                    } else {
                        self.append_engine_log(
                                "warn",
                                "proxy",
                                "proxy_upstream_skipped",
                                &format!(
                                    "listener_id={},route_id={},upstream_id={},reason=target_not_resolved",
                                    listener.id, route.id, upstream.id
                                ),
                            );
                    }
                }

                if resolved_upstreams.is_empty() {
                    self.append_engine_log(
                        "warn",
                        "proxy",
                        "proxy_route_skipped",
                        &format!(
                            "listener_id={},route_id={},reason=no_resolved_upstream",
                            listener.id, route.id
                        ),
                    );
                    continue;
                }

                runtime_upstreams.insert(route.id.clone(), resolved_upstreams);
                runtime_routes.push(route);
            }

            if runtime_routes.is_empty() {
                let message = "no valid proxy routes are available".to_owned();
                self.append_engine_log(
                    "warn",
                    "proxy",
                    "proxy_listener_skipped",
                    &format!("listener_id={},reason=no_runtime_route", listener.id),
                );
                runtime_updates.push(ProxyRuntimeStatusItem {
                    listener_id: listener.id.clone(),
                    state: RuntimeState::Error,
                    last_error: Some(message),
                    last_apply_at: Some(now),
                });
                continue;
            }

            let traffic_recorder = self.traffic.recorder(
                TrafficEntityType::LegacyRule,
                listener.id.clone(),
                Arc::clone(&self.logger),
            );
            let spawn_result = match listener.protocol {
                ProxyProtocol::Http => spawn_http_reverse_proxy(
                    listen_addr,
                    runtime_routes,
                    runtime_upstreams,
                    self.proxy_upstream_trust_root_paths(),
                    traffic_recorder,
                    self.proxy_metrics.recorder(),
                ),
                ProxyProtocol::Https => match listener.tls_mode {
                    ProxyTlsMode::ManualCert | ProxyTlsMode::LocalCa => {
                        match listener.cert_id.as_deref() {
                            Some(cert_id) => match certificates_map.get(cert_id) {
                                Some(certificate) => spawn_https_reverse_proxy(
                                    listen_addr,
                                    &certificate.cert_path,
                                    &certificate.key_path,
                                    runtime_routes,
                                    runtime_upstreams,
                                    self.proxy_upstream_trust_root_paths(),
                                    traffic_recorder,
                                    self.proxy_metrics.recorder(),
                                ),
                                None => Err(std::io::Error::new(
                                    std::io::ErrorKind::NotFound,
                                    format!("proxy certificate not found: {cert_id}"),
                                )),
                            },
                            None => Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidInput,
                                "https listener requires cert_id when tls is enabled",
                            )),
                        }
                    }
                    ProxyTlsMode::Disabled => Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "https listener requires tls configuration",
                    )),
                },
            };
            match spawn_result {
                Ok(handle) => {
                    self.append_engine_log(
                        "info",
                        "proxy",
                        "proxy_listener_started",
                        &format!("listener_id={},listen={listen_addr}", listener.id),
                    );
                    new_active.insert(listener.id.clone(), handle);
                    runtime_updates.push(ProxyRuntimeStatusItem {
                        listener_id: listener.id.clone(),
                        state: RuntimeState::Running,
                        last_error: None,
                        last_apply_at: Some(now),
                    });
                }
                Err(err) => {
                    let msg = format!("start proxy listener failed: {err}");
                    self.append_engine_log(
                        "error",
                        "proxy",
                        "proxy_listener_start_failed",
                        &format!("listener_id={},reason={msg}", listener.id),
                    );
                    self.logger.log_error(
                        ErrorLogEntry::new("proxy_listener_start_failed", msg)
                            .with_rule_id(listener.id.clone())
                            .with_target(listen_addr.to_string()),
                    );
                    runtime_updates.push(ProxyRuntimeStatusItem {
                        listener_id: listener.id.clone(),
                        state: RuntimeState::Error,
                        last_error: Some(format!("start proxy listener failed: {err}")),
                        last_apply_at: Some(now),
                    });
                }
            }
        }

        {
            let mut active_proxy = self.active_proxy.lock();
            *active_proxy = new_active;
        }
        {
            let mut store = self.store.write();
            store.proxy_runtime.clear();
            for item in runtime_updates {
                store.proxy_runtime.insert(item.listener_id.clone(), item);
            }
        }
    }

    pub fn create_proxy_listener(
        &self,
        req: CreateProxyListenerRequest,
    ) -> Result<String, EngineError> {
        self.validate_proxy_listener(
            None,
            &req.name,
            &req.listen_host,
            req.listen_port,
            req.protocol,
            req.tls_mode,
            req.cert_id.as_deref(),
            req.bind_mode,
            req.nic_id.as_deref(),
        )?;

        let listener_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let listener = ProxyListener {
            id: listener_id.clone(),
            name: req.name.trim().to_owned(),
            listen_host: req.listen_host.trim().to_owned(),
            listen_port: req.listen_port,
            protocol: req.protocol,
            tls_mode: req.tls_mode,
            cert_id: clean_optional_text(req.cert_id),
            bind_mode: req.bind_mode,
            nic_id: clean_optional_text(req.nic_id),
            enabled: req.enabled,
            created_at: now,
            updated_at: now,
        };

        let mut store = self.store.write();
        store.proxy_listeners.insert(listener_id.clone(), listener);
        store.proxy_routes.insert(listener_id.clone(), Vec::new());
        append_log(
            &mut store,
            "info",
            "proxy",
            "proxy_listener_created",
            &format!("listener_id={listener_id}"),
        );
        self.persist_store(&store);
        drop(store);
        self.apply_proxy_listeners();
        Ok(listener_id)
    }

    pub fn create_proxy_certificate(
        &self,
        req: CreateProxyCertificateRequest,
    ) -> Result<String, EngineError> {
        let name = req.name.trim().to_owned();
        let domains = normalize_server_names(req.domains);
        let certificate_id = Uuid::new_v4().to_string();
        let (cert_path, key_path) = self.prepare_proxy_certificate_material(
            &certificate_id,
            &name,
            req.source_type,
            req.cert_path.trim(),
            req.key_path.trim(),
            &domains,
        )?;
        self.validate_proxy_certificate(
            None,
            &name,
            req.source_type,
            &cert_path,
            &key_path,
            &domains,
        )?;

        let now = Utc::now();
        let certificate = ProxyCertificate {
            id: certificate_id.clone(),
            name,
            source_type: req.source_type,
            cert_path,
            key_path,
            domains,
            created_at: now,
            updated_at: now,
        };

        let mut store = self.store.write();
        store
            .proxy_certificates
            .insert(certificate_id.clone(), certificate);
        append_log(
            &mut store,
            "info",
            "proxy",
            "proxy_certificate_created",
            &format!("certificate_id={certificate_id}"),
        );
        self.persist_store(&store);
        Ok(certificate_id)
    }

    pub fn update_proxy_certificate(
        &self,
        id: &str,
        req: UpdateProxyCertificateRequest,
    ) -> Result<(), EngineError> {
        let previous = {
            let store = self.store.read();
            store
                .proxy_certificates
                .get(id)
                .cloned()
                .ok_or_else(|| EngineError::ProxyCertificateNotFound(id.to_owned()))?
        };
        let name = req.name.trim().to_owned();
        let domains = normalize_server_names(req.domains);
        let (cert_path, key_path) = self.prepare_proxy_certificate_material(
            id,
            &name,
            req.source_type,
            req.cert_path.trim(),
            req.key_path.trim(),
            &domains,
        )?;
        self.validate_proxy_certificate(
            Some(id),
            &name,
            req.source_type,
            &cert_path,
            &key_path,
            &domains,
        )?;

        let mut store = self.store.write();
        let certificate = store
            .proxy_certificates
            .get_mut(id)
            .ok_or_else(|| EngineError::ProxyCertificateNotFound(id.to_owned()))?;
        certificate.name = name;
        certificate.source_type = req.source_type;
        certificate.cert_path = cert_path;
        certificate.key_path = key_path;
        certificate.domains = domains;
        certificate.updated_at = Utc::now();
        append_log(
            &mut store,
            "info",
            "proxy",
            "proxy_certificate_updated",
            &format!("certificate_id={id}"),
        );
        self.persist_store(&store);
        if previous.source_type == ProxyCertificateSourceType::LocalCa
            && req.source_type != ProxyCertificateSourceType::LocalCa
        {
            self.cleanup_generated_certificate_files(&previous.cert_path, &previous.key_path);
        }
        Ok(())
    }

    pub fn delete_proxy_certificate(&self, id: &str) -> Result<(), EngineError> {
        let mut store = self.store.write();
        if store
            .proxy_listeners
            .values()
            .any(|listener| listener.cert_id.as_deref() == Some(id))
        {
            return Err(EngineError::InvalidProxy(
                "proxy certificate is currently used by a listener".to_owned(),
            ));
        }
        let certificate = if let Some(certificate) = store.proxy_certificates.remove(id) {
            certificate
        } else {
            return Err(EngineError::ProxyCertificateNotFound(id.to_owned()));
        };
        append_log(
            &mut store,
            "info",
            "proxy",
            "proxy_certificate_deleted",
            &format!("certificate_id={id}"),
        );
        self.persist_store(&store);
        drop(store);
        if certificate.source_type == ProxyCertificateSourceType::LocalCa {
            self.cleanup_generated_certificate_files(&certificate.cert_path, &certificate.key_path);
        }
        Ok(())
    }

    pub fn update_proxy_listener(
        &self,
        id: &str,
        req: UpdateProxyListenerRequest,
    ) -> Result<(), EngineError> {
        self.validate_proxy_listener(
            Some(id),
            &req.name,
            &req.listen_host,
            req.listen_port,
            req.protocol,
            req.tls_mode,
            req.cert_id.as_deref(),
            req.bind_mode,
            req.nic_id.as_deref(),
        )?;

        let mut store = self.store.write();
        let listener = store
            .proxy_listeners
            .get_mut(id)
            .ok_or_else(|| EngineError::ProxyListenerNotFound(id.to_owned()))?;
        listener.name = req.name.trim().to_owned();
        listener.listen_host = req.listen_host.trim().to_owned();
        listener.listen_port = req.listen_port;
        listener.protocol = req.protocol;
        listener.tls_mode = req.tls_mode;
        listener.cert_id = clean_optional_text(req.cert_id);
        listener.bind_mode = req.bind_mode;
        listener.nic_id = clean_optional_text(req.nic_id);
        listener.enabled = req.enabled;
        listener.updated_at = Utc::now();
        append_log(
            &mut store,
            "info",
            "proxy",
            "proxy_listener_updated",
            &format!("listener_id={id}"),
        );
        self.persist_store(&store);
        drop(store);
        self.apply_proxy_listeners();
        Ok(())
    }

    pub fn delete_proxy_listener(&self, id: &str) -> Result<(), EngineError> {
        let mut store = self.store.write();
        if store.proxy_listeners.remove(id).is_none() {
            return Err(EngineError::ProxyListenerNotFound(id.to_owned()));
        }
        store.proxy_runtime.remove(id);
        let route_ids = store
            .proxy_routes
            .remove(id)
            .unwrap_or_default()
            .into_iter()
            .map(|route| route.id)
            .collect::<Vec<_>>();
        for route_id in route_ids {
            store.proxy_upstreams.remove(&route_id);
        }
        append_log(
            &mut store,
            "info",
            "proxy",
            "proxy_listener_deleted",
            &format!("listener_id={id}"),
        );
        self.persist_store(&store);
        drop(store);
        self.apply_proxy_listeners();
        Ok(())
    }

    pub fn create_proxy_route(&self, req: CreateProxyRouteRequest) -> Result<String, EngineError> {
        let server_names = normalize_server_names(req.server_names);
        self.validate_proxy_route(
            None,
            &req.listener_id,
            &server_names,
            req.path_prefix.as_deref(),
            req.is_default,
        )?;

        let route_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let route = ProxyRoute {
            id: route_id.clone(),
            listener_id: req.listener_id.clone(),
            server_names,
            path_prefix: normalize_optional_path(req.path_prefix)?,
            is_default: req.is_default,
            enabled: req.enabled,
            created_at: now,
            updated_at: now,
        };

        let mut store = self.store.write();
        store
            .proxy_routes
            .entry(req.listener_id.clone())
            .or_default()
            .push(route);
        store.proxy_upstreams.insert(route_id.clone(), Vec::new());
        append_log(
            &mut store,
            "info",
            "proxy",
            "proxy_route_created",
            &format!("listener_id={},route_id={route_id}", req.listener_id),
        );
        self.persist_store(&store);
        drop(store);
        self.apply_proxy_listeners();
        Ok(route_id)
    }

    pub fn update_proxy_route(
        &self,
        id: &str,
        req: UpdateProxyRouteRequest,
    ) -> Result<(), EngineError> {
        let route = self
            .find_proxy_route(id)
            .ok_or_else(|| EngineError::ProxyRouteNotFound(id.to_owned()))?;
        let server_names = normalize_server_names(req.server_names);
        self.validate_proxy_route(
            Some(id),
            &route.listener_id,
            &server_names,
            req.path_prefix.as_deref(),
            req.is_default,
        )?;

        let mut store = self.store.write();
        let routes = store
            .proxy_routes
            .get_mut(&route.listener_id)
            .ok_or_else(|| EngineError::ProxyListenerNotFound(route.listener_id.clone()))?;
        let route = routes
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| EngineError::ProxyRouteNotFound(id.to_owned()))?;
        route.server_names = server_names;
        route.path_prefix = normalize_optional_path(req.path_prefix)?;
        route.is_default = req.is_default;
        route.enabled = req.enabled;
        route.updated_at = Utc::now();
        append_log(
            &mut store,
            "info",
            "proxy",
            "proxy_route_updated",
            &format!("route_id={id}"),
        );
        self.persist_store(&store);
        drop(store);
        self.apply_proxy_listeners();
        Ok(())
    }

    pub fn delete_proxy_route(&self, id: &str) -> Result<(), EngineError> {
        let route = self
            .find_proxy_route(id)
            .ok_or_else(|| EngineError::ProxyRouteNotFound(id.to_owned()))?;

        let mut store = self.store.write();
        let routes = store
            .proxy_routes
            .get_mut(&route.listener_id)
            .ok_or_else(|| EngineError::ProxyListenerNotFound(route.listener_id.clone()))?;
        let original_len = routes.len();
        routes.retain(|item| item.id != id);
        if routes.len() == original_len {
            return Err(EngineError::ProxyRouteNotFound(id.to_owned()));
        }
        store.proxy_upstreams.remove(id);
        append_log(
            &mut store,
            "info",
            "proxy",
            "proxy_route_deleted",
            &format!("route_id={id}"),
        );
        self.persist_store(&store);
        drop(store);
        self.apply_proxy_listeners();
        Ok(())
    }

    pub fn create_proxy_upstream(
        &self,
        req: CreateProxyUpstreamRequest,
    ) -> Result<String, EngineError> {
        self.validate_proxy_upstream(
            None,
            &req.route_id,
            req.upstream_scheme,
            req.target_kind,
            req.target_ref.as_deref(),
            req.target_host.as_deref(),
            req.target_port,
            req.path_rewrite_from.as_deref(),
            req.path_rewrite_to.as_deref(),
        )?;

        let upstream_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let upstream = ProxyUpstream {
            id: upstream_id.clone(),
            route_id: req.route_id.clone(),
            target_kind: req.target_kind,
            target_ref: clean_optional_text(req.target_ref),
            target_host: clean_optional_text(req.target_host),
            target_port: req.target_port,
            upstream_scheme: req.upstream_scheme,
            path_rewrite_from: normalize_optional_path(req.path_rewrite_from)?,
            path_rewrite_to: normalize_optional_path(req.path_rewrite_to)?,
            enabled: req.enabled,
            created_at: now,
            updated_at: now,
        };

        let mut store = self.store.write();
        store
            .proxy_upstreams
            .entry(req.route_id.clone())
            .or_default()
            .push(upstream);
        append_log(
            &mut store,
            "info",
            "proxy",
            "proxy_upstream_created",
            &format!("route_id={},upstream_id={upstream_id}", req.route_id),
        );
        self.persist_store(&store);
        drop(store);
        self.apply_proxy_listeners();
        Ok(upstream_id)
    }

    pub fn update_proxy_upstream(
        &self,
        id: &str,
        req: UpdateProxyUpstreamRequest,
    ) -> Result<(), EngineError> {
        let upstream = self
            .find_proxy_upstream(id)
            .ok_or_else(|| EngineError::ProxyUpstreamNotFound(id.to_owned()))?;
        self.validate_proxy_upstream(
            Some(id),
            &upstream.route_id,
            req.upstream_scheme,
            req.target_kind,
            req.target_ref.as_deref(),
            req.target_host.as_deref(),
            req.target_port,
            req.path_rewrite_from.as_deref(),
            req.path_rewrite_to.as_deref(),
        )?;

        let mut store = self.store.write();
        let upstreams = store
            .proxy_upstreams
            .get_mut(&upstream.route_id)
            .ok_or_else(|| EngineError::ProxyRouteNotFound(upstream.route_id.clone()))?;
        let upstream = upstreams
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| EngineError::ProxyUpstreamNotFound(id.to_owned()))?;
        upstream.target_kind = req.target_kind;
        upstream.target_ref = clean_optional_text(req.target_ref);
        upstream.target_host = clean_optional_text(req.target_host);
        upstream.target_port = req.target_port;
        upstream.upstream_scheme = req.upstream_scheme;
        upstream.path_rewrite_from = normalize_optional_path(req.path_rewrite_from)?;
        upstream.path_rewrite_to = normalize_optional_path(req.path_rewrite_to)?;
        upstream.enabled = req.enabled;
        upstream.updated_at = Utc::now();
        append_log(
            &mut store,
            "info",
            "proxy",
            "proxy_upstream_updated",
            &format!("upstream_id={id}"),
        );
        self.persist_store(&store);
        drop(store);
        self.apply_proxy_listeners();
        Ok(())
    }

    pub fn delete_proxy_upstream(&self, id: &str) -> Result<(), EngineError> {
        let upstream = self
            .find_proxy_upstream(id)
            .ok_or_else(|| EngineError::ProxyUpstreamNotFound(id.to_owned()))?;

        let mut store = self.store.write();
        let upstreams = store
            .proxy_upstreams
            .get_mut(&upstream.route_id)
            .ok_or_else(|| EngineError::ProxyRouteNotFound(upstream.route_id.clone()))?;
        let original_len = upstreams.len();
        upstreams.retain(|item| item.id != id);
        if upstreams.len() == original_len {
            return Err(EngineError::ProxyUpstreamNotFound(id.to_owned()));
        }
        append_log(
            &mut store,
            "info",
            "proxy",
            "proxy_upstream_deleted",
            &format!("upstream_id={id}"),
        );
        self.persist_store(&store);
        drop(store);
        self.apply_proxy_listeners();
        Ok(())
    }

    pub fn create_rule(&self, req: CreateRuleRequest) -> Result<String, EngineError> {
        self.validate_new_rule(&req.rule)?;

        let rule_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let rule = ProxyRule {
            id: rule_id.clone(),
            name: req.rule.name,
            rule_type: req.rule.rule_type,
            listen_host: req.rule.listen_host,
            listen_port: req.rule.listen_port,
            target_kind: req.rule.target_kind,
            target_ref: req.rule.target_ref,
            target_host: req.rule.target_host,
            target_port: req.rule.target_port,
            bind_mode: req.rule.bind_mode,
            nic_id: req.rule.nic_id,
            enabled: req.rule.enabled,
            created_at: now,
            updated_at: now,
        };

        let firewall = req
            .firewall
            .map(|policy| FirewallPolicy {
                rule_id: rule_id.clone(),
                allow_domain: policy.allow_domain,
                allow_private: policy.allow_private,
                allow_public: policy.allow_public,
                direction: policy.direction.unwrap_or_else(|| "inbound".to_owned()),
                action: policy.action.unwrap_or_else(|| "allow".to_owned()),
            })
            .unwrap_or_else(|| FirewallPolicy::default_allow(rule_id.clone()));

        let runtime = RuntimeStatusItem {
            rule_id: rule_id.clone(),
            state: RuntimeState::Stopped,
            last_error: None,
            last_apply_at: None,
        };

        let mut store = self.store.write();
        store.rules.insert(rule_id.clone(), rule);
        store.firewalls.insert(rule_id.clone(), firewall);
        store.runtime.insert(rule_id.clone(), runtime);
        append_log(
            &mut store,
            "info",
            "engine",
            "rule_created",
            &format!("rule_id={rule_id}"),
        );
        self.persist_store(&store);
        Ok(rule_id)
    }

    pub fn migrate_rule_to_proxy(&self, rule_id: &str) -> Result<RuleMigrationRecord, EngineError> {
        if let Some(active) = self.active.lock().remove(rule_id) {
            self.stop_active_runtime(rule_id, active);
        }

        let now = Utc::now();
        let rule = {
            let store = self.store.read();
            if let Some(existing) = store.rule_migrations.get(rule_id) {
                if existing.status == RuleMigrationStatus::Migrated {
                    return Err(EngineError::InvalidRule(
                        "rule has already been migrated to proxy".to_owned(),
                    ));
                }
            }
            store
                .rules
                .get(rule_id)
                .cloned()
                .ok_or_else(|| EngineError::RuleNotFound(rule_id.to_owned()))?
        };

        match rule.rule_type {
            RuleType::TcpFwd | RuleType::HttpProxy => {}
            _ => {
                return Err(EngineError::InvalidRule(
                    "only tcp_fwd and http_proxy support proxy migration".to_owned(),
                ));
            }
        }

        self.validate_proxy_listener(
            None,
            &format!("migrated-{}", rule.name),
            &rule.listen_host,
            rule.listen_port,
            ProxyProtocol::Http,
            ProxyTlsMode::Disabled,
            None,
            rule.bind_mode,
            rule.nic_id.as_deref(),
        )?;

        let listener_id = Uuid::new_v4().to_string();
        let route_id = Uuid::new_v4().to_string();
        let upstream_id = Uuid::new_v4().to_string();
        let mut migration_detail = None;

        let listener = ProxyListener {
            id: listener_id.clone(),
            name: format!("migrated-{}", rule.name),
            listen_host: rule.listen_host.clone(),
            listen_port: rule.listen_port,
            protocol: ProxyProtocol::Http,
            tls_mode: ProxyTlsMode::Disabled,
            cert_id: None,
            bind_mode: rule.bind_mode,
            nic_id: rule.nic_id.clone(),
            enabled: rule.enabled,
            created_at: now,
            updated_at: now,
        };

        let route = ProxyRoute {
            id: route_id.clone(),
            listener_id: listener_id.clone(),
            server_names: match rule.rule_type {
                RuleType::HttpProxy => vec!["127.0.0.1".to_owned()],
                _ => Vec::new(),
            },
            path_prefix: None,
            is_default: rule.rule_type == RuleType::TcpFwd,
            enabled: rule.enabled,
            created_at: now,
            updated_at: now,
        };

        let upstream = match rule.rule_type {
            RuleType::TcpFwd => {
                let target_port = rule.target_port.ok_or_else(|| {
                    EngineError::InvalidRule("tcp_fwd migration requires target_port".to_owned())
                })?;
                ProxyUpstream {
                    id: upstream_id.clone(),
                    route_id: route_id.clone(),
                    target_kind: rule.target_kind,
                    target_ref: rule.target_ref.clone(),
                    target_host: rule.target_host.clone(),
                    target_port,
                    upstream_scheme: UpstreamScheme::Http,
                    path_rewrite_from: None,
                    path_rewrite_to: None,
                    enabled: true,
                    created_at: now,
                    updated_at: now,
                }
            }
            RuleType::HttpProxy => {
                migration_detail = Some(
                    "http_proxy migrated as proxy draft; complete upstream target before enabling traffic"
                        .to_owned(),
                );
                ProxyUpstream {
                    id: upstream_id.clone(),
                    route_id: route_id.clone(),
                    target_kind: TargetKind::Static,
                    target_ref: None,
                    target_host: Some("127.0.0.1".to_owned()),
                    target_port: 80,
                    upstream_scheme: UpstreamScheme::Http,
                    path_rewrite_from: None,
                    path_rewrite_to: None,
                    enabled: false,
                    created_at: now,
                    updated_at: now,
                }
            }
            _ => unreachable!(),
        };

        let migration = RuleMigrationRecord {
            rule_id: rule_id.to_owned(),
            status: RuleMigrationStatus::Migrated,
            original_rule_enabled: rule.enabled,
            proxy_listener_id: listener_id.clone(),
            proxy_route_id: route_id.clone(),
            proxy_upstream_id: Some(upstream_id.clone()),
            detail: migration_detail.clone(),
            migrated_at: now,
            rollbacked_at: None,
        };

        let mut store = self.store.write();
        store.proxy_listeners.insert(listener_id.clone(), listener);
        store.proxy_routes.insert(listener_id.clone(), vec![route]);
        store
            .proxy_upstreams
            .insert(route_id.clone(), vec![upstream]);
        store
            .rule_migrations
            .insert(rule_id.to_owned(), migration.clone());
        if let Some(old_rule) = store.rules.get_mut(rule_id) {
            old_rule.enabled = false;
            old_rule.updated_at = now;
        }
        if let Some(runtime) = store.runtime.get_mut(rule_id) {
            runtime.state = RuntimeState::Stopped;
            runtime.last_error = Some("migrated to proxy".to_owned());
            runtime.last_apply_at = Some(now);
        }
        append_log(
            &mut store,
            "info",
            "proxy",
            "rule_migrated_to_proxy",
            &format!(
                "rule_id={rule_id},listener_id={listener_id},route_id={route_id},upstream_id={upstream_id}"
            ),
        );
        self.persist_store(&store);
        drop(store);
        self.apply_proxy_listeners();
        Ok(migration)
    }

    pub fn rollback_rule_migration(
        &self,
        rule_id: &str,
    ) -> Result<RuleMigrationRecord, EngineError> {
        let now = Utc::now();
        let migration = {
            let store = self.store.read();
            let migration = store
                .rule_migrations
                .get(rule_id)
                .cloned()
                .ok_or_else(|| EngineError::RuleNotFound(rule_id.to_owned()))?;
            if migration.status != RuleMigrationStatus::Migrated {
                return Err(EngineError::InvalidRule(
                    "only migrated rules can be rollbacked".to_owned(),
                ));
            }
            migration
        };

        if let Some(active_proxy) = self
            .active_proxy
            .lock()
            .remove(&migration.proxy_listener_id)
        {
            active_proxy.stop_and_join();
        }

        let mut store = self.store.write();
        if !store.rules.contains_key(rule_id) {
            return Err(EngineError::RuleNotFound(rule_id.to_owned()));
        }

        store.proxy_listeners.remove(&migration.proxy_listener_id);
        store.proxy_routes.remove(&migration.proxy_listener_id);
        store.proxy_upstreams.remove(&migration.proxy_route_id);

        if let Some(rule) = store.rules.get_mut(rule_id) {
            rule.enabled = migration.original_rule_enabled;
            rule.updated_at = now;
        }
        if let Some(runtime) = store.runtime.get_mut(rule_id) {
            runtime.state = RuntimeState::Stopped;
            runtime.last_error = None;
            runtime.last_apply_at = Some(now);
        }

        let updated_migration = RuleMigrationRecord {
            status: RuleMigrationStatus::Rollbacked,
            rollbacked_at: Some(now),
            ..migration
        };
        store
            .rule_migrations
            .insert(rule_id.to_owned(), updated_migration.clone());

        append_log(
            &mut store,
            "info",
            "proxy",
            "rule_migration_rollbacked",
            &format!(
                "rule_id={rule_id},listener_id={},route_id={}",
                updated_migration.proxy_listener_id, updated_migration.proxy_route_id
            ),
        );
        self.persist_store(&store);
        drop(store);
        self.apply_proxy_listeners();
        Ok(updated_migration)
    }

    pub fn update_rule(&self, id: &str, patch: RulePatch) -> Result<(), EngineError> {
        if let Some(active) = self.active.lock().remove(id) {
            self.stop_active_runtime(id, active);
        }

        let new_listen_host = patch.listen_host.clone();
        let new_listen_port = patch.listen_port;

        if new_listen_host.is_some() || new_listen_port.is_some() {
            let store = self.store.read();
            let current_rule = store.rules.get(id);

            let check_host = new_listen_host
                .as_ref()
                .map(|h| h.as_str())
                .unwrap_or_else(|| current_rule.map(|r| r.listen_host.as_str()).unwrap_or(""));
            let check_port =
                new_listen_port.unwrap_or_else(|| current_rule.map(|r| r.listen_port).unwrap_or(0));
            let current_rule_type = current_rule
                .map(|rule| rule.rule_type)
                .unwrap_or(RuleType::TcpFwd);

            for (rid, existing_rule) in store.rules.iter() {
                if rid != id
                    && existing_rule.listen_host == check_host
                    && existing_rule.listen_port == check_port
                {
                    return Err(EngineError::InvalidRule(format!(
                        "port {} on {} is already used by rule '{}'",
                        check_port, check_host, existing_rule.name
                    )));
                }
            }
            drop(store);

            if let (Some(host), Some(port)) = (new_listen_host.as_ref(), new_listen_port) {
                if is_listen_addr_occupied(current_rule_type, host, port) {
                    return Err(EngineError::InvalidRule(format!(
                        "port {} on {} is already in use by another process",
                        port, host
                    )));
                }
            }
        }

        let mut store = self.store.write();
        let rule = store
            .rules
            .get_mut(id)
            .ok_or_else(|| EngineError::RuleNotFound(id.to_owned()))?;

        if let Some(name) = patch.name {
            rule.name = name;
        }
        if let Some(listen_host) = patch.listen_host {
            rule.listen_host = listen_host;
        }
        if let Some(listen_port) = patch.listen_port {
            rule.listen_port = listen_port;
        }
        if let Some(target_ref) = patch.target_ref {
            rule.target_ref = target_ref;
        }
        if let Some(target_host) = patch.target_host {
            rule.target_host = target_host;
        }
        if let Some(target_port) = patch.target_port {
            rule.target_port = target_port;
        }
        if let Some(bind_mode) = patch.bind_mode {
            rule.bind_mode = bind_mode;
        }
        if let Some(nic_id) = patch.nic_id {
            rule.nic_id = nic_id;
        }
        if let Some(enabled) = patch.enabled {
            rule.enabled = enabled;
        }
        rule.updated_at = Utc::now();

        if let Some(status) = store.runtime.get_mut(id) {
            status.state = RuntimeState::Stopped;
            status.last_error = None;
            status.last_apply_at = Some(Utc::now());
        }

        append_log(
            &mut store,
            "info",
            "engine",
            "rule_updated",
            &format!("rule_id={id}"),
        );
        self.persist_store(&store);
        Ok(())
    }

    pub fn delete_rule(&self, id: &str) -> Result<(), EngineError> {
        if let Some(active) = self.active.lock().remove(id) {
            self.stop_active_runtime(id, active);
        }

        let mut store = self.store.write();
        if store.rules.remove(id).is_none() {
            return Err(EngineError::RuleNotFound(id.to_owned()));
        }
        store.firewalls.remove(id);
        store.runtime.remove(id);

        append_log(
            &mut store,
            "info",
            "engine",
            "rule_deleted",
            &format!("rule_id={id}"),
        );
        self.persist_store(&store);
        Ok(())
    }

    pub fn enable_rule(&self, id: &str, enabled: bool) -> Result<(), EngineError> {
        let mut store = self.store.write();
        let rule = store
            .rules
            .get_mut(id)
            .ok_or_else(|| EngineError::RuleNotFound(id.to_owned()))?;
        rule.enabled = enabled;
        rule.updated_at = Utc::now();

        append_log(
            &mut store,
            "info",
            "engine",
            "rule_toggled",
            &format!("rule_id={id},enabled={enabled}"),
        );
        self.persist_store(&store);
        Ok(())
    }

    pub fn update_firewall_policy(
        &self,
        id: &str,
        policy: NewFirewallPolicy,
    ) -> Result<(), EngineError> {
        let mut store = self.store.write();
        if !store.rules.contains_key(id) {
            return Err(EngineError::RuleNotFound(id.to_owned()));
        }
        let item = store
            .firewalls
            .entry(id.to_owned())
            .or_insert_with(|| FirewallPolicy::default_allow(id.to_owned()));
        item.allow_domain = policy.allow_domain;
        item.allow_private = policy.allow_private;
        item.allow_public = policy.allow_public;
        item.direction = policy.direction.unwrap_or_else(|| "inbound".to_owned());
        item.action = policy.action.unwrap_or_else(|| "allow".to_owned());
        append_log(
            &mut store,
            "info",
            "engine",
            "rule_firewall_updated",
            &format!("rule_id={id}"),
        );
        self.persist_store(&store);
        Ok(())
    }

    pub fn apply_rules(&self) -> ApplyRulesResult {
        self.stop_all_active_rules();

        let rules = {
            let store = self.store.read();
            store.rules.values().cloned().collect::<Vec<_>>()
        };

        let mut seen_listens = HashMap::<SocketAddr, String>::new();
        let mut failed = Vec::new();
        let now = Utc::now();

        let mut new_active = HashMap::new();
        let mut store = self.store.write();

        for rule in rules {
            store
                .runtime
                .entry(rule.id.clone())
                .or_insert_with(|| RuntimeStatusItem {
                    rule_id: rule.id.clone(),
                    state: RuntimeState::Stopped,
                    last_error: None,
                    last_apply_at: None,
                });

            if !rule.enabled {
                set_runtime_status(&mut store, &rule.id, RuntimeState::Stopped, None, now);
                continue;
            }

            let forward_kind = match rule.rule_type {
                RuleType::TcpFwd => ForwarderKind::Tcp,
                RuleType::UdpFwd => ForwarderKind::Udp,
                RuleType::HttpProxy => ForwarderKind::HttpProxy,
                RuleType::Socks5Proxy => ForwarderKind::Socks5Proxy,
            };

            let listen_addr = match self.resolve_listen_addr(&rule) {
                Ok(addr) => addr,
                Err(err) => {
                    set_runtime_status(
                        &mut store,
                        &rule.id,
                        RuntimeState::Error,
                        Some(err.clone()),
                        now,
                    );
                    failed.push(rule.id.clone());
                    append_log(
                        &mut store,
                        "error",
                        "engine",
                        "rule_apply_failed",
                        &format!("rule_id={},reason={err}", rule.id),
                    );
                    self.logger.log_error(
                        ErrorLogEntry::new("listen_resolve_failed", err.clone())
                            .with_rule_id(rule.id.clone()),
                    );
                    continue;
                }
            };

            if let Some(existing_id) = seen_listens.get(&listen_addr) {
                let err = format!(
                    "listen conflict {} already used by rule {}",
                    listen_addr, existing_id
                );
                set_runtime_status(
                    &mut store,
                    &rule.id,
                    RuntimeState::Error,
                    Some(err.clone()),
                    now,
                );
                failed.push(rule.id.clone());
                append_log(
                    &mut store,
                    "error",
                    "engine",
                    "rule_apply_failed",
                    &format!("rule_id={},reason={err}", rule.id),
                );
                self.logger.log_error(
                    ErrorLogEntry::new("listen_conflict", err.clone())
                        .with_rule_id(rule.id.clone())
                        .with_target(listen_addr.to_string()),
                );
                continue;
            }
            seen_listens.insert(listen_addr, rule.id.clone());

            let target_addr = match rule.rule_type {
                RuleType::TcpFwd | RuleType::UdpFwd => match self.resolve_target_addr(&rule) {
                    Ok(addr) => Some(addr),
                    Err(err) => {
                        set_runtime_status(
                            &mut store,
                            &rule.id,
                            RuntimeState::Error,
                            Some(err.clone()),
                            now,
                        );
                        failed.push(rule.id.clone());
                        append_log(
                            &mut store,
                            "error",
                            "engine",
                            "rule_apply_failed",
                            &format!("rule_id={},reason={err}", rule.id),
                        );
                        self.logger.log_error(
                            ErrorLogEntry::new("target_resolve_failed", err.clone())
                                .with_rule_id(rule.id.clone()),
                        );
                        continue;
                    }
                },
                RuleType::HttpProxy | RuleType::Socks5Proxy => None,
            };

            let firewall_policy = store
                .firewalls
                .get(&rule.id)
                .cloned()
                .unwrap_or_else(|| FirewallPolicy::default_allow(rule.id.clone()));

            let traffic_recorder = self.traffic.recorder(
                TrafficEntityType::LegacyRule,
                rule.id.clone(),
                Arc::clone(&self.logger),
            );
            let forwarder =
                match spawn_forwarder(forward_kind, listen_addr, target_addr, traffic_recorder) {
                    Ok(handle) => handle,
                    Err(err) => {
                        let msg = format!("start forwarder failed: {err}");
                        set_runtime_status(
                            &mut store,
                            &rule.id,
                            RuntimeState::Error,
                            Some(msg.clone()),
                            now,
                        );
                        failed.push(rule.id.clone());
                        append_log(
                            &mut store,
                            "error",
                            "engine",
                            "rule_apply_failed",
                            &format!("rule_id={},reason={msg}", rule.id),
                        );
                        self.logger.log_error(
                            ErrorLogEntry::new("forwarder_start_failed", msg.clone())
                                .with_rule_id(rule.id.clone())
                                .with_target(listen_addr.to_string())
                                .with_detail(json!({
                                  "listen": listen_addr.to_string(),
                                  "forward_kind": format!("{forward_kind:?}")
                                })),
                        );
                        continue;
                    }
                };

            let firewall_runtime =
                match apply_firewall(self.options.firewall_mode, &rule, &firewall_policy) {
                    Ok(value) => value,
                    Err(err) => {
                        let msg = format!("apply firewall failed: {err}");
                        forwarder.stop_and_join();
                        set_runtime_status(
                            &mut store,
                            &rule.id,
                            RuntimeState::Error,
                            Some(msg.clone()),
                            now,
                        );
                        failed.push(rule.id.clone());
                        append_log(
                            &mut store,
                            "error",
                            "engine",
                            "rule_apply_failed",
                            &format!("rule_id={},reason={msg}", rule.id),
                        );
                        self.logger.log_error(
                            ErrorLogEntry::new("firewall_apply_failed", msg.clone())
                                .with_rule_id(rule.id.clone())
                                .with_target(listen_addr.to_string())
                                .with_detail(json!({
                                  "listen": listen_addr.to_string(),
                                  "target": target_addr.map(|value| value.to_string()),
                                })),
                        );
                        continue;
                    }
                };

            set_runtime_status(&mut store, &rule.id, RuntimeState::Running, None, now);
            append_log(
                &mut store,
                "info",
                "engine",
                "rule_applied",
                &format!(
                    "rule_id={},listen={},target={}",
                    rule.id,
                    listen_addr,
                    target_addr
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "-".to_owned())
                ),
            );

            new_active.insert(
                rule.id.clone(),
                ActiveRuleRuntime {
                    forwarder,
                    firewall: firewall_runtime,
                    rule_type: rule.rule_type,
                    listen_addr,
                    target_addr,
                },
            );
        }

        {
            let mut active = self.active.lock();
            *active = new_active;
        }

        let result = ApplyRulesResult {
            applied: store
                .runtime
                .values()
                .filter(|status| status.state == RuntimeState::Running)
                .count(),
            failed,
        };
        self.persist_store(&store);
        result
    }

    pub fn stop_rules(&self) -> StopRulesResult {
        self.stop_all_active_rules();

        let mut store = self.store.write();
        let now = Utc::now();
        let mut stopped = 0usize;

        for status in store.runtime.values_mut() {
            if status.state != RuntimeState::Stopped {
                stopped += 1;
            }
            status.state = RuntimeState::Stopped;
            status.last_error = None;
            status.last_apply_at = Some(now);
        }

        append_log(
            &mut store,
            "info",
            "engine",
            "all_rules_stopped",
            &format!("stopped={stopped}"),
        );
        self.persist_store(&store);
        StopRulesResult { stopped }
    }

    pub fn get_runtime_status(&self) -> Vec<RuntimeStatusItem> {
        let store = self.store.read();
        let mut items = store.runtime.values().cloned().collect::<Vec<_>>();
        items.sort_by(|a, b| a.rule_id.cmp(&b.rule_id));
        items
    }

    pub fn tail_logs(&self, cursor: usize) -> TailLogsResult {
        let store = self.store.read();
        let events = store.logs.iter().skip(cursor).cloned().collect::<Vec<_>>();
        TailLogsResult {
            next_cursor: store.logs.len(),
            events,
        }
    }

    pub fn list_traffic_monitor_entities(&self) -> Vec<TrafficMonitorEntity> {
        let store = self.store.read();
        let mut items = store
            .rules
            .values()
            .map(|rule| TrafficMonitorEntity {
                entity_type: TrafficEntityType::LegacyRule,
                entity_id: rule.id.clone(),
                label: rule.name.clone(),
                enabled: rule.enabled,
            })
            .collect::<Vec<_>>();

        for route in store.proxy_routes.values().flatten() {
            let listener_label = store
                .proxy_listeners
                .get(&route.listener_id)
                .map(build_proxy_listener_traffic_label)
                .unwrap_or_else(|| format!("listener:{}", short_id(&route.listener_id)));
            let route_label = build_proxy_route_traffic_label(route);
            for upstream in store.proxy_upstreams.get(&route.id).into_iter().flatten() {
                items.push(TrafficMonitorEntity {
                    entity_type: TrafficEntityType::ProxyUpstream,
                    entity_id: upstream.id.clone(),
                    label: format!(
                        "{listener_label} / {route_label} / {}",
                        build_proxy_upstream_traffic_label(upstream)
                    ),
                    enabled: upstream.enabled && route.enabled,
                });
            }
        }

        items.sort_by(|a, b| {
            (
                traffic_entity_type_rank(a.entity_type),
                !a.enabled,
                a.label.to_ascii_lowercase(),
            )
                .cmp(&(
                    traffic_entity_type_rank(b.entity_type),
                    !b.enabled,
                    b.label.to_ascii_lowercase(),
                ))
        });
        items
    }

    pub fn get_traffic_window_data(
        &self,
        entities: Vec<TrafficWindowQueryEntity>,
    ) -> Vec<TrafficWindowData> {
        self.traffic.get_window_data(&entities)
    }

    pub fn query_traffic_stats(&self, req: QueryTrafficStatsRequest) -> QueryTrafficStatsResult {
        self.traffic.query_stats(&req)
    }

    pub fn query_logs(&self, req: LogQueryRequest) -> LogQueryResult {
        let store = self.store.read();
        let level = req
            .level
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty());
        let module = req
            .module
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty());
        let rule_id = req
            .rule_id
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty());
        let keyword = req
            .keyword
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|v| v.to_lowercase());
        let start_time = req.start_time;
        let end_time = req.end_time;

        let mut events = store
            .logs
            .iter()
            .filter(|log| {
                if let Some(level) = level {
                    if !log.level.eq_ignore_ascii_case(level) {
                        return false;
                    }
                }
                if let Some(module) = module {
                    if !log.module.eq_ignore_ascii_case(module) {
                        return false;
                    }
                }
                if let Some(rule_id) = rule_id {
                    if !log_matches_rule_id(log, rule_id) {
                        return false;
                    }
                }
                if let Some(start) = start_time {
                    if log.time < start {
                        return false;
                    }
                }
                if let Some(end) = end_time {
                    if log.time > end {
                        return false;
                    }
                }
                if let Some(keyword) = keyword.as_deref() {
                    let hay = format!(
                        "{} {} {} {}",
                        log.level.to_lowercase(),
                        log.module.to_lowercase(),
                        log.event.to_lowercase(),
                        log.detail.to_lowercase()
                    );
                    if !hay.contains(keyword) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect::<Vec<_>>();

        let total = events.len();
        if req.newest_first.unwrap_or(false) {
            events.reverse();
        }
        if let Some(limit) = req.limit {
            events.truncate(limit);
        }

        LogQueryResult { total, events }
    }

    pub fn get_rule_log_stats(&self, req: RuleLogStatsRequest) -> Vec<RuleLogStatsItem> {
        #[derive(Default)]
        struct Acc {
            total: usize,
            errors: usize,
            last_time: Option<chrono::DateTime<Utc>>,
            last_error: Option<String>,
        }

        let since = req
            .since_minutes
            .map(|minutes| Utc::now() - chrono::Duration::minutes(i64::from(minutes)));

        let mut allow_set = HashSet::<String>::new();
        let mut map = HashMap::<String, Acc>::new();
        if let Some(rule_ids) = req.rule_ids {
            for rule_id in rule_ids {
                let clean = rule_id.trim();
                if !clean.is_empty() {
                    allow_set.insert(clean.to_owned());
                    map.entry(clean.to_owned()).or_default();
                }
            }
        }

        let store = self.store.read();
        for log in &store.logs {
            if let Some(since) = since {
                if log.time < since {
                    continue;
                }
            }

            let Some(rule_id) = extract_rule_id(log) else {
                continue;
            };
            if !allow_set.is_empty() && !allow_set.contains(&rule_id) {
                continue;
            }
            let acc = map.entry(rule_id).or_default();

            acc.total += 1;
            if log.level.eq_ignore_ascii_case("error") {
                acc.errors += 1;
                acc.last_error = Some(log.detail.clone());
            }
            acc.last_time = Some(log.time);
        }

        let mut items = map
            .into_iter()
            .map(|(rule_id, acc)| RuleLogStatsItem {
                rule_id,
                total: acc.total,
                errors: acc.errors,
                last_time: acc.last_time,
                last_error: acc.last_error,
            })
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.rule_id.cmp(&b.rule_id));
        items
    }

    pub fn reconcile_runtime_topology(&self) -> Option<ApplyRulesResult> {
        let active_snapshot = {
            let active = self.active.lock();
            if active.is_empty() {
                return None;
            }
            active
                .iter()
                .map(|(rule_id, runtime)| {
                    (
                        rule_id.clone(),
                        (runtime.rule_type, runtime.listen_addr, runtime.target_addr),
                    )
                })
                .collect::<HashMap<_, _>>()
        };

        let rules = {
            let store = self.store.read();
            store.rules.clone()
        };

        let mut changed = Vec::new();
        for (rule_id, (old_type, old_listen, old_target)) in active_snapshot {
            let Some(rule) = rules.get(&rule_id) else {
                changed.push(format!("rule_removed={rule_id}"));
                continue;
            };
            if !rule.enabled {
                changed.push(format!("rule_disabled={rule_id}"));
                continue;
            }

            let new_listen = match self.resolve_listen_addr(rule) {
                Ok(value) => value,
                Err(err) => {
                    changed.push(format!("listen_resolve_failed={rule_id}:{err}"));
                    continue;
                }
            };
            let new_target = match rule.rule_type {
                RuleType::TcpFwd | RuleType::UdpFwd => match self.resolve_target_addr(rule) {
                    Ok(value) => Some(value),
                    Err(err) => {
                        changed.push(format!("target_resolve_failed={rule_id}:{err}"));
                        continue;
                    }
                },
                RuleType::HttpProxy | RuleType::Socks5Proxy => None,
            };

            if new_listen != old_listen || new_target != old_target || old_type != rule.rule_type {
                changed.push(format!(
                    "rule_id={rule_id},listen={old_listen}->{new_listen},target={}->{},type={old_type:?}->{:?}",
                    old_target.map(|v| v.to_string()).unwrap_or_else(|| "-".to_owned()),
                    new_target.map(|v| v.to_string()).unwrap_or_else(|| "-".to_owned()),
                    rule.rule_type
                ));
            }
        }

        if changed.is_empty() {
            return None;
        }

        {
            let detail = changed.join(" | ");
            let mut store = self.store.write();
            append_log(&mut store, "warn", "engine", "topology_changed", &detail);
            self.persist_store(&store);
            self.logger.log_error(
                ErrorLogEntry::new(
                    "topology_changed",
                    "runtime topology changed, rules reapplied",
                )
                .with_detail(json!({
                  "changes": detail
                })),
            );
        }

        self.apply_proxy_listeners();
        Some(self.apply_rules())
    }

    fn resolve_listen_addr(&self, rule: &ProxyRule) -> Result<SocketAddr, String> {
        let host_ip = match rule.bind_mode {
            BindMode::AllNics => {
                let host = if rule.listen_host.trim().is_empty() {
                    "0.0.0.0"
                } else {
                    rule.listen_host.as_str()
                };
                host.parse::<IpAddr>()
                    .map_err(|err| format!("invalid listen_host {host}: {err}"))?
            }
            BindMode::SingleNic => {
                let nic_id = rule
                    .nic_id
                    .as_ref()
                    .ok_or_else(|| "single_nic mode requires nic_id".to_owned())?;
                resolve_nic_ip(nic_id)
                    .ok_or_else(|| format!("unable to resolve nic_id {nic_id} to IP"))?
            }
        };
        Ok(SocketAddr::new(host_ip, rule.listen_port))
    }

    fn resolve_target_addr(&self, rule: &ProxyRule) -> Result<SocketAddr, String> {
        let target_port = rule
            .target_port
            .ok_or_else(|| "target_port is required for forwarding rules".to_owned())?;

        let target_host = match rule.target_kind {
            TargetKind::Static => rule
                .target_host
                .clone()
                .ok_or_else(|| "target_host is required for static target".to_owned())?,
            TargetKind::Wsl | TargetKind::Hyperv => {
                if let Some(target_ref) = rule.target_ref.as_ref().map(|value| value.trim()) {
                    if !target_ref.is_empty() {
                        if let Some(host) =
                            resolve_dynamic_target_host(rule.target_kind, target_ref)
                        {
                            host
                        } else {
                            return Err(format!(
                                "unable to resolve {:?} target_ref {} to IP",
                                rule.target_kind, target_ref
                            ));
                        }
                    } else {
                        rule.target_host.clone().ok_or_else(|| {
                            "target_ref is empty and target_host fallback is missing".to_owned()
                        })?
                    }
                } else {
                    rule.target_host.clone().ok_or_else(|| {
                        format!(
                            "target_ref is required for {:?} target, or provide target_host fallback",
                            rule.target_kind
                        )
                    })?
                }
            }
        };

        (target_host.as_str(), target_port)
            .to_socket_addrs()
            .map_err(|err| format!("resolve target address failed: {err}"))?
            .next()
            .ok_or_else(|| "resolve target address produced no result".to_owned())
    }

    fn resolve_proxy_listen_addr(&self, listener: &ProxyListener) -> Result<SocketAddr, String> {
        let host = match listener.bind_mode {
            BindMode::AllNics => listener.listen_host.clone(),
            BindMode::SingleNic => {
                let nic_id = listener.nic_id.as_deref().unwrap_or("").trim();
                if nic_id.is_empty() {
                    return Err("single_nic mode requires nic_id".to_owned());
                }
                resolve_nic_ip(nic_id)
                    .map(|ip| ip.to_string())
                    .ok_or_else(|| format!("resolve nic ip failed for {nic_id}"))?
            }
        };

        let addr = format!("{}:{}", host.trim(), listener.listen_port);
        addr.parse::<SocketAddr>()
            .map_err(|err| format!("parse listen address failed: {err}"))
    }

    fn stop_all_active_rules(&self) {
        let old_active = {
            let mut active = self.active.lock();
            std::mem::take(&mut *active)
        };
        for (rule_id, runtime) in old_active {
            self.stop_active_runtime(&rule_id, runtime);
        }
    }

    fn stop_all_active_proxy_listeners(&self) {
        let old_active = {
            let mut active = self.active_proxy.lock();
            std::mem::take(&mut *active)
        };
        let now = Utc::now();
        let mut store = self.store.write();
        for (listener_id, runtime) in old_active {
            runtime.stop_and_join();
            self.traffic
                .flush_entity(TrafficEntityType::LegacyRule, &listener_id);
            store.proxy_runtime.insert(
                listener_id.clone(),
                ProxyRuntimeStatusItem {
                    listener_id,
                    state: RuntimeState::Stopped,
                    last_error: None,
                    last_apply_at: Some(now),
                },
            );
        }
        self.traffic
            .flush_entities_of_type(TrafficEntityType::ProxyUpstream);
    }

    fn stop_active_runtime(&self, rule_id: &str, runtime: ActiveRuleRuntime) {
        runtime.forwarder.stop_and_join();
        self.traffic
            .flush_entity(TrafficEntityType::LegacyRule, rule_id);
        if let Err(err) = cleanup_firewall(self.options.firewall_mode, &runtime.firewall.names) {
            let mut store = self.store.write();
            append_log(
                &mut store,
                "warn",
                "engine",
                "firewall_cleanup_failed",
                &format!("rule_id={rule_id},reason={err}"),
            );
            self.logger.log_error(
                ErrorLogEntry::new("firewall_cleanup_failed", err.to_string())
                    .with_rule_id(rule_id.to_owned())
                    .with_detail(json!({
                      "rule_names": runtime.firewall.names
                    })),
            );
            self.persist_store(&store);
        }
    }

    fn persist_store(&self, store: &EngineStore) {
        let Some(sqlite) = &self.sqlite else {
            return;
        };
        let snapshot = snapshot_from_store(store);
        if let Err(err) = sqlite.save_snapshot(&snapshot) {
            warn!("persist snapshot failed: {err}");
        }
    }

    fn append_engine_log(&self, level: &str, module: &str, event: &str, detail: &str) {
        let mut store = self.store.write();
        append_log(&mut store, level, module, event, detail);
        self.persist_store(&store);
    }

    fn validate_new_rule(&self, rule: &NewProxyRule) -> Result<(), EngineError> {
        if rule.name.trim().is_empty() {
            return Err(EngineError::InvalidRule("name is required".to_owned()));
        }
        if rule.listen_host.trim().is_empty() {
            return Err(EngineError::InvalidRule(
                "listen_host is required".to_owned(),
            ));
        }
        if rule.listen_port == 0 {
            return Err(EngineError::InvalidRule(
                "listen_port must be > 0".to_owned(),
            ));
        }
        if rule.bind_mode == BindMode::SingleNic && rule.nic_id.as_deref().unwrap_or("").is_empty()
        {
            return Err(EngineError::InvalidRule(
                "single_nic mode requires nic_id".to_owned(),
            ));
        }

        let store = self.store.read();
        for existing_rule in store.rules.values() {
            if existing_rule.listen_host == rule.listen_host
                && existing_rule.listen_port == rule.listen_port
            {
                return Err(EngineError::InvalidRule(format!(
                    "port {} on {} is already used by rule '{}'",
                    rule.listen_port, rule.listen_host, existing_rule.name
                )));
            }
        }
        drop(store);

        if is_listen_addr_occupied(rule.rule_type, &rule.listen_host, rule.listen_port) {
            return Err(EngineError::InvalidRule(format!(
                "port {} on {} is already in use by another process",
                rule.listen_port, rule.listen_host
            )));
        }

        if rule.rule_type == RuleType::TcpFwd || rule.rule_type == RuleType::UdpFwd {
            match rule.target_kind {
                TargetKind::Static => {
                    if rule.target_host.as_deref().unwrap_or("").trim().is_empty() {
                        return Err(EngineError::InvalidRule(
                            "target_host is required for static forwarding target".to_owned(),
                        ));
                    }
                }
                TargetKind::Wsl | TargetKind::Hyperv => {
                    if rule.target_ref.as_deref().unwrap_or("").trim().is_empty()
                        && rule.target_host.as_deref().unwrap_or("").trim().is_empty()
                    {
                        return Err(EngineError::InvalidRule(format!(
                            "target_ref is required for {:?} forwarding target",
                            rule.target_kind
                        )));
                    }
                }
            }
            if rule.target_port.is_none() {
                return Err(EngineError::InvalidRule(
                    "target_port is required for tcp/udp forwarding".to_owned(),
                ));
            }
        }
        if rule.rule_type == RuleType::HttpProxy || rule.rule_type == RuleType::Socks5Proxy {
            if rule.target_kind != TargetKind::Static {
                return Err(EngineError::InvalidRule(
                    "http/socks5 proxy requires target_kind=static".to_owned(),
                ));
            }
        }
        Ok(())
    }

    fn validate_proxy_listener(
        &self,
        current_id: Option<&str>,
        name: &str,
        listen_host: &str,
        listen_port: u16,
        protocol: ProxyProtocol,
        tls_mode: ProxyTlsMode,
        cert_id: Option<&str>,
        bind_mode: BindMode,
        nic_id: Option<&str>,
    ) -> Result<(), EngineError> {
        if name.trim().is_empty() {
            return Err(EngineError::InvalidProxy(
                "proxy listener name is required".to_owned(),
            ));
        }
        if listen_host.trim().is_empty() {
            return Err(EngineError::InvalidProxy(
                "proxy listen_host is required".to_owned(),
            ));
        }
        if listen_port == 0 {
            return Err(EngineError::InvalidProxy(
                "proxy listen_port must be > 0".to_owned(),
            ));
        }
        if bind_mode == BindMode::SingleNic && nic_id.unwrap_or("").trim().is_empty() {
            return Err(EngineError::InvalidProxy(
                "single_nic mode requires nic_id".to_owned(),
            ));
        }
        match protocol {
            ProxyProtocol::Http if tls_mode != ProxyTlsMode::Disabled => {
                return Err(EngineError::InvalidProxy(
                    "http listener must use tls_mode=disabled".to_owned(),
                ));
            }
            ProxyProtocol::Https if tls_mode == ProxyTlsMode::Disabled => {
                return Err(EngineError::InvalidProxy(
                    "https listener requires tls configuration".to_owned(),
                ));
            }
            _ => {}
        }
        if matches!(tls_mode, ProxyTlsMode::ManualCert | ProxyTlsMode::LocalCa)
            && cert_id.unwrap_or("").trim().is_empty()
        {
            return Err(EngineError::InvalidProxy(
                "tls-enabled https listener requires cert_id".to_owned(),
            ));
        }

        let store = self.store.read();
        if matches!(tls_mode, ProxyTlsMode::ManualCert | ProxyTlsMode::LocalCa) {
            let cert_id = cert_id.unwrap_or("").trim();
            let certificate = store
                .proxy_certificates
                .get(cert_id)
                .ok_or_else(|| EngineError::ProxyCertificateNotFound(cert_id.to_owned()))?;
            match (tls_mode, certificate.source_type) {
                (ProxyTlsMode::ManualCert, ProxyCertificateSourceType::ManualUpload)
                | (ProxyTlsMode::LocalCa, ProxyCertificateSourceType::LocalCa) => {}
                (ProxyTlsMode::ManualCert, _) => {
                    return Err(EngineError::InvalidProxy(
                        "manual_cert tls mode requires a manual_upload certificate".to_owned(),
                    ));
                }
                (ProxyTlsMode::LocalCa, _) => {
                    return Err(EngineError::InvalidProxy(
                        "local_ca tls mode requires a local_ca certificate".to_owned(),
                    ));
                }
                (ProxyTlsMode::Disabled, _) => {}
            }
        }
        for listener in store.proxy_listeners.values() {
            if Some(listener.id.as_str()) != current_id
                && listener.listen_host == listen_host.trim()
                && listener.listen_port == listen_port
            {
                return Err(EngineError::InvalidProxy(format!(
                    "proxy listener {}:{} is already used by '{}'",
                    listen_host.trim(),
                    listen_port,
                    listener.name
                )));
            }
        }
        Ok(())
    }

    fn validate_proxy_certificate(
        &self,
        current_id: Option<&str>,
        name: &str,
        _source_type: ProxyCertificateSourceType,
        cert_path: &str,
        key_path: &str,
        domains: &[String],
    ) -> Result<(), EngineError> {
        if name.is_empty() {
            return Err(EngineError::InvalidProxy(
                "proxy certificate name is required".to_owned(),
            ));
        }
        if cert_path.is_empty() {
            return Err(EngineError::InvalidProxy(
                "proxy certificate cert_path is required".to_owned(),
            ));
        }
        if key_path.is_empty() {
            return Err(EngineError::InvalidProxy(
                "proxy certificate key_path is required".to_owned(),
            ));
        }
        if domains.is_empty() {
            return Err(EngineError::InvalidProxy(
                "proxy certificate requires at least one domain".to_owned(),
            ));
        }
        if !Path::new(cert_path).exists() {
            return Err(EngineError::InvalidProxy(format!(
                "proxy certificate file not found: {cert_path}"
            )));
        }
        if !Path::new(key_path).exists() {
            return Err(EngineError::InvalidProxy(format!(
                "proxy key file not found: {key_path}"
            )));
        }

        let store = self.store.read();
        for certificate in store.proxy_certificates.values() {
            if Some(certificate.id.as_str()) == current_id {
                continue;
            }
            if certificate.name.eq_ignore_ascii_case(name) {
                return Err(EngineError::InvalidProxy(format!(
                    "proxy certificate '{}' already exists",
                    name
                )));
            }
        }

        Ok(())
    }

    fn prepare_proxy_certificate_material(
        &self,
        certificate_id: &str,
        name: &str,
        source_type: ProxyCertificateSourceType,
        cert_path: &str,
        key_path: &str,
        domains: &[String],
    ) -> Result<(String, String), EngineError> {
        match source_type {
            ProxyCertificateSourceType::ManualUpload => {
                Ok((cert_path.to_owned(), key_path.to_owned()))
            }
            ProxyCertificateSourceType::LocalCa => {
                self.generate_local_ca_certificate_files(certificate_id, name, domains)
            }
        }
    }

    fn generate_local_ca_certificate_files(
        &self,
        certificate_id: &str,
        name: &str,
        domains: &[String],
    ) -> Result<(String, String), EngineError> {
        let root_dir = self.proxy_certificate_assets_dir().join("local-ca");
        let certs_dir = root_dir.join("certs");
        fs::create_dir_all(&certs_dir).map_err(|err| EngineError::Storage(err.to_string()))?;

        let root_cert_path = root_dir.join("root-ca.pem");
        let root_key_path = root_dir.join("root-ca.key");
        let (ca_cert, ca_key_pair) = ensure_local_ca_root(&root_cert_path, &root_key_path)?;

        let leaf_key = KeyPair::generate()
            .map_err(|err| EngineError::Storage(format!("failed to generate leaf key: {err}")))?;
        let mut leaf_params = CertificateParams::new(domains.to_vec()).map_err(|err| {
            EngineError::InvalidProxy(format!("invalid certificate domains: {err}"))
        })?;
        let mut distinguished_name = DistinguishedName::new();
        distinguished_name.push(DnType::CommonName, name);
        leaf_params.distinguished_name = distinguished_name;
        let leaf_cert = leaf_params
            .signed_by(&leaf_key, &ca_cert, &ca_key_pair)
            .map_err(|err| {
                EngineError::Storage(format!("failed to sign local ca certificate: {err}"))
            })?;

        let cert_path = certs_dir.join(format!("{certificate_id}.pem"));
        let key_path = certs_dir.join(format!("{certificate_id}.key"));
        fs::write(&cert_path, leaf_cert.pem()).map_err(|err| {
            EngineError::Storage(format!("failed to write certificate file: {err}"))
        })?;
        fs::write(&key_path, leaf_key.serialize_pem())
            .map_err(|err| EngineError::Storage(format!("failed to write key file: {err}")))?;

        Ok((
            cert_path.display().to_string(),
            key_path.display().to_string(),
        ))
    }

    fn proxy_certificate_assets_dir(&self) -> PathBuf {
        if let Some(sqlite) = &self.sqlite {
            return sqlite
                .path()
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("certificates");
        }
        std::env::temp_dir().join("wsl-bridge-dev-certificates")
    }

    fn proxy_upstream_trust_root_paths(&self) -> Vec<PathBuf> {
        vec![self
            .proxy_certificate_assets_dir()
            .join("local-ca")
            .join("root-ca.pem")]
    }

    fn cleanup_generated_certificate_files(&self, cert_path: &str, key_path: &str) {
        let _ = fs::remove_file(cert_path);
        let _ = fs::remove_file(key_path);
    }

    fn validate_proxy_route(
        &self,
        current_id: Option<&str>,
        listener_id: &str,
        server_names: &[String],
        path_prefix: Option<&str>,
        is_default: bool,
    ) -> Result<(), EngineError> {
        if !self.store.read().proxy_listeners.contains_key(listener_id) {
            return Err(EngineError::ProxyListenerNotFound(listener_id.to_owned()));
        }
        if !is_default && server_names.is_empty() {
            return Err(EngineError::InvalidProxy(
                "proxy route requires at least one server_name unless default route is enabled"
                    .to_owned(),
            ));
        }
        if let Some(prefix) = path_prefix {
            validate_path_like(prefix, "proxy route path_prefix")?;
        }

        let store = self.store.read();
        if is_default {
            let has_other_default = store
                .proxy_routes
                .get(listener_id)
                .into_iter()
                .flatten()
                .any(|route| route.is_default && Some(route.id.as_str()) != current_id);
            if has_other_default {
                return Err(EngineError::InvalidProxy(
                    "only one default route is allowed per listener".to_owned(),
                ));
            }
        }
        Ok(())
    }

    fn validate_proxy_upstream(
        &self,
        _current_id: Option<&str>,
        route_id: &str,
        upstream_scheme: UpstreamScheme,
        target_kind: TargetKind,
        target_ref: Option<&str>,
        target_host: Option<&str>,
        target_port: u16,
        path_rewrite_from: Option<&str>,
        path_rewrite_to: Option<&str>,
    ) -> Result<(), EngineError> {
        let route = self
            .find_proxy_route(route_id)
            .ok_or_else(|| EngineError::ProxyRouteNotFound(route_id.to_owned()))?;
        let listener = {
            let store = self.store.read();
            store
                .proxy_listeners
                .get(&route.listener_id)
                .cloned()
                .ok_or_else(|| EngineError::ProxyListenerNotFound(route.listener_id.clone()))?
        };
        if target_port == 0 {
            return Err(EngineError::InvalidProxy(
                "proxy upstream target_port must be > 0".to_owned(),
            ));
        }
        match upstream_scheme {
            UpstreamScheme::Grpc if listener.protocol != ProxyProtocol::Http => {
                return Err(EngineError::InvalidProxy(
                    "grpc upstream requires an http listener".to_owned(),
                ));
            }
            UpstreamScheme::Grpcs if listener.protocol != ProxyProtocol::Https => {
                return Err(EngineError::InvalidProxy(
                    "grpcs upstream requires an https listener".to_owned(),
                ));
            }
            _ => {}
        }
        if matches!(
            upstream_scheme,
            UpstreamScheme::Grpc | UpstreamScheme::Grpcs
        ) {
            if !route.is_default {
                return Err(EngineError::InvalidProxy(format!(
                    "{} upstream currently requires a default route",
                    match upstream_scheme {
                        UpstreamScheme::Grpc => "grpc",
                        UpstreamScheme::Grpcs => "grpcs",
                        _ => unreachable!(),
                    }
                )));
            }
            if path_rewrite_from.is_some() || path_rewrite_to.is_some() {
                return Err(EngineError::InvalidProxy(format!(
                    "{} upstream does not support path rewrite yet",
                    match upstream_scheme {
                        UpstreamScheme::Grpc => "grpc",
                        UpstreamScheme::Grpcs => "grpcs",
                        _ => unreachable!(),
                    }
                )));
            }
        }
        match target_kind {
            TargetKind::Static => {
                if target_host.unwrap_or("").trim().is_empty() {
                    return Err(EngineError::InvalidProxy(
                        "static upstream requires target_host".to_owned(),
                    ));
                }
            }
            TargetKind::Wsl | TargetKind::Hyperv => {
                if target_ref.unwrap_or("").trim().is_empty()
                    && target_host.unwrap_or("").trim().is_empty()
                {
                    return Err(EngineError::InvalidProxy(
                        "dynamic upstream requires target_ref or fallback target_host".to_owned(),
                    ));
                }
            }
        }
        if let Some(path) = path_rewrite_from {
            validate_path_like(path, "proxy upstream path_rewrite_from")?;
        }
        if let Some(path) = path_rewrite_to {
            validate_path_like(path, "proxy upstream path_rewrite_to")?;
        }
        Ok(())
    }

    fn find_proxy_route(&self, route_id: &str) -> Option<ProxyRoute> {
        let store = self.store.read();
        store
            .proxy_routes
            .values()
            .flat_map(|routes| routes.iter())
            .find(|route| route.id == route_id)
            .cloned()
    }

    fn find_proxy_upstream(&self, upstream_id: &str) -> Option<ProxyUpstream> {
        let store = self.store.read();
        store
            .proxy_upstreams
            .values()
            .flat_map(|upstreams| upstreams.iter())
            .find(|upstream| upstream.id == upstream_id)
            .cloned()
    }
}

impl RuleEngine {
    fn bootstrap_default_hosts_group_from_path(
        &self,
        path: &Path,
    ) -> Result<HostsGroup, EngineError> {
        {
            let store = self.store.read();
            if let Some(group) = store
                .hosts_groups
                .values()
                .filter(|item| matches!(item.source_type, HostsGroupSourceType::SystemImported))
                .min_by(|left, right| {
                    left.created_at
                        .cmp(&right.created_at)
                        .then(left.id.cmp(&right.id))
                })
            {
                return Ok(group.clone());
            }
            if let Some(group) = store
                .hosts_groups
                .values()
                .find(|item| item.name == "default")
            {
                return Ok(group.clone());
            }
        }

        let parsed = read_hosts_file(path)
            .map_err(|err| EngineError::Storage(format!("read system hosts failed: {err}")))?;
        let now = Utc::now();
        let group_id = Uuid::new_v4().to_string();
        let group = HostsGroup {
            id: group_id.clone(),
            name: "default".to_owned(),
            description: Some("Imported from system hosts".to_owned()),
            source_type: HostsGroupSourceType::SystemImported,
            is_active: false,
            created_at: now,
            updated_at: now,
        };
        let entries = parsed
            .into_iter()
            .enumerate()
            .map(|(index, item)| HostsEntry {
                id: Uuid::new_v4().to_string(),
                group_id: group_id.clone(),
                ip: item.ip,
                domain: item.domain,
                comment: clean_optional_text(item.comment),
                enabled: true,
                order_index: index as u32,
                created_at: now,
                updated_at: now,
            })
            .collect::<Vec<_>>();

        let mut store = self.store.write();
        store.hosts_groups.insert(group_id.clone(), group.clone());
        store.hosts_entries.insert(group_id.clone(), entries);
        append_log(
            &mut store,
            "info",
            "hosts",
            "hosts_default_bootstrapped",
            &format!("group_id={group_id},path={}", path.display()),
        );
        self.persist_store(&store);
        Ok(group)
    }

    fn activate_hosts_group_to_path(&self, group_id: &str, path: &Path) -> Result<(), EngineError> {
        let entries = self.list_hosts_entries(group_id)?;
        let content = render_hosts_text(
            &entries
                .iter()
                .map(|entry| HostsEntryInput {
                    id: Some(entry.id.clone()),
                    ip: entry.ip.clone(),
                    domain: entry.domain.clone(),
                    comment: entry.comment.clone(),
                    enabled: entry.enabled,
                    order_index: entry.order_index,
                })
                .collect::<Vec<_>>(),
        );
        write_hosts_file(path, &content)
            .map_err(|err| EngineError::Storage(format!("write system hosts failed: {err}")))?;

        let mut store = self.store.write();
        if !store.hosts_groups.contains_key(group_id) {
            return Err(EngineError::HostsGroupNotFound(group_id.to_owned()));
        }
        for group in store.hosts_groups.values_mut() {
            group.is_active = group.id == group_id;
            if group.is_active {
                group.updated_at = Utc::now();
            }
        }
        append_log(
            &mut store,
            "info",
            "hosts",
            "hosts_group_activated",
            &format!("group_id={group_id},path={}", path.display()),
        );
        self.persist_store(&store);
        Ok(())
    }
}

fn ensure_local_ca_root(
    cert_path: &Path,
    key_path: &Path,
) -> Result<(rcgen::Certificate, KeyPair), EngineError> {
    if cert_path.exists() && key_path.exists() {
        let key_pem = fs::read_to_string(key_path)
            .map_err(|err| EngineError::Storage(format!("read local ca key failed: {err}")))?;
        let key_pair = KeyPair::from_pem(&key_pem)
            .map_err(|err| EngineError::Storage(format!("parse local ca key failed: {err}")))?;
        let params = build_local_ca_params()?;
        let cert = params
            .self_signed(&key_pair)
            .map_err(|err| EngineError::Storage(format!("rebuild local ca cert failed: {err}")))?;
        fs::write(cert_path, cert.pem())
            .map_err(|err| EngineError::Storage(format!("rewrite local ca cert failed: {err}")))?;
        return Ok((cert, key_pair));
    }

    let params = build_local_ca_params()?;
    let key_pair = KeyPair::generate()
        .map_err(|err| EngineError::Storage(format!("generate local ca key failed: {err}")))?;
    let cert = params
        .self_signed(&key_pair)
        .map_err(|err| EngineError::Storage(format!("generate local ca cert failed: {err}")))?;
    fs::write(cert_path, cert.pem())
        .map_err(|err| EngineError::Storage(format!("write local ca cert failed: {err}")))?;
    fs::write(key_path, key_pair.serialize_pem())
        .map_err(|err| EngineError::Storage(format!("write local ca key failed: {err}")))?;
    Ok((cert, key_pair))
}

fn build_local_ca_params() -> Result<CertificateParams, EngineError> {
    let mut params = CertificateParams::new(vec!["wsl-bridge.local-ca".to_owned()])
        .map_err(|err| EngineError::Storage(format!("build local ca params failed: {err}")))?;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(DnType::CommonName, "wsl-bridge Local CA");
    params.distinguished_name = distinguished_name;
    Ok(params)
}

fn traffic_entity_type_rank(value: TrafficEntityType) -> u8 {
    match value {
        TrafficEntityType::LegacyRule => 0,
        TrafficEntityType::ProxyUpstream => 1,
    }
}

fn build_proxy_listener_traffic_label(listener: &ProxyListener) -> String {
    compact_traffic_label_segment(&listener.name, 7, || {
        format!("lst:{}", short_id(&listener.id))
    })
}

fn build_proxy_route_traffic_label(route: &ProxyRoute) -> String {
    let server_name = route
        .server_names
        .first()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or(if route.is_default { "default" } else { "" });
    compact_traffic_label_segment(server_name, 7, || format!("route:{}", short_id(&route.id)))
}

fn build_proxy_upstream_traffic_label(upstream: &ProxyUpstream) -> String {
    if let Some(target_ref) = upstream
        .target_ref
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return compact_traffic_label_segment(target_ref, 7, || {
            format!("up:{}", short_id(&upstream.id))
        });
    }
    if let Some(target_host) = upstream
        .target_host
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return compact_traffic_label_segment(target_host, 7, || {
            format!("up:{}", short_id(&upstream.id))
        });
    }
    format!("up:{}", short_id(&upstream.id))
}

fn compact_traffic_label_segment<F>(value: &str, max_chars: usize, fallback: F) -> String
where
    F: FnOnce() -> String,
{
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return fallback();
    }
    trimmed.chars().take(max_chars).collect()
}

fn short_id(value: &str) -> &str {
    value.get(..8).unwrap_or(value)
}

fn append_log(store: &mut EngineStore, level: &str, module: &str, event: &str, detail: &str) {
    store.log_seq += 1;
    store.logs.push(AuditLog {
        id: store.log_seq,
        time: Utc::now(),
        level: level.to_owned(),
        module: module.to_owned(),
        event: event.to_owned(),
        detail: detail.to_owned(),
    });
}

fn snapshot_from_store(store: &EngineStore) -> Snapshot {
    Snapshot {
        rules: store.rules.clone(),
        firewalls: store.firewalls.clone(),
        runtime: store.runtime.clone(),
        hosts_groups: store.hosts_groups.clone(),
        hosts_entries: store.hosts_entries.clone(),
        proxy_listeners: store.proxy_listeners.clone(),
        proxy_routes: store.proxy_routes.clone(),
        proxy_upstreams: store.proxy_upstreams.clone(),
        proxy_certificates: store.proxy_certificates.clone(),
        rule_migrations: store.rule_migrations.clone(),
        logs: store.logs.clone(),
        log_seq: store.log_seq,
        mcp_config: store.mcp_config.clone(),
        app_settings: store.app_settings.clone(),
    }
}

fn set_runtime_status(
    store: &mut EngineStore,
    rule_id: &str,
    state: RuntimeState,
    last_error: Option<String>,
    at: chrono::DateTime<Utc>,
) {
    let item = store
        .runtime
        .entry(rule_id.to_owned())
        .or_insert_with(|| RuntimeStatusItem {
            rule_id: rule_id.to_owned(),
            state: RuntimeState::Stopped,
            last_error: None,
            last_apply_at: None,
        });
    item.state = state;
    item.last_error = last_error;
    item.last_apply_at = Some(at);
}

fn extract_rule_id(log: &AuditLog) -> Option<String> {
    extract_rule_id_from_text(&log.detail)
}

fn log_matches_rule_id(log: &AuditLog, rule_id: &str) -> bool {
    extract_rule_id(log).as_deref() == Some(rule_id)
        || log.detail.contains(rule_id)
        || log.event.contains(rule_id)
        || log.module.contains(rule_id)
}

fn extract_rule_id_from_text(text: &str) -> Option<String> {
    let marker = "rule_id=";
    let start = text.find(marker)?;
    let value = &text[start + marker.len()..];
    let end = value.find([',', ' ', '|']).unwrap_or(value.len());
    let rule_id = value[..end].trim();
    if rule_id.is_empty() {
        None
    } else {
        Some(rule_id.to_owned())
    }
}

fn clean_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty())
}

fn normalize_server_names(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .map(|item| item.trim().to_ascii_lowercase())
        .filter(|item| !item.is_empty())
        .filter(|item| seen.insert(item.clone()))
        .collect()
}

fn normalize_optional_path(value: Option<String>) -> Result<Option<String>, EngineError> {
    let value = clean_optional_text(value);
    if let Some(path) = value.as_deref() {
        validate_path_like(path, "path")?;
    }
    Ok(value)
}

fn validate_path_like(value: &str, label: &str) -> Result<(), EngineError> {
    if !value.starts_with('/') {
        return Err(EngineError::InvalidProxy(format!(
            "{label} must start with '/'"
        )));
    }
    Ok(())
}

fn is_listen_addr_occupied(rule_type: RuleType, host: &str, port: u16) -> bool {
    let addr_str = format!("{host}:{port}");
    let Ok(addr) = addr_str.parse::<SocketAddr>() else {
        return false;
    };

    match rule_type {
        RuleType::UdpFwd => UdpSocket::bind(addr).is_err(),
        RuleType::TcpFwd | RuleType::HttpProxy | RuleType::Socks5Proxy => {
            TcpListener::bind(addr).is_err()
        }
    }
}

fn validate_hosts_entry_input(entry: &HostsEntryInput) -> Result<(), EngineError> {
    if entry.ip.trim().parse::<IpAddr>().is_err() {
        return Err(EngineError::InvalidHosts(format!(
            "invalid hosts ip: {}",
            entry.ip
        )));
    }
    if entry.domain.trim().is_empty() {
        return Err(EngineError::InvalidHosts(
            "hosts domain is required".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::io::{BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream, UdpSocket};
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use std::thread;
    use std::time::Duration;
    use std::time::{SystemTime, UNIX_EPOCH};

    use rcgen::{
        generate_simple_self_signed, CertificateParams, CertifiedKey, DistinguishedName, DnType,
        KeyPair,
    };
    use rustls::pki_types::{PrivateKeyDer, ServerName};
    use rustls::{
        ClientConfig, ClientConnection, RootCertStore, ServerConfig, ServerConnection, StreamOwned,
    };
    use wsl_bridge_shared::{
        AppSettings, BindMode, CloseBehavior, CopyHostsGroupRequest, CreateHostsGroupRequest,
        CreateProxyCertificateRequest, CreateProxyListenerRequest, CreateProxyRouteRequest,
        CreateProxyUpstreamRequest, CreateRuleRequest, ExportHostsGroupRequest, HostsEntryInput,
        ImportHostsGroupRequest, LogQueryRequest, NewProxyRule, ProxyCertificateSourceType,
        ProxyProtocol, ProxyTlsMode, QueryTrafficStatsRequest, RuleLogStatsRequest,
        RuleMigrationStatus, RulePatch, RuleType, RuntimeState, SaveHostsEntriesRequest,
        TargetKind, TrafficEntityType, TrafficWindowQueryEntity, UpdateHostsGroupRequest,
        UpdateProxyRouteRequest,
        UpstreamScheme,
    };

    use crate::forwarder::HTTP2_PRIOR_KNOWLEDGE_PREFACE;

    use super::{EngineError, EngineOptions, RuleEngine};

    fn test_rule(name: &str, port: u16) -> CreateRuleRequest {
        CreateRuleRequest {
            rule: NewProxyRule {
                name: name.to_owned(),
                rule_type: RuleType::TcpFwd,
                listen_host: "127.0.0.1".to_owned(),
                listen_port: port,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port: Some(80),
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            },
            firewall: None,
        }
    }

    fn free_tcp_port() -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind ephemeral");
        listener.local_addr().expect("local addr").port()
    }

    fn free_udp_port() -> u16 {
        let socket = UdpSocket::bind(("127.0.0.1", 0)).expect("bind ephemeral");
        socket.local_addr().expect("local addr").port()
    }

    fn bind_test_tcp_listener() -> (TcpListener, u16) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test listener");
        let port = listener.local_addr().expect("listener local addr").port();
        (listener, port)
    }

    fn write_temp_fixture(prefix: &str, extension: &str, content: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("duration")
            .as_nanos();
        let path = env::temp_dir().join(format!("{prefix}-{now}.{extension}"));
        fs::write(&path, content).expect("write temp fixture");
        path
    }

    fn proxy_test_local_ca_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn proxy_test_local_ca_root_dir() -> PathBuf {
        env::temp_dir()
            .join("wsl-bridge-dev-certificates")
            .join("local-ca")
    }

    fn generate_trusted_local_ca_leaf(hosts: &[&str], common_name: &str) -> (String, String) {
        let root_dir = proxy_test_local_ca_root_dir();
        fs::create_dir_all(&root_dir).expect("create local ca root dir");
        let root_cert_path = root_dir.join("root-ca.pem");
        let root_key_path = root_dir.join("root-ca.key");
        let (ca_cert, ca_key_pair) = super::ensure_local_ca_root(&root_cert_path, &root_key_path)
            .expect("ensure local ca root");

        let leaf_key = KeyPair::generate().expect("generate leaf key");
        let mut leaf_params = CertificateParams::new(
            hosts
                .iter()
                .map(|item| (*item).to_owned())
                .collect::<Vec<_>>(),
        )
        .expect("build leaf params");
        let mut distinguished_name = DistinguishedName::new();
        distinguished_name.push(DnType::CommonName, common_name);
        leaf_params.distinguished_name = distinguished_name;
        let leaf_cert = leaf_params
            .signed_by(&leaf_key, &ca_cert, &ca_key_pair)
            .expect("sign local ca leaf");
        (leaf_cert.pem(), leaf_key.serialize_pem())
    }

    fn create_test_tls_server_config(cert_pem: &str, key_pem: &str) -> ServerConfig {
        let mut cert_reader = BufReader::new(cert_pem.as_bytes());
        let cert_chain = rustls_pemfile::certs(&mut cert_reader)
            .collect::<Result<Vec<_>, _>>()
            .expect("read cert chain");
        let mut key_reader = BufReader::new(key_pem.as_bytes());
        let private_key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut key_reader)
            .expect("read key")
            .expect("private key");
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, private_key)
            .expect("build tls server config")
    }

    fn create_test_tls_client_config(root_cert_pem: &str) -> std::sync::Arc<ClientConfig> {
        let mut root_reader = BufReader::new(root_cert_pem.as_bytes());
        let root_certs = rustls_pemfile::certs(&mut root_reader)
            .collect::<Result<Vec<_>, _>>()
            .expect("read root certs");
        let mut root_store = RootCertStore::empty();
        for cert in root_certs {
            root_store.add(cert).expect("add root cert");
        }
        std::sync::Arc::new(
            ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth(),
        )
    }

    fn connect_tls_test_client(listen_port: u16) -> StreamOwned<ClientConnection, TcpStream> {
        let root_cert_path = proxy_test_local_ca_root_dir().join("root-ca.pem");
        let root_cert_pem = fs::read_to_string(&root_cert_path).expect("read root cert");
        let client_config = create_test_tls_client_config(&root_cert_pem);
        let outbound = TcpStream::connect(("127.0.0.1", listen_port)).expect("connect listener");
        outbound
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set read timeout");
        outbound
            .set_write_timeout(Some(Duration::from_secs(2)))
            .expect("set write timeout");
        let server_name = ServerName::IpAddress(std::net::IpAddr::from([127, 0, 0, 1]).into());
        let connection =
            ClientConnection::new(client_config, server_name).expect("create tls client");
        StreamOwned::new(connection, outbound)
    }

    fn wait_for_proxy_listener_state(
        engine: &RuleEngine,
        listener_id: &str,
        expected: RuntimeState,
    ) -> wsl_bridge_shared::ProxyRuntimeStatusItem {
        for _ in 0..20 {
            if let Some(status) = engine
                .get_proxy_runtime_status()
                .into_iter()
                .find(|item| item.listener_id == listener_id)
            {
                if status.state == expected {
                    return status;
                }
            }
            thread::sleep(Duration::from_millis(40));
        }
        engine
            .get_proxy_runtime_status()
            .into_iter()
            .find(|item| item.listener_id == listener_id)
            .expect("runtime status")
    }

    fn distinct_free_udp_port(existing: u16) -> u16 {
        loop {
            let port = free_udp_port();
            if port != existing {
                return port;
            }
        }
    }

    #[test]
    fn create_and_update_rule() {
        let engine = RuleEngine::new();
        let id = engine.create_rule(test_rule("web", 38080)).expect("create");
        engine
            .update_rule(
                &id,
                RulePatch {
                    name: Some("web-updated".to_owned()),
                    ..RulePatch::default()
                },
            )
            .expect("update");
        let rules = engine.list_rules();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "web-updated");
    }

    #[test]
    fn create_rule_rejects_listen_conflict() {
        let engine = RuleEngine::new();
        let _id1 = engine.create_rule(test_rule("a", 38100)).expect("create a");
        let err = engine
            .create_rule(test_rule("b", 38100))
            .expect_err("conflict should fail");
        assert!(err.to_string().contains("already used by rule"));
    }

    #[test]
    fn stop_all_rules() {
        let engine = RuleEngine::new();
        let _id = engine.create_rule(test_rule("web", 38120)).expect("create");
        let _ = engine.apply_rules();
        let result = engine.stop_rules();
        assert_eq!(result.stopped, 1);
    }

    #[test]
    fn query_logs_by_rule_id_works() {
        let engine = RuleEngine::new();
        let id = engine
            .create_rule(test_rule("log-query", 38125))
            .expect("create");
        let _ = engine.apply_rules();
        let result = engine.query_logs(LogQueryRequest {
            rule_id: Some(id.clone()),
            newest_first: Some(true),
            ..LogQueryRequest::default()
        });
        assert!(result.total >= 1);
        assert!(
            result
                .events
                .iter()
                .any(|item| item.detail.contains(&format!("rule_id={id}"))),
            "expected events containing rule_id"
        );
        let _ = engine.stop_rules();
    }

    #[test]
    fn rule_log_stats_works() {
        let engine = RuleEngine::new();
        let id = engine
            .create_rule(test_rule("log-stats", 38126))
            .expect("create");
        let _ = engine.apply_rules();
        let items = engine.get_rule_log_stats(RuleLogStatsRequest {
            rule_ids: Some(vec![id.clone()]),
            since_minutes: Some(60),
        });
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].rule_id, id);
        assert!(items[0].total >= 1);
        let _ = engine.stop_rules();
    }

    #[test]
    fn sqlite_roundtrip() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("duration")
            .as_nanos();
        let path = env::temp_dir().join(format!("wsl-bridge-test-{now}.db"));

        {
            let engine = RuleEngine::with_sqlite(&path).expect("sqlite engine");
            let id = engine
                .create_rule(test_rule("persisted", 38150))
                .expect("create");
            engine.enable_rule(&id, true).expect("enable");
            let _ = engine.apply_rules();
            let _ = engine.stop_rules();
        }

        {
            let engine = RuleEngine::with_sqlite(&path).expect("reload");
            let rules = engine.list_rules();
            assert_eq!(rules.len(), 1);
            assert_eq!(rules[0].name, "persisted");
            let logs = engine.tail_logs(0);
            assert!(!logs.events.is_empty());
        }

        let _ = fs::remove_file(path);
    }

    #[test]
    fn app_settings_roundtrip() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("duration")
            .as_nanos();
        let path = env::temp_dir().join(format!("wsl-bridge-settings-{now}.db"));

        {
            let engine = RuleEngine::with_sqlite(&path).expect("sqlite engine");
            engine
                .update_app_settings(AppSettings {
                    close_behavior: CloseBehavior::Minimize,
                    show_tray_on_start: false,
                    user_uid: None,
                })
                .expect("update app settings");
        }

        {
            let engine = RuleEngine::with_sqlite(&path).expect("reload");
            let settings = engine.get_app_settings();
            assert_eq!(settings.close_behavior, CloseBehavior::Minimize);
            assert!(!settings.show_tray_on_start);
        }

        let _ = fs::remove_file(path);
    }

    #[test]
    fn tcp_forwarding_works() {
        let target_port = free_tcp_port();
        let listen_port = free_tcp_port();

        let server = thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", target_port)).expect("target bind");
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 4];
            stream.read_exact(&mut buf).expect("read");
            stream.write_all(&buf).expect("write");
        });

        let engine = RuleEngine::new();
        let req = CreateRuleRequest {
            rule: NewProxyRule {
                name: "tcp-e2e".to_owned(),
                rule_type: RuleType::TcpFwd,
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port: Some(target_port),
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            },
            firewall: None,
        };
        let _id = engine.create_rule(req).expect("create rule");
        let result = engine.apply_rules();
        assert_eq!(result.failed.len(), 0);

        let mut client = TcpStream::connect(("127.0.0.1", listen_port)).expect("connect forward");
        client.write_all(b"ping").expect("send");
        let mut buf = [0u8; 4];
        client.read_exact(&mut buf).expect("recv");
        assert_eq!(&buf, b"ping");

        let _ = engine.stop_rules();
        let _ = server.join();
    }

    #[test]
    fn udp_forwarding_works() {
        let target_port = free_udp_port();
        let listen_port = distinct_free_udp_port(target_port);

        let server = thread::spawn(move || {
            let socket = UdpSocket::bind(("127.0.0.1", target_port)).expect("udp target bind");
            let mut buf = [0u8; 1024];
            let (len, src) = socket.recv_from(&mut buf).expect("recv");
            socket.send_to(&buf[..len], src).expect("send");
        });
        thread::sleep(Duration::from_millis(80));

        let engine = RuleEngine::new();
        let req = CreateRuleRequest {
            rule: NewProxyRule {
                name: "udp-e2e".to_owned(),
                rule_type: RuleType::UdpFwd,
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port: Some(target_port),
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            },
            firewall: None,
        };
        let _id = engine.create_rule(req).expect("create rule");
        let result = engine.apply_rules();
        assert_eq!(result.failed.len(), 0);

        let client = UdpSocket::bind(("127.0.0.1", 0)).expect("udp client bind");
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set timeout");
        client
            .send_to(b"pong", ("127.0.0.1", listen_port))
            .expect("send");
        let mut buf = [0u8; 16];
        let (len, _) = client.recv_from(&mut buf).expect("recv");
        assert_eq!(&buf[..len], b"pong");

        let _ = engine.stop_rules();
        let _ = server.join();
    }

    #[test]
    fn traffic_stats_roundtrip_works() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("duration")
            .as_nanos();
        let path = env::temp_dir().join(format!("wsl-bridge-traffic-{now}.db"));
        let target_port = free_udp_port();
        let listen_port = distinct_free_udp_port(target_port);

        let server = thread::spawn(move || {
            let socket = UdpSocket::bind(("127.0.0.1", target_port)).expect("udp target bind");
            let mut buf = [0u8; 1024];
            let (len, src) = socket.recv_from(&mut buf).expect("recv");
            socket.send_to(&buf[..len], src).expect("send");
        });
        thread::sleep(Duration::from_millis(80));

        let engine = RuleEngine::with_sqlite(&path).expect("sqlite engine");
        let req = CreateRuleRequest {
            rule: NewProxyRule {
                name: "traffic-udp".to_owned(),
                rule_type: RuleType::UdpFwd,
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port: Some(target_port),
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            },
            firewall: None,
        };
        let rule_id = engine.create_rule(req).expect("create rule");
        let result = engine.apply_rules();
        assert!(result.failed.is_empty());

        let client = UdpSocket::bind(("127.0.0.1", 0)).expect("udp client bind");
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set timeout");
        client
            .send_to(b"stat", ("127.0.0.1", listen_port))
            .expect("send");
        let mut buf = [0u8; 16];
        let (len, _) = client.recv_from(&mut buf).expect("recv");
        assert_eq!(&buf[..len], b"stat");

        thread::sleep(Duration::from_millis(150));

        let window = engine.get_traffic_window_data(vec![TrafficWindowQueryEntity {
            entity_type: TrafficEntityType::LegacyRule,
            entity_id: rule_id.clone(),
        }]);
        assert_eq!(window.len(), 1);
        assert!(window[0].samples.iter().any(|item| item.bytes_in > 0));

        let _ = engine.stop_rules();

        let stats = engine.query_traffic_stats(QueryTrafficStatsRequest {
            entity_type: TrafficEntityType::LegacyRule,
            entity_id: rule_id.clone(),
            ..QueryTrafficStatsRequest::default()
        });
        assert_eq!(stats.stats.len(), 1);
        assert!(stats.total_bytes_in >= 4);
        assert!(stats.total_bytes_out >= 4);
        assert!(stats.total_connections >= 1);

        let _ = server.join();
        let _ = fs::remove_file(path);
    }

    #[test]
    fn proxy_http_listener_routes_and_rewrites_path() {
        let target_port = free_tcp_port();
        let listen_port = free_tcp_port();

        let server = thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", target_port)).expect("target bind");
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set timeout");
            let mut request = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(len) => {
                        request.extend_from_slice(&chunk[..len]);
                        if request.windows(4).any(|window| window == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(err)
                        if err.kind() == std::io::ErrorKind::WouldBlock
                            || err.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        break;
                    }
                    Err(err) => panic!("read request failed: {err}"),
                }
            }
            let request_text = String::from_utf8_lossy(&request).to_string();
            let body = request_text
                .lines()
                .next()
                .unwrap_or("missing request line")
                .to_owned();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });

        let engine = RuleEngine::new();
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "http-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: vec!["a.example.com".to_owned()],
                path_prefix: Some("/api".to_owned()),
                is_default: false,
                enabled: true,
            })
            .expect("create route");

        let _upstream_id = engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port,
                upstream_scheme: UpstreamScheme::Http,
                path_rewrite_from: Some("/api".to_owned()),
                path_rewrite_to: Some("/".to_owned()),
                enabled: true,
            })
            .expect("create upstream");

        let status = wait_for_proxy_listener_state(&engine, &listener_id, RuntimeState::Running);
        assert_eq!(status.state, RuntimeState::Running);

        let mut client = TcpStream::connect(("127.0.0.1", listen_port)).expect("connect proxy");
        client
            .write_all(
                b"GET /api/ping HTTP/1.1\r\nHost: a.example.com\r\nConnection: close\r\n\r\n",
            )
            .expect("send request");
        let mut response = String::new();
        client.read_to_string(&mut response).expect("read response");

        assert!(response.contains("HTTP/1.1 200 OK"));
        assert!(response.contains("GET /ping HTTP/1.1"));

        drop(engine);
        let _ = server.join();
    }

    #[test]
    fn proxy_http_listener_does_not_half_close_plain_http_upstream_before_response() {
        let target_port = free_tcp_port();
        let listen_port = free_tcp_port();

        let server = thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", target_port)).expect("target bind");
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(Duration::from_millis(250)))
                .expect("set timeout");

            let mut request = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => panic!("proxy half-closed upstream before response"),
                    Ok(len) => {
                        request.extend_from_slice(&chunk[..len]);
                        if request.windows(4).any(|window| window == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(err)
                        if err.kind() == std::io::ErrorKind::WouldBlock
                            || err.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        break;
                    }
                    Err(err) => panic!("read request failed: {err}"),
                }
            }

            let request_text = String::from_utf8_lossy(&request).to_string();
            assert!(request_text.contains("GET / HTTP/1.1"));
            assert!(request_text
                .to_ascii_lowercase()
                .contains("connection: close"));

            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .expect("write response");
        });

        let engine = RuleEngine::new();
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "http-no-half-close-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: Vec::new(),
                path_prefix: None,
                is_default: true,
                enabled: true,
            })
            .expect("create route");

        engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port,
                upstream_scheme: UpstreamScheme::Http,
                path_rewrite_from: None,
                path_rewrite_to: None,
                enabled: true,
            })
            .expect("create upstream");

        let status = wait_for_proxy_listener_state(&engine, &listener_id, RuntimeState::Running);
        assert_eq!(status.state, RuntimeState::Running);

        let mut client = TcpStream::connect(("127.0.0.1", listen_port)).expect("connect proxy");
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost:8081\r\nConnection: keep-alive\r\n\r\n")
            .expect("send request");
        let mut response = String::new();
        client.read_to_string(&mut response).expect("read response");
        assert!(response.contains("HTTP/1.1 200 OK"));
        assert!(response.ends_with("ok"));

        drop(engine);
        let _ = server.join();
    }

    #[test]
    fn proxy_https_listener_does_not_half_close_plain_http_upstream_before_response() {
        let _local_ca_lock = proxy_test_local_ca_lock();
        let target_port = free_tcp_port();
        let listen_port = free_tcp_port();

        let server = thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", target_port)).expect("target bind");
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(Duration::from_millis(250)))
                .expect("set timeout");

            let mut request = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => panic!("proxy half-closed upstream before response"),
                    Ok(len) => {
                        request.extend_from_slice(&chunk[..len]);
                        if request.windows(4).any(|window| window == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(err)
                        if err.kind() == std::io::ErrorKind::WouldBlock
                            || err.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        break;
                    }
                    Err(err) => panic!("read request failed: {err}"),
                }
            }

            let request_text = String::from_utf8_lossy(&request).to_string();
            assert!(request_text.contains("GET / HTTP/1.1"));
            assert!(request_text
                .to_ascii_lowercase()
                .contains("connection: close"));

            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .expect("write response");
        });

        let (cert_pem, key_pem) =
            generate_trusted_local_ca_leaf(&["127.0.0.1"], "https-plain-http-upstream");
        let cert_path = write_temp_fixture("wsl-bridge-https-no-half-close-cert", "pem", &cert_pem);
        let key_path = write_temp_fixture("wsl-bridge-https-no-half-close-key", "key", &key_pem);

        let engine = RuleEngine::new();
        let cert_id = engine
            .create_proxy_certificate(CreateProxyCertificateRequest {
                name: "https-no-half-close".to_owned(),
                source_type: ProxyCertificateSourceType::ManualUpload,
                cert_path: cert_path.to_string_lossy().to_string(),
                key_path: key_path.to_string_lossy().to_string(),
                domains: vec!["127.0.0.1".to_owned()],
            })
            .expect("create certificate");

        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "https-no-half-close-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                protocol: ProxyProtocol::Https,
                tls_mode: ProxyTlsMode::ManualCert,
                cert_id: Some(cert_id),
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: Vec::new(),
                path_prefix: None,
                is_default: true,
                enabled: true,
            })
            .expect("create route");

        engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port,
                upstream_scheme: UpstreamScheme::Http,
                path_rewrite_from: None,
                path_rewrite_to: None,
                enabled: true,
            })
            .expect("create upstream");

        let status = wait_for_proxy_listener_state(&engine, &listener_id, RuntimeState::Running);
        assert_eq!(status.state, RuntimeState::Running);

        let mut client = connect_tls_test_client(listen_port);
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: keep-alive\r\n\r\n")
            .expect("send request");
        let mut response = String::new();
        client.read_to_string(&mut response).expect("read response");
        assert!(response.contains("HTTP/1.1 200 OK"));
        assert!(response.ends_with("ok"));

        drop(engine);
        let _ = server.join();
        let _ = fs::remove_file(cert_path);
        let _ = fs::remove_file(key_path);
    }

    #[test]
    fn proxy_route_and_upstream_runtime_metrics_are_recorded() {
        let target_port = free_tcp_port();
        let listen_port = free_tcp_port();

        let server = thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", target_port)).expect("target bind");
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set timeout");
            let mut request = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(len) => {
                        request.extend_from_slice(&chunk[..len]);
                        if request.windows(4).any(|window| window == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(err)
                        if err.kind() == std::io::ErrorKind::WouldBlock
                            || err.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        break;
                    }
                    Err(err) => panic!("read request failed: {err}"),
                }
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .expect("write response");
        });

        let engine = RuleEngine::new();
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "metrics-http-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: vec!["metrics.example.com".to_owned()],
                path_prefix: Some("/api".to_owned()),
                is_default: false,
                enabled: true,
            })
            .expect("create route");

        let upstream_id = engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id: route_id.clone(),
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port,
                upstream_scheme: UpstreamScheme::Http,
                path_rewrite_from: Some("/api".to_owned()),
                path_rewrite_to: Some("/".to_owned()),
                enabled: true,
            })
            .expect("create upstream");

        thread::sleep(Duration::from_millis(180));

        let mut client = TcpStream::connect(("127.0.0.1", listen_port)).expect("connect proxy");
        client
            .write_all(
                b"GET /api/ping HTTP/1.1\r\nHost: metrics.example.com\r\nConnection: close\r\n\r\n",
            )
            .expect("send request");
        let mut response = String::new();
        client.read_to_string(&mut response).expect("read response");
        assert!(response.contains("HTTP/1.1 200 OK"));

        thread::sleep(Duration::from_millis(120));

        let route_runtime = engine
            .list_proxy_route_runtime(&listener_id)
            .into_iter()
            .find(|item| item.route_id == route_id)
            .expect("route runtime");
        assert_eq!(route_runtime.hit_count, 1);
        assert_eq!(route_runtime.error_count, 0);
        assert_eq!(
            route_runtime.last_server_name.as_deref(),
            Some("metrics.example.com")
        );
        assert_eq!(
            route_runtime.last_request_path.as_deref(),
            Some("/api/ping")
        );

        let upstream_runtime = engine
            .list_proxy_upstream_runtime(&route_id)
            .into_iter()
            .find(|item| item.upstream_id == upstream_id)
            .expect("upstream runtime");
        assert_eq!(upstream_runtime.hit_count, 1);
        let traffic_entities = engine.list_traffic_monitor_entities();
        let proxy_entity = traffic_entities
            .into_iter()
            .find(|item| item.entity_id == upstream_id)
            .expect("proxy traffic entity");
        assert_eq!(
            proxy_entity.label,
            "metrics / metrics / 127.0.0"
        );
        assert_eq!(upstream_runtime.error_count, 0);
        assert!(upstream_runtime
            .last_target
            .as_deref()
            .unwrap_or_default()
            .contains("/ping"));
        assert_eq!(upstream_runtime.last_request_path.as_deref(), Some("/ping"));

        drop(engine);
        let _ = server.join();
    }

    #[test]
    fn proxy_traffic_entity_label_updates_after_route_server_name_change() {
        let engine = RuleEngine::new();
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "listener-alpha".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port: free_tcp_port(),
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: vec!["before.example.com".to_owned()],
                path_prefix: None,
                is_default: false,
                enabled: true,
            })
            .expect("create route");

        let upstream_id = engine
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

        let before = engine
            .list_traffic_monitor_entities()
            .into_iter()
            .find(|item| item.entity_id == upstream_id)
            .expect("traffic entity before update");
        assert_eq!(before.label, "listene / before. / 127.0.0");

        engine
            .update_proxy_route(
                &route_id,
                UpdateProxyRouteRequest {
                    server_names: vec!["after.example.com".to_owned()],
                    path_prefix: None,
                    is_default: false,
                    enabled: true,
                },
            )
            .expect("update route");

        let after = engine
            .list_traffic_monitor_entities()
            .into_iter()
            .find(|item| item.entity_id == upstream_id)
            .expect("traffic entity after update");
        assert_eq!(after.label, "listene / after.e / 127.0.0");
    }

    #[test]
    fn proxy_default_route_traffic_entity_label_prefers_server_name_when_present() {
        let engine = RuleEngine::new();
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "listener-alpha".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port: free_tcp_port(),
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id,
                server_names: vec!["before.example.com".to_owned()],
                path_prefix: None,
                is_default: true,
                enabled: true,
            })
            .expect("create route");

        let upstream_id = engine
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

        let before = engine
            .list_traffic_monitor_entities()
            .into_iter()
            .find(|item| item.entity_id == upstream_id)
            .expect("traffic entity before update");
        assert_eq!(before.label, "listene / before. / 127.0.0");

        engine
            .update_proxy_route(
                &route_id,
                UpdateProxyRouteRequest {
                    server_names: vec!["after.example.com".to_owned()],
                    path_prefix: None,
                    is_default: true,
                    enabled: true,
                },
            )
            .expect("update default route");

        let after = engine
            .list_traffic_monitor_entities()
            .into_iter()
            .find(|item| item.entity_id == upstream_id)
            .expect("traffic entity after update");
        assert_eq!(after.label, "listene / after.e / 127.0.0");
    }

    #[test]
    fn proxy_http_listener_proxies_websocket_upgrade_and_stream() {
        let target_port = free_tcp_port();
        let listen_port = free_tcp_port();

        let server = thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", target_port)).expect("target bind");
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set timeout");

            let mut request = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(len) => {
                        request.extend_from_slice(&chunk[..len]);
                        if request.windows(4).any(|window| window == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(err)
                        if err.kind() == std::io::ErrorKind::WouldBlock
                            || err.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        break;
                    }
                    Err(err) => panic!("read request failed: {err}"),
                }
            }

            let request_text = String::from_utf8_lossy(&request).to_string();
            assert!(request_text.starts_with("GET /socket/chat HTTP/1.1"));
            assert!(request_text
                .to_ascii_lowercase()
                .contains("upgrade: websocket"));

            stream
                .write_all(
                    b"HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\n",
                )
                .expect("write upgrade response");

            let mut payload = [0u8; 4];
            stream
                .read_exact(&mut payload)
                .expect("read websocket payload");
            assert_eq!(&payload, b"ping");
            stream.write_all(b"pong").expect("write websocket echo");
        });

        let engine = RuleEngine::new();
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "ws-http-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: vec!["ws.example.com".to_owned()],
                path_prefix: Some("/ws".to_owned()),
                is_default: false,
                enabled: true,
            })
            .expect("create route");

        engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id: route_id.clone(),
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port,
                upstream_scheme: UpstreamScheme::Ws,
                path_rewrite_from: Some("/ws".to_owned()),
                path_rewrite_to: Some("/socket".to_owned()),
                enabled: true,
            })
            .expect("create upstream");

        thread::sleep(Duration::from_millis(180));

        let mut client = TcpStream::connect(("127.0.0.1", listen_port)).expect("connect proxy");
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set timeout");
        client
            .write_all(
                b"GET /ws/chat HTTP/1.1\r\nHost: ws.example.com\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Key: test-key\r\nSec-WebSocket-Version: 13\r\n\r\n",
            )
            .expect("send websocket upgrade request");

        let mut response = Vec::new();
        let mut header_chunk = [0u8; 1024];
        loop {
            let len = client
                .read(&mut header_chunk)
                .expect("read upgrade response");
            if len == 0 {
                break;
            }
            response.extend_from_slice(&header_chunk[..len]);
            if response.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let response_text = String::from_utf8_lossy(&response).to_string();
        assert!(response_text.contains("101 Switching Protocols"));

        client.write_all(b"ping").expect("send websocket payload");
        let mut echoed = [0u8; 4];
        client.read_exact(&mut echoed).expect("read websocket echo");
        assert_eq!(&echoed, b"pong");

        thread::sleep(Duration::from_millis(120));

        let route_runtime = engine
            .list_proxy_route_runtime(&listener_id)
            .into_iter()
            .find(|item| item.route_id == route_id)
            .expect("route runtime");
        assert_eq!(route_runtime.hit_count, 1);
        assert_eq!(route_runtime.error_count, 0);

        drop(engine);
        let _ = server.join();
    }

    #[test]
    fn proxy_http_listener_proxies_https_upstream_response() {
        let _local_ca_lock = proxy_test_local_ca_lock();
        let (target_listener, target_port) = bind_test_tcp_listener();
        let listen_port = free_tcp_port();

        let (cert_pem, key_pem) =
            generate_trusted_local_ca_leaf(&["127.0.0.1"], "trusted-https-upstream");
        let tls_config = std::sync::Arc::new(create_test_tls_server_config(&cert_pem, &key_pem));

        let server = thread::spawn(move || {
            let (stream, _) = target_listener.accept().expect("accept");
            let connection =
                ServerConnection::new(tls_config).expect("create tls server connection");
            let mut stream = StreamOwned::new(connection, stream);
            let mut request = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(len) => {
                        request.extend_from_slice(&chunk[..len]);
                        if request.windows(4).any(|window| window == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(err) => panic!("read request failed: {err}"),
                }
            }
            let request_text = String::from_utf8_lossy(&request).to_string();
            let body = request_text
                .lines()
                .next()
                .unwrap_or("missing request line")
                .to_owned();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });

        let engine = RuleEngine::new();
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "https-upstream-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id,
                server_names: vec!["secure.example.com".to_owned()],
                path_prefix: Some("/api".to_owned()),
                is_default: false,
                enabled: true,
            })
            .expect("create route");

        engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id: route_id.clone(),
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port,
                upstream_scheme: UpstreamScheme::Https,
                path_rewrite_from: Some("/api".to_owned()),
                path_rewrite_to: Some("/secure".to_owned()),
                enabled: true,
            })
            .expect("create upstream");

        thread::sleep(Duration::from_millis(180));

        let mut client = TcpStream::connect(("127.0.0.1", listen_port)).expect("connect proxy");
        client
            .write_all(
                b"GET /api/ping HTTP/1.1\r\nHost: secure.example.com\r\nConnection: close\r\n\r\n",
            )
            .expect("send request");
        let mut response = String::new();
        client.read_to_string(&mut response).expect("read response");

        assert!(response.contains("HTTP/1.1 200 OK"));
        assert!(response.contains("GET /secure/ping HTTP/1.1"));

        drop(engine);
        let _ = server.join();
    }

    #[test]
    fn proxy_http_listener_records_error_for_untrusted_https_upstream() {
        let (target_listener, target_port) = bind_test_tcp_listener();
        let listen_port = free_tcp_port();

        let CertifiedKey { cert, key_pair } =
            generate_simple_self_signed(vec!["127.0.0.1".to_owned()])
                .expect("generate untrusted certificate");
        let tls_config = std::sync::Arc::new(create_test_tls_server_config(
            &cert.pem(),
            &key_pair.serialize_pem(),
        ));

        let server = thread::spawn(move || {
            let (stream, _) = target_listener.accept().expect("accept");
            let connection =
                ServerConnection::new(tls_config).expect("create tls server connection");
            let mut stream = StreamOwned::new(connection, stream);
            let mut sink = [0u8; 256];
            let _ = stream.read(&mut sink);
        });

        let engine = RuleEngine::new();
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "https-untrusted-upstream-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: vec!["untrusted.example.com".to_owned()],
                path_prefix: Some("/api".to_owned()),
                is_default: false,
                enabled: true,
            })
            .expect("create route");

        let upstream_id = engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id: route_id.clone(),
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port,
                upstream_scheme: UpstreamScheme::Https,
                path_rewrite_from: Some("/api".to_owned()),
                path_rewrite_to: Some("/secure".to_owned()),
                enabled: true,
            })
            .expect("create upstream");

        thread::sleep(Duration::from_millis(180));

        let mut client = TcpStream::connect(("127.0.0.1", listen_port)).expect("connect proxy");
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set timeout");
        client
            .write_all(
                b"GET /api/ping HTTP/1.1\r\nHost: untrusted.example.com\r\nConnection: close\r\n\r\n",
            )
            .expect("send request");
        let mut response = String::new();
        client.read_to_string(&mut response).expect("read response");

        assert!(response.contains("HTTP/1.1 502 Bad Gateway"));

        thread::sleep(Duration::from_millis(120));

        let route_runtime = engine
            .list_proxy_route_runtime(&listener_id)
            .into_iter()
            .find(|item| item.route_id == route_id)
            .expect("route runtime");
        assert_eq!(route_runtime.hit_count, 1);
        assert_eq!(route_runtime.error_count, 1);

        let upstream_runtime = engine
            .list_proxy_upstream_runtime(&route_id)
            .into_iter()
            .find(|item| item.upstream_id == upstream_id)
            .expect("upstream runtime");
        assert_eq!(upstream_runtime.hit_count, 0);
        assert_eq!(upstream_runtime.error_count, 1);

        drop(engine);
        let _ = server.join();
    }

    #[test]
    fn proxy_http_listener_proxies_wss_upgrade_and_stream() {
        let _local_ca_lock = proxy_test_local_ca_lock();
        let (target_listener, target_port) = bind_test_tcp_listener();
        let listen_port = free_tcp_port();

        let (cert_pem, key_pem) =
            generate_trusted_local_ca_leaf(&["127.0.0.1"], "trusted-wss-upstream");
        let tls_config = std::sync::Arc::new(create_test_tls_server_config(&cert_pem, &key_pem));

        let server = thread::spawn(move || {
            let (stream, _) = target_listener.accept().expect("accept");
            let connection =
                ServerConnection::new(tls_config).expect("create tls server connection");
            let mut stream = StreamOwned::new(connection, stream);

            let mut request = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(len) => {
                        request.extend_from_slice(&chunk[..len]);
                        if request.windows(4).any(|window| window == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(err) => panic!("read request failed: {err}"),
                }
            }

            let request_text = String::from_utf8_lossy(&request).to_string();
            assert!(request_text.starts_with("GET /socket/chat HTTP/1.1"));
            assert!(request_text
                .to_ascii_lowercase()
                .contains("upgrade: websocket"));

            stream
                .write_all(
                    b"HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\n",
                )
                .expect("write upgrade response");

            let mut payload = [0u8; 4];
            stream
                .read_exact(&mut payload)
                .expect("read websocket payload");
            assert_eq!(&payload, b"ping");
            stream.write_all(b"pong").expect("write websocket echo");
        });

        let engine = RuleEngine::new();
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "wss-upstream-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: vec!["wss.example.com".to_owned()],
                path_prefix: Some("/wss".to_owned()),
                is_default: false,
                enabled: true,
            })
            .expect("create route");

        engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id: route_id.clone(),
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port,
                upstream_scheme: UpstreamScheme::Wss,
                path_rewrite_from: Some("/wss".to_owned()),
                path_rewrite_to: Some("/socket".to_owned()),
                enabled: true,
            })
            .expect("create upstream");

        thread::sleep(Duration::from_millis(180));

        let mut client = TcpStream::connect(("127.0.0.1", listen_port)).expect("connect proxy");
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set timeout");
        client
            .write_all(
                b"GET /wss/chat HTTP/1.1\r\nHost: wss.example.com\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Key: test-key\r\nSec-WebSocket-Version: 13\r\n\r\n",
            )
            .expect("send websocket upgrade request");

        let mut response = Vec::new();
        let mut header_chunk = [0u8; 1024];
        loop {
            let len = client
                .read(&mut header_chunk)
                .expect("read upgrade response");
            if len == 0 {
                break;
            }
            response.extend_from_slice(&header_chunk[..len]);
            if response.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let response_text = String::from_utf8_lossy(&response).to_string();
        assert!(response_text.contains("101 Switching Protocols"));

        client.write_all(b"ping").expect("send websocket payload");
        let mut echoed = [0u8; 4];
        client.read_exact(&mut echoed).expect("read websocket echo");
        assert_eq!(&echoed, b"pong");

        thread::sleep(Duration::from_millis(120));

        let route_runtime = engine
            .list_proxy_route_runtime(&listener_id)
            .into_iter()
            .find(|item| item.route_id == route_id)
            .expect("route runtime");
        assert_eq!(route_runtime.hit_count, 1);
        assert_eq!(route_runtime.error_count, 0);

        drop(engine);
        let _ = server.join();
    }

    #[test]
    fn proxy_http_listener_records_error_for_untrusted_wss_upstream() {
        let (target_listener, target_port) = bind_test_tcp_listener();
        let listen_port = free_tcp_port();

        let CertifiedKey { cert, key_pair } =
            generate_simple_self_signed(vec!["127.0.0.1".to_owned()])
                .expect("generate untrusted certificate");
        let tls_config = std::sync::Arc::new(create_test_tls_server_config(
            &cert.pem(),
            &key_pair.serialize_pem(),
        ));

        let server = thread::spawn(move || {
            let (stream, _) = target_listener.accept().expect("accept");
            let connection =
                ServerConnection::new(tls_config).expect("create tls server connection");
            let mut stream = StreamOwned::new(connection, stream);
            let mut sink = [0u8; 256];
            let _ = stream.read(&mut sink);
        });

        let engine = RuleEngine::new();
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "wss-untrusted-upstream-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: vec!["untrusted-wss.example.com".to_owned()],
                path_prefix: Some("/wss".to_owned()),
                is_default: false,
                enabled: true,
            })
            .expect("create route");

        let upstream_id = engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id: route_id.clone(),
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port,
                upstream_scheme: UpstreamScheme::Wss,
                path_rewrite_from: Some("/wss".to_owned()),
                path_rewrite_to: Some("/socket".to_owned()),
                enabled: true,
            })
            .expect("create upstream");

        thread::sleep(Duration::from_millis(180));

        let mut client = TcpStream::connect(("127.0.0.1", listen_port)).expect("connect proxy");
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set timeout");
        client
            .write_all(
                b"GET /wss/chat HTTP/1.1\r\nHost: untrusted-wss.example.com\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Key: test-key\r\nSec-WebSocket-Version: 13\r\n\r\n",
            )
            .expect("send websocket upgrade request");

        let mut response = String::new();
        client.read_to_string(&mut response).expect("read response");

        assert!(response.contains("HTTP/1.1 502 Bad Gateway"));

        thread::sleep(Duration::from_millis(120));

        let route_runtime = engine
            .list_proxy_route_runtime(&listener_id)
            .into_iter()
            .find(|item| item.route_id == route_id)
            .expect("route runtime");
        assert_eq!(route_runtime.hit_count, 1);
        assert_eq!(route_runtime.error_count, 1);

        let upstream_runtime = engine
            .list_proxy_upstream_runtime(&route_id)
            .into_iter()
            .find(|item| item.upstream_id == upstream_id)
            .expect("upstream runtime");
        assert_eq!(upstream_runtime.hit_count, 0);
        assert_eq!(upstream_runtime.error_count, 1);

        drop(engine);
        let _ = server.join();
    }

    #[test]
    fn tcp_rule_can_migrate_to_proxy() {
        let target_port = free_tcp_port();
        let listen_port = free_tcp_port();

        let engine = RuleEngine::new();
        let rule_id = engine
            .create_rule(CreateRuleRequest {
                rule: NewProxyRule {
                    name: "legacy-tcp".to_owned(),
                    rule_type: RuleType::TcpFwd,
                    listen_host: "127.0.0.1".to_owned(),
                    listen_port,
                    target_kind: TargetKind::Static,
                    target_ref: None,
                    target_host: Some("127.0.0.1".to_owned()),
                    target_port: Some(target_port),
                    bind_mode: BindMode::AllNics,
                    nic_id: None,
                    enabled: true,
                },
                firewall: None,
            })
            .expect("create rule");

        let migration = engine
            .migrate_rule_to_proxy(&rule_id)
            .expect("migrate rule");
        assert_eq!(migration.status, RuleMigrationStatus::Migrated);
        assert_eq!(migration.rule_id, rule_id);
        assert!(migration.detail.is_none());

        let migrated_rule = engine
            .list_rules()
            .into_iter()
            .find(|item| item.id == rule_id)
            .expect("legacy rule");
        assert!(!migrated_rule.enabled);

        let listener = engine
            .list_proxy_listeners()
            .into_iter()
            .find(|item| item.id == migration.proxy_listener_id)
            .expect("proxy listener");
        assert_eq!(listener.listen_port, listen_port);
        assert!(listener.enabled);

        let route = engine
            .list_proxy_routes(&listener.id)
            .expect("list routes")
            .into_iter()
            .find(|item| item.id == migration.proxy_route_id)
            .expect("proxy route");
        assert!(route.is_default);
        assert!(route.server_names.is_empty());

        let upstream = engine
            .list_proxy_upstreams(&route.id)
            .expect("list upstreams")
            .into_iter()
            .find(|item| Some(item.id.clone()) == migration.proxy_upstream_id)
            .expect("proxy upstream");
        assert_eq!(upstream.target_host.as_deref(), Some("127.0.0.1"));
        assert_eq!(upstream.target_port, target_port);
        assert!(upstream.enabled);
    }

    #[test]
    fn http_proxy_rule_migrates_as_proxy_draft() {
        let listen_port = free_tcp_port();

        let engine = RuleEngine::new();
        let rule_id = engine
            .create_rule(CreateRuleRequest {
                rule: NewProxyRule {
                    name: "legacy-http-proxy".to_owned(),
                    rule_type: RuleType::HttpProxy,
                    listen_host: "127.0.0.1".to_owned(),
                    listen_port,
                    target_kind: TargetKind::Static,
                    target_ref: None,
                    target_host: None,
                    target_port: None,
                    bind_mode: BindMode::AllNics,
                    nic_id: None,
                    enabled: true,
                },
                firewall: None,
            })
            .expect("create rule");

        let migration = engine
            .migrate_rule_to_proxy(&rule_id)
            .expect("migrate http proxy");
        assert_eq!(migration.status, RuleMigrationStatus::Migrated);
        assert!(migration.detail.is_some());

        let listener = engine
            .list_proxy_listeners()
            .into_iter()
            .find(|item| item.id == migration.proxy_listener_id)
            .expect("proxy listener");
        let route = engine
            .list_proxy_routes(&listener.id)
            .expect("list routes")
            .into_iter()
            .find(|item| item.id == migration.proxy_route_id)
            .expect("proxy route");
        assert_eq!(route.server_names, vec!["127.0.0.1".to_owned()]);
        assert!(!route.is_default);

        let upstream = engine
            .list_proxy_upstreams(&route.id)
            .expect("list upstreams")
            .into_iter()
            .find(|item| Some(item.id.clone()) == migration.proxy_upstream_id)
            .expect("proxy upstream");
        assert!(!upstream.enabled);
        assert_eq!(upstream.target_host.as_deref(), Some("127.0.0.1"));
        assert_eq!(upstream.target_port, 80);
    }

    #[test]
    fn proxy_certificate_can_be_created_and_bound_to_https_listener() {
        let cert_path = write_temp_fixture(
            "wsl-bridge-cert",
            "pem",
            "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n",
        );
        let key_path = write_temp_fixture(
            "wsl-bridge-key",
            "key",
            "-----BEGIN PRIVATE KEY-----\nMIIB\n-----END PRIVATE KEY-----\n",
        );

        let engine = RuleEngine::new();
        let certificate_id = engine
            .create_proxy_certificate(CreateProxyCertificateRequest {
                name: "dev-cert".to_owned(),
                source_type: ProxyCertificateSourceType::ManualUpload,
                cert_path: cert_path.display().to_string(),
                key_path: key_path.display().to_string(),
                domains: vec!["example.test".to_owned(), "*.example.test".to_owned()],
            })
            .expect("create proxy certificate");

        let certificates = engine.list_proxy_certificates();
        assert_eq!(certificates.len(), 1);
        assert_eq!(certificates[0].id, certificate_id);
        assert_eq!(certificates[0].domains.len(), 2);

        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "https-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port: free_tcp_port(),
                protocol: ProxyProtocol::Https,
                tls_mode: ProxyTlsMode::ManualCert,
                cert_id: Some(certificate_id.clone()),
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: false,
            })
            .expect("create https listener");

        let listener = engine
            .list_proxy_listeners()
            .into_iter()
            .find(|item| item.id == listener_id)
            .expect("listener");
        assert_eq!(listener.cert_id.as_deref(), Some(certificate_id.as_str()));

        let _ = fs::remove_file(cert_path);
        let _ = fs::remove_file(key_path);
    }

    #[test]
    fn proxy_https_listener_starts_with_manual_certificate() {
        let target_port = free_tcp_port();
        let listen_port = free_tcp_port();
        let CertifiedKey { cert, key_pair } =
            generate_simple_self_signed(vec!["example.test".to_owned()])
                .expect("generate certificate");
        let cert_pem = cert.pem();
        let key_pem = key_pair.serialize_pem();
        let cert_path = write_temp_fixture("wsl-bridge-https-cert", "pem", &cert_pem);
        let key_path = write_temp_fixture("wsl-bridge-https-key", "key", &key_pem);

        let engine = RuleEngine::new();
        let certificate_id = engine
            .create_proxy_certificate(CreateProxyCertificateRequest {
                name: "runtime-https-cert".to_owned(),
                source_type: ProxyCertificateSourceType::ManualUpload,
                cert_path: cert_path.display().to_string(),
                key_path: key_path.display().to_string(),
                domains: vec!["example.test".to_owned()],
            })
            .expect("create proxy certificate");

        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "https-runtime-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                protocol: ProxyProtocol::Https,
                tls_mode: ProxyTlsMode::ManualCert,
                cert_id: Some(certificate_id),
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: vec!["example.test".to_owned()],
                path_prefix: Some("/api".to_owned()),
                is_default: false,
                enabled: true,
            })
            .expect("create route");

        engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id: route_id.clone(),
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port,
                upstream_scheme: UpstreamScheme::Http,
                path_rewrite_from: Some("/api".to_owned()),
                path_rewrite_to: Some("/".to_owned()),
                enabled: true,
            })
            .expect("create upstream");

        let status = wait_for_proxy_listener_state(&engine, &listener_id, RuntimeState::Running);
        assert_eq!(status.state, RuntimeState::Running);
        assert!(status.last_error.is_none());

        drop(engine);
        let _ = fs::remove_file(cert_path);
        let _ = fs::remove_file(key_path);
    }

    #[test]
    fn proxy_https_listener_accepts_https_upstream_configuration() {
        let listen_port = free_tcp_port();

        let CertifiedKey {
            cert: inbound_cert,
            key_pair: inbound_key,
        } = generate_simple_self_signed(vec!["secure.example.test".to_owned()])
            .expect("generate inbound certificate");
        let inbound_cert_path =
            write_temp_fixture("wsl-bridge-https-inbound-cert", "pem", &inbound_cert.pem());
        let inbound_key_path = write_temp_fixture(
            "wsl-bridge-https-inbound-key",
            "key",
            &inbound_key.serialize_pem(),
        );

        let engine = RuleEngine::new();
        let certificate_id = engine
            .create_proxy_certificate(CreateProxyCertificateRequest {
                name: "inbound-https-cert".to_owned(),
                source_type: ProxyCertificateSourceType::ManualUpload,
                cert_path: inbound_cert_path.display().to_string(),
                key_path: inbound_key_path.display().to_string(),
                domains: vec!["secure.example.test".to_owned()],
            })
            .expect("create inbound certificate");

        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "inbound-https-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                protocol: ProxyProtocol::Https,
                tls_mode: ProxyTlsMode::ManualCert,
                cert_id: Some(certificate_id),
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: vec!["secure.example.test".to_owned()],
                path_prefix: Some("/api".to_owned()),
                is_default: false,
                enabled: true,
            })
            .expect("create route");

        engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port: 443,
                upstream_scheme: UpstreamScheme::Https,
                path_rewrite_from: Some("/api".to_owned()),
                path_rewrite_to: Some("/secure".to_owned()),
                enabled: true,
            })
            .expect("create upstream");

        let status = wait_for_proxy_listener_state(&engine, &listener_id, RuntimeState::Running);
        assert_eq!(status.state, RuntimeState::Running);
        assert!(status.last_error.is_none());

        let _ = fs::remove_file(inbound_cert_path);
        let _ = fs::remove_file(inbound_key_path);
        drop(engine);
    }

    #[test]
    fn proxy_https_listener_accepts_wss_upstream_configuration() {
        let listen_port = free_tcp_port();

        let CertifiedKey {
            cert: inbound_cert,
            key_pair: inbound_key,
        } = generate_simple_self_signed(vec!["secure.example.test".to_owned()])
            .expect("generate inbound certificate");
        let inbound_cert_path =
            write_temp_fixture("wsl-bridge-https-inbound-cert", "pem", &inbound_cert.pem());
        let inbound_key_path = write_temp_fixture(
            "wsl-bridge-https-inbound-key",
            "key",
            &inbound_key.serialize_pem(),
        );

        let engine = RuleEngine::new();
        let certificate_id = engine
            .create_proxy_certificate(CreateProxyCertificateRequest {
                name: "inbound-https-cert".to_owned(),
                source_type: ProxyCertificateSourceType::ManualUpload,
                cert_path: inbound_cert_path.display().to_string(),
                key_path: inbound_key_path.display().to_string(),
                domains: vec!["secure.example.test".to_owned()],
            })
            .expect("create inbound certificate");

        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "inbound-https-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                protocol: ProxyProtocol::Https,
                tls_mode: ProxyTlsMode::ManualCert,
                cert_id: Some(certificate_id),
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: vec!["secure.example.test".to_owned()],
                path_prefix: Some("/wss".to_owned()),
                is_default: false,
                enabled: true,
            })
            .expect("create route");

        engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port: 443,
                upstream_scheme: UpstreamScheme::Wss,
                path_rewrite_from: Some("/wss".to_owned()),
                path_rewrite_to: Some("/socket".to_owned()),
                enabled: true,
            })
            .expect("create upstream");

        let status = wait_for_proxy_listener_state(&engine, &listener_id, RuntimeState::Running);
        assert_eq!(status.state, RuntimeState::Running);
        assert!(status.last_error.is_none());

        let _ = fs::remove_file(inbound_cert_path);
        let _ = fs::remove_file(inbound_key_path);
        drop(engine);
    }

    #[test]
    fn grpc_upstream_requires_http_listener() {
        let CertifiedKey {
            cert: inbound_cert,
            key_pair: inbound_key,
        } = generate_simple_self_signed(vec!["grpc.example.test".to_owned()])
            .expect("generate inbound certificate");
        let inbound_cert_path =
            write_temp_fixture("wsl-bridge-grpc-inbound-cert", "pem", &inbound_cert.pem());
        let inbound_key_path = write_temp_fixture(
            "wsl-bridge-grpc-inbound-key",
            "key",
            &inbound_key.serialize_pem(),
        );

        let engine = RuleEngine::new();
        let certificate_id = engine
            .create_proxy_certificate(CreateProxyCertificateRequest {
                name: "grpc-inbound-cert".to_owned(),
                source_type: ProxyCertificateSourceType::ManualUpload,
                cert_path: inbound_cert_path.display().to_string(),
                key_path: inbound_key_path.display().to_string(),
                domains: vec!["grpc.example.test".to_owned()],
            })
            .expect("create inbound certificate");
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "grpc-http-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port: free_tcp_port(),
                protocol: ProxyProtocol::Https,
                tls_mode: ProxyTlsMode::ManualCert,
                cert_id: Some(certificate_id),
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: false,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id,
                server_names: vec!["grpc.example.test".to_owned()],
                path_prefix: Some("/".to_owned()),
                is_default: false,
                enabled: true,
            })
            .expect("create route");

        let err = engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port: 50051,
                upstream_scheme: UpstreamScheme::Grpc,
                path_rewrite_from: None,
                path_rewrite_to: None,
                enabled: true,
            })
            .expect_err("grpc upstream should be rejected on https listener");

        assert!(
            matches!(err, EngineError::InvalidProxy(message) if message.contains("grpc upstream requires an http listener"))
        );

        let _ = fs::remove_file(inbound_cert_path);
        let _ = fs::remove_file(inbound_key_path);
    }

    #[test]
    fn grpcs_upstream_requires_https_listener() {
        let engine = RuleEngine::new();
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "grpcs-http-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port: free_tcp_port(),
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: false,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id,
                server_names: vec!["grpcs.example.test".to_owned()],
                path_prefix: Some("/".to_owned()),
                is_default: false,
                enabled: true,
            })
            .expect("create route");

        let err = engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port: 50051,
                upstream_scheme: UpstreamScheme::Grpcs,
                path_rewrite_from: None,
                path_rewrite_to: None,
                enabled: true,
            })
            .expect_err("grpcs upstream should be rejected on http listener");

        assert!(
            matches!(err, EngineError::InvalidProxy(message) if message.contains("grpcs upstream requires an https listener"))
        );
    }

    #[test]
    fn grpc_upstream_requires_default_route() {
        let engine = RuleEngine::new();
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "grpc-route-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port: free_tcp_port(),
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: false,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id,
                server_names: vec!["grpc.example.test".to_owned()],
                path_prefix: Some("/grpc".to_owned()),
                is_default: false,
                enabled: true,
            })
            .expect("create route");

        let err = engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port: 50051,
                upstream_scheme: UpstreamScheme::Grpc,
                path_rewrite_from: None,
                path_rewrite_to: None,
                enabled: true,
            })
            .expect_err("grpc upstream should require default route");

        assert!(
            matches!(err, EngineError::InvalidProxy(message) if message.contains("grpc upstream currently requires a default route"))
        );
    }

    #[test]
    fn grpc_upstream_rejects_path_rewrite() {
        let engine = RuleEngine::new();
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "grpc-rewrite-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port: free_tcp_port(),
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: false,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id,
                server_names: vec![],
                path_prefix: None,
                is_default: true,
                enabled: true,
            })
            .expect("create route");

        let err = engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port: 50051,
                upstream_scheme: UpstreamScheme::Grpc,
                path_rewrite_from: Some("/grpc".to_owned()),
                path_rewrite_to: Some("/".to_owned()),
                enabled: true,
            })
            .expect_err("grpc upstream should reject path rewrite");

        assert!(
            matches!(err, EngineError::InvalidProxy(message) if message.contains("grpc upstream does not support path rewrite yet"))
        );
    }

    #[test]
    fn grpcs_upstream_requires_default_route() {
        let CertifiedKey {
            cert: inbound_cert,
            key_pair: inbound_key,
        } = generate_simple_self_signed(vec!["grpcs.example.test".to_owned()])
            .expect("generate inbound certificate");
        let inbound_cert_path =
            write_temp_fixture("wsl-bridge-grpcs-inbound-cert", "pem", &inbound_cert.pem());
        let inbound_key_path = write_temp_fixture(
            "wsl-bridge-grpcs-inbound-key",
            "key",
            &inbound_key.serialize_pem(),
        );

        let engine = RuleEngine::new();
        let certificate_id = engine
            .create_proxy_certificate(CreateProxyCertificateRequest {
                name: "grpcs-inbound-cert".to_owned(),
                source_type: ProxyCertificateSourceType::ManualUpload,
                cert_path: inbound_cert_path.display().to_string(),
                key_path: inbound_key_path.display().to_string(),
                domains: vec!["grpcs.example.test".to_owned()],
            })
            .expect("create inbound certificate");
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "grpcs-route-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port: free_tcp_port(),
                protocol: ProxyProtocol::Https,
                tls_mode: ProxyTlsMode::ManualCert,
                cert_id: Some(certificate_id),
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: false,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id,
                server_names: vec!["grpcs.example.test".to_owned()],
                path_prefix: Some("/grpc".to_owned()),
                is_default: false,
                enabled: true,
            })
            .expect("create route");

        let err = engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port: 50051,
                upstream_scheme: UpstreamScheme::Grpcs,
                path_rewrite_from: None,
                path_rewrite_to: None,
                enabled: true,
            })
            .expect_err("grpcs upstream should require default route");

        assert!(
            matches!(err, EngineError::InvalidProxy(message) if message.contains("grpcs upstream currently requires a default route"))
        );

        let _ = fs::remove_file(inbound_cert_path);
        let _ = fs::remove_file(inbound_key_path);
    }

    #[test]
    fn grpcs_upstream_rejects_path_rewrite() {
        let CertifiedKey {
            cert: inbound_cert,
            key_pair: inbound_key,
        } = generate_simple_self_signed(vec!["grpcs.example.test".to_owned()])
            .expect("generate inbound certificate");
        let inbound_cert_path =
            write_temp_fixture("wsl-bridge-grpcs-rewrite-cert", "pem", &inbound_cert.pem());
        let inbound_key_path = write_temp_fixture(
            "wsl-bridge-grpcs-rewrite-key",
            "key",
            &inbound_key.serialize_pem(),
        );

        let engine = RuleEngine::new();
        let certificate_id = engine
            .create_proxy_certificate(CreateProxyCertificateRequest {
                name: "grpcs-rewrite-cert".to_owned(),
                source_type: ProxyCertificateSourceType::ManualUpload,
                cert_path: inbound_cert_path.display().to_string(),
                key_path: inbound_key_path.display().to_string(),
                domains: vec!["grpcs.example.test".to_owned()],
            })
            .expect("create inbound certificate");
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "grpcs-rewrite-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port: free_tcp_port(),
                protocol: ProxyProtocol::Https,
                tls_mode: ProxyTlsMode::ManualCert,
                cert_id: Some(certificate_id),
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: false,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id,
                server_names: vec![],
                path_prefix: None,
                is_default: true,
                enabled: true,
            })
            .expect("create route");

        let err = engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port: 50051,
                upstream_scheme: UpstreamScheme::Grpcs,
                path_rewrite_from: Some("/grpc".to_owned()),
                path_rewrite_to: Some("/".to_owned()),
                enabled: true,
            })
            .expect_err("grpcs upstream should reject path rewrite");

        assert!(
            matches!(err, EngineError::InvalidProxy(message) if message.contains("grpcs upstream does not support path rewrite yet"))
        );

        let _ = fs::remove_file(inbound_cert_path);
        let _ = fs::remove_file(inbound_key_path);
    }

    #[test]
    fn proxy_http_listener_tunnels_grpc_h2c_prior_knowledge() {
        let target_port = free_tcp_port();
        let listen_port = free_tcp_port();

        let server = thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", target_port)).expect("target bind");
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set timeout");
            let mut request = vec![0u8; HTTP2_PRIOR_KNOWLEDGE_PREFACE.len() + 4];
            stream.read_exact(&mut request).expect("read h2c preface");
            assert_eq!(
                &request[..HTTP2_PRIOR_KNOWLEDGE_PREFACE.len()],
                HTTP2_PRIOR_KNOWLEDGE_PREFACE
            );
            assert_eq!(&request[HTTP2_PRIOR_KNOWLEDGE_PREFACE.len()..], b"ping");
            stream.write_all(b"pong").expect("write response");
        });

        let engine = RuleEngine::new();
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "grpc-h2c-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: vec![],
                path_prefix: None,
                is_default: true,
                enabled: true,
            })
            .expect("create route");

        engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id: route_id.clone(),
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port,
                upstream_scheme: UpstreamScheme::Grpc,
                path_rewrite_from: None,
                path_rewrite_to: None,
                enabled: true,
            })
            .expect("create upstream");

        thread::sleep(Duration::from_millis(180));

        let mut client = TcpStream::connect(("127.0.0.1", listen_port)).expect("connect proxy");
        client
            .write_all(HTTP2_PRIOR_KNOWLEDGE_PREFACE)
            .expect("write h2c preface");
        client.write_all(b"ping").expect("write payload");
        client
            .shutdown(std::net::Shutdown::Write)
            .expect("shutdown write");

        let mut response = [0u8; 4];
        client.read_exact(&mut response).expect("read response");
        assert_eq!(&response, b"pong");

        thread::sleep(Duration::from_millis(120));

        let route_runtime = engine
            .list_proxy_route_runtime(&listener_id)
            .into_iter()
            .find(|item| item.route_id == route_id)
            .expect("route runtime");
        assert_eq!(route_runtime.hit_count, 1);
        assert_eq!(route_runtime.error_count, 0);

        drop(engine);
        let _ = server.join();
    }

    #[test]
    fn proxy_https_listener_accepts_grpcs_upstream_configuration() {
        let listen_port = free_tcp_port();

        let CertifiedKey {
            cert: inbound_cert,
            key_pair: inbound_key,
        } = generate_simple_self_signed(vec!["secure-grpcs.example.test".to_owned()])
            .expect("generate inbound certificate");
        let inbound_cert_path =
            write_temp_fixture("wsl-bridge-grpcs-listener-cert", "pem", &inbound_cert.pem());
        let inbound_key_path = write_temp_fixture(
            "wsl-bridge-grpcs-listener-key",
            "key",
            &inbound_key.serialize_pem(),
        );

        let engine = RuleEngine::new();
        let certificate_id = engine
            .create_proxy_certificate(CreateProxyCertificateRequest {
                name: "grpcs-listener-cert".to_owned(),
                source_type: ProxyCertificateSourceType::ManualUpload,
                cert_path: inbound_cert_path.display().to_string(),
                key_path: inbound_key_path.display().to_string(),
                domains: vec!["secure-grpcs.example.test".to_owned()],
            })
            .expect("create inbound certificate");
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "grpcs-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                protocol: ProxyProtocol::Https,
                tls_mode: ProxyTlsMode::ManualCert,
                cert_id: Some(certificate_id),
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: vec![],
                path_prefix: None,
                is_default: true,
                enabled: true,
            })
            .expect("create route");

        engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id: route_id.clone(),
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port: 443,
                upstream_scheme: UpstreamScheme::Grpcs,
                path_rewrite_from: None,
                path_rewrite_to: None,
                enabled: true,
            })
            .expect("create upstream");

        let status = wait_for_proxy_listener_state(&engine, &listener_id, RuntimeState::Running);
        assert_eq!(status.state, RuntimeState::Running);

        let _ = fs::remove_file(inbound_cert_path);
        let _ = fs::remove_file(inbound_key_path);
        drop(engine);
    }

    #[test]
    fn proxy_https_listener_tunnels_grpcs_prior_knowledge() {
        let _local_ca_lock = proxy_test_local_ca_lock();
        let (target_listener, target_port) = bind_test_tcp_listener();
        let listen_port = free_tcp_port();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("duration")
            .as_nanos();
        let log_dir = env::temp_dir().join(format!("wsl-bridge-grpcs-e2e-logs-{now}"));

        let (inbound_cert_pem, inbound_key_pem) =
            generate_trusted_local_ca_leaf(&["127.0.0.1"], "grpcs-inbound");
        let inbound_cert_path = write_temp_fixture(
            "wsl-bridge-grpcs-e2e-inbound-cert",
            "pem",
            &inbound_cert_pem,
        );
        let inbound_key_path =
            write_temp_fixture("wsl-bridge-grpcs-e2e-inbound-key", "key", &inbound_key_pem);

        let (upstream_cert_pem, upstream_key_pem) =
            generate_trusted_local_ca_leaf(&["127.0.0.1"], "grpcs-upstream");
        let upstream_tls_config = std::sync::Arc::new(create_test_tls_server_config(
            &upstream_cert_pem,
            &upstream_key_pem,
        ));

        let server = thread::spawn(move || {
            let (stream, _) = target_listener.accept().expect("accept");
            let connection =
                ServerConnection::new(upstream_tls_config).expect("create tls server connection");
            let mut stream = StreamOwned::new(connection, stream);
            stream
                .sock
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set timeout");
            let mut request = vec![0u8; HTTP2_PRIOR_KNOWLEDGE_PREFACE.len() + 4];
            stream.read_exact(&mut request).expect("read grpcs preface");
            assert_eq!(
                &request[..HTTP2_PRIOR_KNOWLEDGE_PREFACE.len()],
                HTTP2_PRIOR_KNOWLEDGE_PREFACE
            );
            assert_eq!(&request[HTTP2_PRIOR_KNOWLEDGE_PREFACE.len()..], b"ping");
            stream.write_all(b"pong").expect("write response");
            stream.flush().expect("flush response");
            stream.conn.send_close_notify();
            let _ = stream.flush();
        });

        let engine = RuleEngine::new_with_options_and_log_dir(EngineOptions::default(), &log_dir)
            .expect("create engine with logs");
        let certificate_id = engine
            .create_proxy_certificate(CreateProxyCertificateRequest {
                name: "grpcs-e2e-listener-cert".to_owned(),
                source_type: ProxyCertificateSourceType::ManualUpload,
                cert_path: inbound_cert_path.display().to_string(),
                key_path: inbound_key_path.display().to_string(),
                domains: vec!["127.0.0.1".to_owned()],
            })
            .expect("create inbound certificate");
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "grpcs-e2e-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                protocol: ProxyProtocol::Https,
                tls_mode: ProxyTlsMode::ManualCert,
                cert_id: Some(certificate_id),
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: vec![],
                path_prefix: None,
                is_default: true,
                enabled: true,
            })
            .expect("create route");

        let upstream_id = engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id: route_id.clone(),
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port,
                upstream_scheme: UpstreamScheme::Grpcs,
                path_rewrite_from: None,
                path_rewrite_to: None,
                enabled: true,
            })
            .expect("create upstream");

        let status = wait_for_proxy_listener_state(&engine, &listener_id, RuntimeState::Running);
        assert_eq!(status.state, RuntimeState::Running);

        let root_cert_path = proxy_test_local_ca_root_dir().join("root-ca.pem");
        let root_cert_pem = fs::read_to_string(&root_cert_path).expect("read root cert");
        let client_config = create_test_tls_client_config(&root_cert_pem);
        let outbound = TcpStream::connect(("127.0.0.1", listen_port)).expect("connect listener");
        outbound
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set read timeout");
        outbound
            .set_write_timeout(Some(Duration::from_secs(2)))
            .expect("set write timeout");
        let server_name = ServerName::IpAddress(std::net::IpAddr::from([127, 0, 0, 1]).into());
        let connection =
            ClientConnection::new(client_config, server_name).expect("create tls client");
        let mut client = StreamOwned::new(connection, outbound);
        let mut request = HTTP2_PRIOR_KNOWLEDGE_PREFACE.to_vec();
        request.extend_from_slice(b"ping");
        client.write_all(&request).expect("write request");
        if let Err(err) = client.flush() {
            thread::sleep(Duration::from_millis(120));
            let runtime = engine
                .get_proxy_runtime_status()
                .into_iter()
                .find(|item| item.listener_id == listener_id)
                .expect("runtime status");
            let error_log = fs::read_to_string(log_dir.join("error.log")).unwrap_or_default();
            panic!(
                "flush request failed: {err}; runtime_state={:?}; runtime_error={:?}; error_log={error_log}",
                runtime.state, runtime.last_error
            );
        }

        let mut response = [0u8; 4];
        client.read_exact(&mut response).expect("read response");
        assert_eq!(&response, b"pong");
        client.conn.send_close_notify();
        let _ = client.flush();

        thread::sleep(Duration::from_millis(120));

        let route_runtime = engine
            .list_proxy_route_runtime(&listener_id)
            .into_iter()
            .find(|item| item.route_id == route_id)
            .expect("route runtime");
        assert_eq!(route_runtime.hit_count, 1);
        assert_eq!(route_runtime.error_count, 0);

        let upstream_runtime = engine
            .list_proxy_upstream_runtime(&route_id)
            .into_iter()
            .find(|item| item.upstream_id == upstream_id)
            .expect("upstream runtime");
        assert_eq!(upstream_runtime.hit_count, 1);
        assert_eq!(upstream_runtime.error_count, 0);

        let _ = fs::remove_file(inbound_cert_path);
        let _ = fs::remove_file(inbound_key_path);
        let _ = fs::remove_dir_all(log_dir);
        drop(engine);
        let _ = server.join();
    }

    #[test]
    fn local_ca_certificate_can_be_generated_and_bound_to_https_listener() {
        let _local_ca_lock = proxy_test_local_ca_lock();
        let target_port = free_tcp_port();
        let listen_port = free_tcp_port();

        let engine = RuleEngine::new();
        let certificate_id = engine
            .create_proxy_certificate(CreateProxyCertificateRequest {
                name: "runtime-local-ca-cert".to_owned(),
                source_type: ProxyCertificateSourceType::LocalCa,
                cert_path: String::new(),
                key_path: String::new(),
                domains: vec!["local-ca.example.test".to_owned()],
            })
            .expect("create local ca certificate");

        let certificate = engine
            .list_proxy_certificates()
            .into_iter()
            .find(|item| item.id == certificate_id)
            .expect("local ca certificate");
        assert_eq!(certificate.source_type, ProxyCertificateSourceType::LocalCa);
        assert!(Path::new(&certificate.cert_path).exists());
        assert!(Path::new(&certificate.key_path).exists());

        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "https-local-ca-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                protocol: ProxyProtocol::Https,
                tls_mode: ProxyTlsMode::LocalCa,
                cert_id: Some(certificate_id.clone()),
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create https listener");

        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: vec!["local-ca.example.test".to_owned()],
                path_prefix: Some("/".to_owned()),
                is_default: false,
                enabled: true,
            })
            .expect("create route");

        engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port,
                upstream_scheme: UpstreamScheme::Http,
                path_rewrite_from: None,
                path_rewrite_to: None,
                enabled: true,
            })
            .expect("create upstream");

        let status = wait_for_proxy_listener_state(&engine, &listener_id, RuntimeState::Running);
        assert_eq!(status.state, RuntimeState::Running);
        assert!(status.last_error.is_none());

        let cert_path = certificate.cert_path.clone();
        let key_path = certificate.key_path.clone();
        engine
            .delete_proxy_certificate(&certificate_id)
            .expect_err("bound local ca certificate should not be deletable");
        drop(engine);
        assert!(Path::new(&cert_path).exists());
        assert!(Path::new(&key_path).exists());
    }

    #[test]
    fn deleting_bound_proxy_certificate_is_rejected() {
        let cert_path = write_temp_fixture(
            "wsl-bridge-cert-delete",
            "pem",
            "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n",
        );
        let key_path = write_temp_fixture(
            "wsl-bridge-key-delete",
            "key",
            "-----BEGIN PRIVATE KEY-----\nMIIB\n-----END PRIVATE KEY-----\n",
        );

        let engine = RuleEngine::new();
        let certificate_id = engine
            .create_proxy_certificate(CreateProxyCertificateRequest {
                name: "in-use-cert".to_owned(),
                source_type: ProxyCertificateSourceType::ManualUpload,
                cert_path: cert_path.display().to_string(),
                key_path: key_path.display().to_string(),
                domains: vec!["in-use.test".to_owned()],
            })
            .expect("create proxy certificate");

        engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "https-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port: free_tcp_port(),
                protocol: ProxyProtocol::Https,
                tls_mode: ProxyTlsMode::ManualCert,
                cert_id: Some(certificate_id.clone()),
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: false,
            })
            .expect("create https listener");

        let err = engine
            .delete_proxy_certificate(&certificate_id)
            .expect_err("delete bound certificate should fail");
        assert!(
            matches!(err, EngineError::InvalidProxy(message) if message.contains("currently used by a listener"))
        );

        let _ = fs::remove_file(cert_path);
        let _ = fs::remove_file(key_path);
    }

    #[test]
    fn migrated_tcp_rule_can_rollback_from_proxy() {
        let target_port = free_tcp_port();
        let listen_port = free_tcp_port();

        let engine = RuleEngine::new();
        let rule_id = engine
            .create_rule(CreateRuleRequest {
                rule: NewProxyRule {
                    name: "legacy-tcp-rollback".to_owned(),
                    rule_type: RuleType::TcpFwd,
                    listen_host: "127.0.0.1".to_owned(),
                    listen_port,
                    target_kind: TargetKind::Static,
                    target_ref: None,
                    target_host: Some("127.0.0.1".to_owned()),
                    target_port: Some(target_port),
                    bind_mode: BindMode::AllNics,
                    nic_id: None,
                    enabled: true,
                },
                firewall: None,
            })
            .expect("create rule");

        let migration = engine
            .migrate_rule_to_proxy(&rule_id)
            .expect("migrate rule");

        let rollback = engine
            .rollback_rule_migration(&rule_id)
            .expect("rollback migration");
        assert_eq!(rollback.status, RuleMigrationStatus::Rollbacked);
        assert_eq!(rollback.rule_id, rule_id);
        assert_eq!(rollback.proxy_listener_id, migration.proxy_listener_id);
        assert_eq!(rollback.proxy_route_id, migration.proxy_route_id);
        assert_eq!(rollback.proxy_upstream_id, migration.proxy_upstream_id);
        assert_eq!(
            rollback.original_rule_enabled,
            migration.original_rule_enabled
        );
        assert!(rollback.rollbacked_at.is_some());

        let restored_rule = engine
            .list_rules()
            .into_iter()
            .find(|item| item.id == rule_id)
            .expect("restored legacy rule");
        assert!(restored_rule.enabled);

        assert!(engine
            .list_proxy_listeners()
            .into_iter()
            .all(|item| item.id != migration.proxy_listener_id));
        assert!(engine
            .list_rule_migrations()
            .into_iter()
            .find(|item| item.rule_id == rule_id)
            .is_some_and(|item| item.status == RuleMigrationStatus::Rollbacked));

        assert!(matches!(
            engine.list_proxy_routes(&migration.proxy_listener_id),
            Err(EngineError::ProxyListenerNotFound(id)) if id == migration.proxy_listener_id
        ));

        assert!(matches!(
            engine.list_proxy_upstreams(&migration.proxy_route_id),
            Err(EngineError::ProxyRouteNotFound(id)) if id == migration.proxy_route_id
        ));

        let runtime = engine
            .get_runtime_status()
            .into_iter()
            .find(|item| item.rule_id == rule_id)
            .expect("runtime record");
        assert_eq!(runtime.state, RuntimeState::Stopped);
        assert!(runtime.last_error.is_none());
        assert!(runtime.last_apply_at.is_some());
    }

    #[test]
    fn proxy_runtime_status_reports_running_listener() {
        let target_port = free_tcp_port();
        let listen_port = free_tcp_port();

        let engine = RuleEngine::new();
        let listener_id = engine
            .create_proxy_listener(CreateProxyListenerRequest {
                name: "runtime-http-listener".to_owned(),
                listen_host: "127.0.0.1".to_owned(),
                listen_port,
                protocol: ProxyProtocol::Http,
                tls_mode: ProxyTlsMode::Disabled,
                cert_id: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            })
            .expect("create listener");
        let route_id = engine
            .create_proxy_route(CreateProxyRouteRequest {
                listener_id: listener_id.clone(),
                server_names: vec!["runtime.example.com".to_owned()],
                path_prefix: None,
                is_default: false,
                enabled: true,
            })
            .expect("create route");
        engine
            .create_proxy_upstream(CreateProxyUpstreamRequest {
                route_id,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: Some("127.0.0.1".to_owned()),
                target_port,
                upstream_scheme: UpstreamScheme::Http,
                path_rewrite_from: None,
                path_rewrite_to: None,
                enabled: true,
            })
            .expect("create upstream");

        thread::sleep(Duration::from_millis(120));

        let status = engine
            .get_proxy_runtime_status()
            .into_iter()
            .find(|item| item.listener_id == listener_id)
            .expect("runtime status");
        assert_eq!(status.state, RuntimeState::Running);
        assert!(status.last_error.is_none());

        drop(engine);
    }

    #[test]
    fn http_proxy_connect_works() {
        let target_port = free_tcp_port();
        let proxy_port = free_tcp_port();

        let server = thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", target_port)).expect("target bind");
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 4];
            stream.read_exact(&mut buf).expect("read");
            stream.write_all(&buf).expect("write");
        });

        let engine = RuleEngine::new();
        let req = CreateRuleRequest {
            rule: NewProxyRule {
                name: "http-proxy-connect".to_owned(),
                rule_type: RuleType::HttpProxy,
                listen_host: "127.0.0.1".to_owned(),
                listen_port: proxy_port,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: None,
                target_port: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            },
            firewall: None,
        };
        let _id = engine.create_rule(req).expect("create rule");
        let result = engine.apply_rules();
        assert_eq!(result.failed.len(), 0);

        let mut client = TcpStream::connect(("127.0.0.1", proxy_port)).expect("connect proxy");
        let connect_req = format!(
            "CONNECT 127.0.0.1:{target_port} HTTP/1.1\r\nHost: 127.0.0.1:{target_port}\r\n\r\n"
        );
        client
            .write_all(connect_req.as_bytes())
            .expect("send connect");
        let mut resp = [0u8; 128];
        let n = client.read(&mut resp).expect("read connect resp");
        let text = String::from_utf8_lossy(&resp[..n]);
        assert!(text.contains("200"));

        client.write_all(b"ping").expect("send payload");
        let mut echoed = [0u8; 4];
        client.read_exact(&mut echoed).expect("read echoed");
        assert_eq!(&echoed, b"ping");

        let _ = engine.stop_rules();
        let _ = server.join();
    }

    #[test]
    fn socks5_connect_works() {
        let target_port = free_tcp_port();
        let proxy_port = free_tcp_port();

        let server = thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", target_port)).expect("target bind");
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 4];
            stream.read_exact(&mut buf).expect("read");
            stream.write_all(&buf).expect("write");
        });

        let engine = RuleEngine::new();
        let req = CreateRuleRequest {
            rule: NewProxyRule {
                name: "socks5-connect".to_owned(),
                rule_type: RuleType::Socks5Proxy,
                listen_host: "127.0.0.1".to_owned(),
                listen_port: proxy_port,
                target_kind: TargetKind::Static,
                target_ref: None,
                target_host: None,
                target_port: None,
                bind_mode: BindMode::AllNics,
                nic_id: None,
                enabled: true,
            },
            firewall: None,
        };
        let _id = engine.create_rule(req).expect("create rule");
        let result = engine.apply_rules();
        assert_eq!(result.failed.len(), 0);

        let mut client = TcpStream::connect(("127.0.0.1", proxy_port)).expect("connect proxy");
        client
            .write_all(&[0x05, 0x01, 0x00])
            .expect("send greeting");
        let mut greeting_resp = [0u8; 2];
        client
            .read_exact(&mut greeting_resp)
            .expect("read greeting response");
        assert_eq!(greeting_resp, [0x05, 0x00]);

        let mut connect_req = vec![0x05, 0x01, 0x00, 0x01, 127, 0, 0, 1];
        connect_req.extend_from_slice(&target_port.to_be_bytes());
        client.write_all(&connect_req).expect("send connect");

        let mut connect_resp = [0u8; 10];
        client
            .read_exact(&mut connect_resp)
            .expect("read connect response");
        assert_eq!(connect_resp[0], 0x05);
        assert_eq!(connect_resp[1], 0x00);

        client.write_all(b"pong").expect("send payload");
        let mut echoed = [0u8; 4];
        client.read_exact(&mut echoed).expect("read echoed");
        assert_eq!(&echoed, b"pong");

        let _ = engine.stop_rules();
        let _ = server.join();
    }

    #[test]
    fn hosts_bootstrap_save_copy_and_activate_work() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("duration")
            .as_nanos();
        let db_path = env::temp_dir().join(format!("wsl-bridge-hosts-{now}.db"));
        let hosts_path = env::temp_dir().join(format!("wsl-bridge-hosts-{now}.txt"));
        fs::write(
            &hosts_path,
            "127.0.0.1 localhost api.local # local dev\n::1 ipv6.local\n",
        )
        .expect("write seed hosts");

        let engine = RuleEngine::with_sqlite(&db_path).expect("sqlite engine");
        let group = engine
            .bootstrap_default_hosts_group_from_path(&hosts_path)
            .expect("bootstrap");
        assert_eq!(group.name, "default");

        let entries = engine
            .list_hosts_entries(&group.id)
            .expect("list default entries");
        assert_eq!(entries.len(), 3);

        engine
            .update_hosts_group(
                &group.id,
                UpdateHostsGroupRequest {
                    name: "renamed-default".to_owned(),
                    description: Some("renamed".to_owned()),
                },
            )
            .expect("rename default group");
        let bootstrapped_again = engine
            .bootstrap_default_hosts_group_from_path(&hosts_path)
            .expect("bootstrap after rename");
        assert_eq!(bootstrapped_again.id, group.id);
        assert_eq!(bootstrapped_again.name, "renamed-default");
        assert_eq!(engine.list_hosts_groups().len(), 1);

        engine
            .save_hosts_entries(SaveHostsEntriesRequest {
                group_id: group.id.clone(),
                entries: vec![HostsEntryInput {
                    id: entries.first().map(|item| item.id.clone()),
                    ip: "127.0.0.1".to_owned(),
                    domain: "edited.local".to_owned(),
                    comment: Some("edited".to_owned()),
                    enabled: true,
                    order_index: 0,
                }],
            })
            .expect("save entries");

        let copied_id = engine
            .copy_hosts_group(CopyHostsGroupRequest {
                source_group_id: group.id.clone(),
                name: "copy".to_owned(),
                description: None,
            })
            .expect("copy group");
        let copied_entries = engine
            .list_hosts_entries(&copied_id)
            .expect("list copied entries");
        assert_eq!(copied_entries.len(), 1);
        assert_eq!(copied_entries[0].domain, "edited.local");

        engine
            .activate_hosts_group_to_path(&copied_id, &hosts_path)
            .expect("activate");
        let rendered = fs::read_to_string(&hosts_path).expect("read activated hosts");
        assert!(rendered.contains("127.0.0.1 edited.local # edited"));

        let _ = fs::remove_file(db_path);
        let _ = fs::remove_file(hosts_path);
    }

    #[test]
    fn hosts_import_export_delete_and_validation_work() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("duration")
            .as_nanos();
        let db_path = env::temp_dir().join(format!("wsl-bridge-hosts-import-{now}.db"));
        let hosts_path = env::temp_dir().join(format!("wsl-bridge-hosts-active-{now}.txt"));
        let import_path = env::temp_dir().join(format!("wsl-bridge-hosts-source-{now}.txt"));
        let export_path = env::temp_dir().join(format!("wsl-bridge-hosts-export-{now}.txt"));

        fs::write(&hosts_path, "127.0.0.1 localhost\n").expect("write seed hosts");
        fs::write(
            &import_path,
            "127.0.0.1 a.local b.local # local aliases\n::1 ipv6.local\n",
        )
        .expect("write import hosts");

        let engine = RuleEngine::with_sqlite(&db_path).expect("sqlite engine");
        let default_group = engine
            .bootstrap_default_hosts_group_from_path(&hosts_path)
            .expect("bootstrap");

        let imported_id = engine
            .import_hosts_group(ImportHostsGroupRequest {
                name: Some("imported".to_owned()),
                description: Some("from file".to_owned()),
                path: import_path.display().to_string(),
            })
            .expect("import hosts group");

        let imported_entries = engine
            .list_hosts_entries(&imported_id)
            .expect("list imported entries");
        assert_eq!(imported_entries.len(), 3);
        assert_eq!(imported_entries[0].domain, "a.local");
        assert_eq!(imported_entries[1].domain, "b.local");
        assert_eq!(imported_entries[2].ip, "::1");

        engine
            .export_hosts_group(ExportHostsGroupRequest {
                group_id: imported_id.clone(),
                path: export_path.display().to_string(),
            })
            .expect("export hosts group");
        let exported = fs::read_to_string(&export_path).expect("read exported hosts");
        assert_eq!(
            exported,
            "127.0.0.1 a.local # local aliases\n127.0.0.1 b.local # local aliases\n::1 ipv6.local\n"
        );

        let manual_group_id = engine
            .create_hosts_group(CreateHostsGroupRequest {
                name: "manual".to_owned(),
                description: Some("temp".to_owned()),
            })
            .expect("create manual group");
        engine
            .delete_hosts_group(&manual_group_id)
            .expect("delete manual group");
        assert!(engine
            .list_hosts_groups()
            .into_iter()
            .all(|group| group.id != manual_group_id));

        let invalid_save = engine.save_hosts_entries(SaveHostsEntriesRequest {
            group_id: imported_id.clone(),
            entries: vec![HostsEntryInput {
                id: None,
                ip: "not-an-ip".to_owned(),
                domain: "broken.local".to_owned(),
                comment: None,
                enabled: true,
                order_index: 0,
            }],
        });
        assert!(matches!(invalid_save, Err(EngineError::InvalidHosts(_))));

        engine
            .activate_hosts_group_to_path(&imported_id, &hosts_path)
            .expect("activate imported group");
        let groups_after_import_activate = engine.list_hosts_groups();
        assert_eq!(
            groups_after_import_activate
                .iter()
                .filter(|group| group.is_active)
                .count(),
            1
        );
        assert!(groups_after_import_activate
            .iter()
            .any(|group| group.id == imported_id && group.is_active));
        assert!(engine.delete_hosts_group(&imported_id).is_err());

        engine
            .activate_hosts_group_to_path(&default_group.id, &hosts_path)
            .expect("reactivate default group");
        let groups_after_default_activate = engine.list_hosts_groups();
        assert_eq!(
            groups_after_default_activate
                .iter()
                .filter(|group| group.is_active)
                .count(),
            1
        );
        assert!(groups_after_default_activate
            .iter()
            .any(|group| group.id == default_group.id && group.is_active));
        assert!(groups_after_default_activate
            .iter()
            .any(|group| group.id == imported_id && !group.is_active));

        engine
            .delete_hosts_group(&imported_id)
            .expect("delete imported group");
        assert!(engine.list_hosts_entries(&imported_id).is_err());

        let _ = fs::remove_file(db_path);
        let _ = fs::remove_file(hosts_path);
        let _ = fs::remove_file(import_path);
        let _ = fs::remove_file(export_path);
    }
}
