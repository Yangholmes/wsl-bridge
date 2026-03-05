use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::path::Path;

use chrono::Utc;
use parking_lot::{Mutex, RwLock};
use thiserror::Error;
use tracing::warn;
use uuid::Uuid;
use wsl_bridge_shared::{
    ApplyRulesResult, AuditLog, BindMode, CreateRuleRequest, FirewallPolicy, LogQueryRequest,
    LogQueryResult, NewProxyRule, ProxyRule, RuleLogStatsItem, RuleLogStatsRequest, RulePatch,
    RuleType, RuntimeState, RuntimeStatusItem, StopRulesResult, TailLogsResult, TargetKind,
    TopologySnapshot,
};

use crate::firewall::{apply_firewall, cleanup_firewall, FirewallMode, FirewallRuleRuntime};
use crate::forwarder::{spawn as spawn_forwarder, ForwarderHandle, ForwarderKind};
use crate::sqlite_store::{Snapshot, SqliteStore};
use crate::topology::{
    debug_hyperv_probe, list_adapters, list_wsl_instances, resolve_dynamic_target_host,
    resolve_nic_ip, scan_hyperv, HyperVProbeDebug,
};

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("rule not found: {0}")]
    RuleNotFound(String),
    #[error("invalid rule: {0}")]
    InvalidRule(String),
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
    logs: Vec<AuditLog>,
    log_seq: u64,
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
    sqlite: Option<SqliteStore>,
    options: EngineOptions,
    active: Mutex<HashMap<String, ActiveRuleRuntime>>,
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new_with_options(EngineOptions::default())
    }
}

impl Drop for RuleEngine {
    fn drop(&mut self) {
        self.stop_all_active_rules();
    }
}

impl RuleEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_with_options(options: EngineOptions) -> Self {
        Self {
            store: RwLock::new(EngineStore::default()),
            sqlite: None,
            options,
            active: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_sqlite(path: impl AsRef<Path>) -> Result<Self, EngineError> {
        Self::with_sqlite_and_options(path, EngineOptions::default())
    }

    pub fn with_sqlite_and_options(
        path: impl AsRef<Path>,
        options: EngineOptions,
    ) -> Result<Self, EngineError> {
        let sqlite = SqliteStore::open(path)?;
        let snapshot = sqlite.load_snapshot()?;
        let store = RwLock::new(EngineStore {
            rules: snapshot.rules,
            firewalls: snapshot.firewalls,
            runtime: snapshot.runtime,
            logs: snapshot.logs,
            log_seq: snapshot.log_seq,
        });
        Ok(Self {
            store,
            sqlite: Some(sqlite),
            options,
            active: Mutex::new(HashMap::new()),
        })
    }

    pub fn sqlite_path(&self) -> Option<&Path> {
        self.sqlite.as_ref().map(|store| store.path())
    }

    pub fn scan_topology(&self) -> TopologySnapshot {
        let hyperv = scan_hyperv();
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

    pub fn update_rule(&self, id: &str, patch: RulePatch) -> Result<(), EngineError> {
        if let Some(active) = self.active.lock().remove(id) {
            self.stop_active_runtime(id, active);
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

            let forwarder = match spawn_forwarder(forward_kind, listen_addr, target_addr) {
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
            let mut store = self.store.write();
            append_log(
                &mut store,
                "warn",
                "engine",
                "topology_changed",
                &changed.join(" | "),
            );
            self.persist_store(&store);
        }

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

    fn stop_all_active_rules(&self) {
        let old_active = {
            let mut active = self.active.lock();
            std::mem::take(&mut *active)
        };
        for (rule_id, runtime) in old_active {
            self.stop_active_runtime(&rule_id, runtime);
        }
    }

    fn stop_active_runtime(&self, rule_id: &str, runtime: ActiveRuleRuntime) {
        runtime.forwarder.stop_and_join();
        if let Err(err) = cleanup_firewall(self.options.firewall_mode, &runtime.firewall.names) {
            let mut store = self.store.write();
            append_log(
                &mut store,
                "warn",
                "engine",
                "firewall_cleanup_failed",
                &format!("rule_id={rule_id},reason={err}"),
            );
            self.persist_store(&store);
        }
    }

    fn persist_store(&self, store: &EngineStore) {
        let Some(sqlite) = &self.sqlite else {
            return;
        };
        let snapshot = Snapshot {
            rules: store.rules.clone(),
            firewalls: store.firewalls.clone(),
            runtime: store.runtime.clone(),
            logs: store.logs.clone(),
            log_seq: store.log_seq,
        };
        if let Err(err) = sqlite.save_snapshot(&snapshot) {
            warn!("persist snapshot failed: {err}");
        }
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

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream, UdpSocket};
    use std::thread;
    use std::time::Duration;
    use std::time::{SystemTime, UNIX_EPOCH};

    use wsl_bridge_shared::{
        BindMode, CreateRuleRequest, LogQueryRequest, NewProxyRule, RuleLogStatsRequest, RulePatch,
        RuleType, TargetKind,
    };

    use super::RuleEngine;

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
    fn apply_detects_conflict() {
        let engine = RuleEngine::new();
        let _id1 = engine.create_rule(test_rule("a", 38100)).expect("create a");
        let _id2 = engine.create_rule(test_rule("b", 38100)).expect("create b");
        let result = engine.apply_rules();
        assert_eq!(result.applied, 1);
        assert_eq!(result.failed.len(), 1);
        let _ = engine.stop_rules();
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
        let listen_port = free_udp_port();

        let server = thread::spawn(move || {
            let socket = UdpSocket::bind(("127.0.0.1", target_port)).expect("udp target bind");
            let mut buf = [0u8; 1024];
            let (len, src) = socket.recv_from(&mut buf).expect("recv");
            socket.send_to(&buf[..len], src).expect("send");
        });

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
}
