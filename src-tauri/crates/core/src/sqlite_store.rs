use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use wsl_bridge_shared::{
    AuditLog, BindMode, FirewallPolicy, ProxyRule, RuleType, RuntimeState, RuntimeStatusItem,
    TargetKind,
};

use crate::engine::EngineError;

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub rules: HashMap<String, ProxyRule>,
    pub firewalls: HashMap<String, FirewallPolicy>,
    pub runtime: HashMap<String, RuntimeStatusItem>,
    pub logs: Vec<AuditLog>,
    pub log_seq: u64,
}

#[derive(Debug)]
pub struct SqliteStore {
    path: PathBuf,
    conn: Mutex<Connection>,
}

impl SqliteStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, EngineError> {
        let path = path.as_ref().to_path_buf();
        let conn = Connection::open(&path).map_err(|err| EngineError::Storage(err.to_string()))?;
        let store = Self {
            path,
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_snapshot(&self) -> Result<Snapshot, EngineError> {
        let conn = self.conn.lock();

        let mut rules = HashMap::new();
        let mut firewalls = HashMap::new();
        let mut runtime = HashMap::new();
        let mut logs = Vec::new();

        {
            let mut stmt = conn
                .prepare(
                    "SELECT id,name,type,listen_host,listen_port,target_kind,target_ref,target_host,target_port,bind_mode,nic_id,enabled,created_at,updated_at FROM proxy_rule",
                )
                .map_err(|err| EngineError::Storage(err.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(ProxyRule {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        rule_type: rule_type_from_db(&row.get::<_, String>(2)?).map_err(db_err)?,
                        listen_host: row.get(3)?,
                        listen_port: row.get(4)?,
                        target_kind: target_kind_from_db(&row.get::<_, String>(5)?)
                            .map_err(db_err)?,
                        target_ref: row.get(6)?,
                        target_host: row.get(7)?,
                        target_port: row.get(8)?,
                        bind_mode: bind_mode_from_db(&row.get::<_, String>(9)?).map_err(db_err)?,
                        nic_id: row.get(10)?,
                        enabled: row.get(11)?,
                        created_at: from_millis(row.get(12)?).map_err(db_err)?,
                        updated_at: from_millis(row.get(13)?).map_err(db_err)?,
                    })
                })
                .map_err(|err| EngineError::Storage(err.to_string()))?;
            for row in rows {
                let rule = row.map_err(|err| EngineError::Storage(err.to_string()))?;
                rules.insert(rule.id.clone(), rule);
            }
        }

        {
            let mut stmt = conn
                .prepare(
                    "SELECT rule_id,allow_domain,allow_private,allow_public,direction,action FROM firewall_policy",
                )
                .map_err(|err| EngineError::Storage(err.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(FirewallPolicy {
                        rule_id: row.get(0)?,
                        allow_domain: row.get(1)?,
                        allow_private: row.get(2)?,
                        allow_public: row.get(3)?,
                        direction: row.get(4)?,
                        action: row.get(5)?,
                    })
                })
                .map_err(|err| EngineError::Storage(err.to_string()))?;
            for row in rows {
                let policy = row.map_err(|err| EngineError::Storage(err.to_string()))?;
                firewalls.insert(policy.rule_id.clone(), policy);
            }
        }

        {
            let mut stmt = conn
                .prepare("SELECT rule_id,state,last_error,last_apply_at FROM runtime_state")
                .map_err(|err| EngineError::Storage(err.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    let last_apply_ms = row.get::<_, Option<i64>>(3)?;
                    Ok(RuntimeStatusItem {
                        rule_id: row.get(0)?,
                        state: runtime_state_from_db(&row.get::<_, String>(1)?).map_err(db_err)?,
                        last_error: row.get(2)?,
                        last_apply_at: match last_apply_ms {
                            Some(value) => Some(from_millis(value).map_err(db_err)?),
                            None => None,
                        },
                    })
                })
                .map_err(|err| EngineError::Storage(err.to_string()))?;
            for row in rows {
                let item = row.map_err(|err| EngineError::Storage(err.to_string()))?;
                runtime.insert(item.rule_id.clone(), item);
            }
        }

        {
            let mut stmt = conn
                .prepare("SELECT id,time,level,module,event,detail FROM audit_log ORDER BY id ASC")
                .map_err(|err| EngineError::Storage(err.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(AuditLog {
                        id: row.get(0)?,
                        time: from_millis(row.get(1)?).map_err(db_err)?,
                        level: row.get(2)?,
                        module: row.get(3)?,
                        event: row.get(4)?,
                        detail: row.get(5)?,
                    })
                })
                .map_err(|err| EngineError::Storage(err.to_string()))?;
            for row in rows {
                logs.push(row.map_err(|err| EngineError::Storage(err.to_string()))?);
            }
        }

        let log_seq = logs.last().map(|item| item.id).unwrap_or(0);

        Ok(Snapshot {
            rules,
            firewalls,
            runtime,
            logs,
            log_seq,
        })
    }

    pub fn save_snapshot(&self, snapshot: &Snapshot) -> Result<(), EngineError> {
        let mut conn = self.conn.lock();
        let tx = conn
            .transaction()
            .map_err(|err| EngineError::Storage(err.to_string()))?;

        tx.execute("DELETE FROM proxy_rule", [])
            .map_err(|err| EngineError::Storage(err.to_string()))?;
        tx.execute("DELETE FROM firewall_policy", [])
            .map_err(|err| EngineError::Storage(err.to_string()))?;
        tx.execute("DELETE FROM runtime_state", [])
            .map_err(|err| EngineError::Storage(err.to_string()))?;
        tx.execute("DELETE FROM audit_log", [])
            .map_err(|err| EngineError::Storage(err.to_string()))?;

        let mut rules = snapshot.rules.values().cloned().collect::<Vec<_>>();
        rules.sort_by(|a, b| a.id.cmp(&b.id));
        for rule in rules {
            tx.execute(
                "INSERT INTO proxy_rule (id,name,type,listen_host,listen_port,target_kind,target_ref,target_host,target_port,bind_mode,nic_id,enabled,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                params![
                    rule.id,
                    rule.name,
                    rule_type_to_db(rule.rule_type),
                    rule.listen_host,
                    rule.listen_port,
                    target_kind_to_db(rule.target_kind),
                    rule.target_ref,
                    rule.target_host,
                    rule.target_port,
                    bind_mode_to_db(rule.bind_mode),
                    rule.nic_id,
                    rule.enabled,
                    rule.created_at.timestamp_millis(),
                    rule.updated_at.timestamp_millis(),
                ],
            )
            .map_err(|err| EngineError::Storage(err.to_string()))?;
        }

        let mut policies = snapshot.firewalls.values().cloned().collect::<Vec<_>>();
        policies.sort_by(|a, b| a.rule_id.cmp(&b.rule_id));
        for policy in policies {
            tx.execute(
                "INSERT INTO firewall_policy (rule_id,allow_domain,allow_private,allow_public,direction,action) VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    policy.rule_id,
                    policy.allow_domain,
                    policy.allow_private,
                    policy.allow_public,
                    policy.direction,
                    policy.action,
                ],
            )
            .map_err(|err| EngineError::Storage(err.to_string()))?;
        }

        let mut statuses = snapshot.runtime.values().cloned().collect::<Vec<_>>();
        statuses.sort_by(|a, b| a.rule_id.cmp(&b.rule_id));
        for item in statuses {
            tx.execute(
                "INSERT INTO runtime_state (rule_id,state,last_error,last_apply_at) VALUES (?1,?2,?3,?4)",
                params![
                    item.rule_id,
                    runtime_state_to_db(item.state),
                    item.last_error,
                    item.last_apply_at.map(|dt| dt.timestamp_millis()),
                ],
            )
            .map_err(|err| EngineError::Storage(err.to_string()))?;
        }

        for item in &snapshot.logs {
            tx.execute(
                "INSERT INTO audit_log (id,time,level,module,event,detail) VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    item.id,
                    item.time.timestamp_millis(),
                    item.level,
                    item.module,
                    item.event,
                    item.detail,
                ],
            )
            .map_err(|err| EngineError::Storage(err.to_string()))?;
        }

        tx.commit()
            .map_err(|err| EngineError::Storage(err.to_string()))?;
        Ok(())
    }

    fn init_schema(&self) -> Result<(), EngineError> {
        let conn = self.conn.lock();
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;

            CREATE TABLE IF NOT EXISTS proxy_rule (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                type TEXT NOT NULL,
                listen_host TEXT NOT NULL,
                listen_port INTEGER NOT NULL,
                target_kind TEXT NOT NULL,
                target_ref TEXT NULL,
                target_host TEXT NULL,
                target_port INTEGER NULL,
                bind_mode TEXT NOT NULL,
                nic_id TEXT NULL,
                enabled INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS firewall_policy (
                rule_id TEXT PRIMARY KEY,
                allow_domain INTEGER NOT NULL,
                allow_private INTEGER NOT NULL,
                allow_public INTEGER NOT NULL,
                direction TEXT NOT NULL,
                action TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS runtime_state (
                rule_id TEXT PRIMARY KEY,
                state TEXT NOT NULL,
                last_error TEXT NULL,
                last_apply_at INTEGER NULL
            );

            CREATE TABLE IF NOT EXISTS audit_log (
                id INTEGER PRIMARY KEY,
                time INTEGER NOT NULL,
                level TEXT NOT NULL,
                module TEXT NOT NULL,
                event TEXT NOT NULL,
                detail TEXT NOT NULL
            );
            "#,
        )
        .map_err(|err| EngineError::Storage(err.to_string()))?;
        Ok(())
    }
}

fn from_millis(value: i64) -> Result<chrono::DateTime<Utc>, String> {
    Utc.timestamp_millis_opt(value)
        .single()
        .ok_or_else(|| format!("invalid timestamp millis: {value}"))
}

fn db_err(err: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::<std::io::Error>::new(std::io::Error::new(std::io::ErrorKind::InvalidData, err)),
    )
}

fn rule_type_to_db(value: RuleType) -> &'static str {
    match value {
        RuleType::TcpFwd => "tcp_fwd",
        RuleType::UdpFwd => "udp_fwd",
        RuleType::HttpProxy => "http_proxy",
        RuleType::Socks5Proxy => "socks5_proxy",
    }
}

fn rule_type_from_db(value: &str) -> Result<RuleType, String> {
    match value {
        "tcp_fwd" => Ok(RuleType::TcpFwd),
        "udp_fwd" => Ok(RuleType::UdpFwd),
        "http_proxy" => Ok(RuleType::HttpProxy),
        "socks5_proxy" => Ok(RuleType::Socks5Proxy),
        _ => Err(format!("invalid rule type: {value}")),
    }
}

fn target_kind_to_db(value: TargetKind) -> &'static str {
    match value {
        TargetKind::Wsl => "wsl",
        TargetKind::Hyperv => "hyperv",
        TargetKind::Static => "static",
    }
}

fn target_kind_from_db(value: &str) -> Result<TargetKind, String> {
    match value {
        "wsl" => Ok(TargetKind::Wsl),
        "hyperv" => Ok(TargetKind::Hyperv),
        "static" => Ok(TargetKind::Static),
        _ => Err(format!("invalid target kind: {value}")),
    }
}

fn bind_mode_to_db(value: BindMode) -> &'static str {
    match value {
        BindMode::SingleNic => "single_nic",
        BindMode::AllNics => "all_nics",
    }
}

fn bind_mode_from_db(value: &str) -> Result<BindMode, String> {
    match value {
        "single_nic" => Ok(BindMode::SingleNic),
        "all_nics" => Ok(BindMode::AllNics),
        _ => Err(format!("invalid bind mode: {value}")),
    }
}

fn runtime_state_to_db(value: RuntimeState) -> &'static str {
    match value {
        RuntimeState::Running => "running",
        RuntimeState::Stopped => "stopped",
        RuntimeState::Error => "error",
    }
}

fn runtime_state_from_db(value: &str) -> Result<RuntimeState, String> {
    match value {
        "running" => Ok(RuntimeState::Running),
        "stopped" => Ok(RuntimeState::Stopped),
        "error" => Ok(RuntimeState::Error),
        _ => Err(format!("invalid runtime state: {value}")),
    }
}
