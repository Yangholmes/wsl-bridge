use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::path::Path;

use chrono::Utc;
use parking_lot::{Mutex, RwLock};
use thiserror::Error;
use tracing::warn;
use uuid::Uuid;
use wsl_bridge_shared::{
    ApplyRulesResult, AuditLog, BindMode, CreateRuleRequest, FirewallPolicy, NewProxyRule, ProxyRule,
    RulePatch, RuleType, RuntimeState, RuntimeStatusItem, StopRulesResult, TailLogsResult,
    TopologySnapshot,
};

use crate::firewall::{FirewallMode, FirewallRuleRuntime, apply_firewall, cleanup_firewall};
use crate::forwarder::{ForwarderHandle, ForwarderKind, spawn as spawn_forwarder};
use crate::sqlite_store::{Snapshot, SqliteStore};
use crate::topology::{list_adapters, resolve_nic_ip};

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
        TopologySnapshot {
            adapters: list_adapters(),
            wsl: Vec::new(),
            hyperv: Vec::new(),
            timestamp: Utc::now(),
        }
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
                _ => {
                    let err = format!("rule type {:?} is not in M1 runtime scope", rule.rule_type);
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

            let target_addr = match self.resolve_target_addr(&rule) {
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
                    rule.id, listen_addr, target_addr
                ),
            );

            new_active.insert(
                rule.id.clone(),
                ActiveRuleRuntime {
                    forwarder,
                    firewall: firewall_runtime,
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
        let target_host = rule.target_host.as_ref().ok_or_else(|| {
            "target_host is required in M1; dynamic target resolution is planned in M2".to_owned()
        })?;

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
        if rule.bind_mode == BindMode::SingleNic && rule.nic_id.as_deref().unwrap_or("").is_empty() {
            return Err(EngineError::InvalidRule(
                "single_nic mode requires nic_id".to_owned(),
            ));
        }
        if rule.rule_type == RuleType::TcpFwd || rule.rule_type == RuleType::UdpFwd {
            if rule.target_host.is_none() {
                return Err(EngineError::InvalidRule(
                    "target_host is required for tcp/udp forwarding in M1".to_owned(),
                ));
            }
            if rule.target_port.is_none() {
                return Err(EngineError::InvalidRule(
                    "target_port is required for tcp/udp forwarding".to_owned(),
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
        BindMode, CreateRuleRequest, NewProxyRule, RulePatch, RuleType, TargetKind,
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
    fn sqlite_roundtrip() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("duration")
            .as_nanos();
        let path = env::temp_dir().join(format!("wsl-bridge-test-{now}.db"));

        {
            let engine = RuleEngine::with_sqlite(&path).expect("sqlite engine");
            let id = engine.create_rule(test_rule("persisted", 38150)).expect("create");
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
}
