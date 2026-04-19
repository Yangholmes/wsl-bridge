use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use wsl_bridge_shared::{
    QueryTrafficStatsRequest, QueryTrafficStatsResult, TrafficSample, TrafficStatsInterval,
    TrafficStatsPoint, TrafficWindowData,
};

use crate::app_logs::{AccessLogEntry, AppLogger, ErrorLogEntry};
use crate::sqlite_store::SqliteStore;

const MAX_WINDOW_SECONDS: i64 = 120;

#[derive(Debug, Clone)]
pub struct TrafficRecorder {
    rule_id: String,
    tracker: Arc<TrafficTracker>,
    logger: Arc<AppLogger>,
}

impl TrafficRecorder {
    pub fn new(
        rule_id: impl Into<String>,
        tracker: Arc<TrafficTracker>,
        logger: Arc<AppLogger>,
    ) -> Self {
        Self {
            rule_id: rule_id.into(),
            tracker,
            logger,
        }
    }

    pub fn rule_id(&self) -> &str {
        &self.rule_id
    }

    pub fn record(
        &self,
        bytes_in: u64,
        bytes_out: u64,
        connections: u64,
        requests: u64,
        duration_ms: u64,
    ) {
        self.tracker.record(
            &self.rule_id,
            Utc::now(),
            bytes_in,
            bytes_out,
            connections,
            requests,
            duration_ms,
        );
    }

    pub fn log_access(&self, entry: AccessLogEntry) {
        self.logger.log_access(entry);
    }

    pub fn log_error(&self, entry: ErrorLogEntry) {
        let entry = if entry.rule_id.is_some() {
            entry
        } else {
            entry.with_rule_id(self.rule_id.clone())
        };
        self.logger.log_error(entry);
    }
}

#[derive(Debug, Clone)]
pub struct PersistedTrafficStat {
    pub rule_id: String,
    pub time_bucket: i64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub connections: u64,
    pub requests: u64,
    pub total_duration_ms: u64,
    pub avg_duration_ms: u64,
    pub created_at: i64,
}

#[derive(Debug, Default)]
pub struct TrafficTracker {
    sqlite: Option<Arc<SqliteStore>>,
    inner: Mutex<TrafficState>,
}

#[derive(Debug, Default)]
struct TrafficState {
    rules: HashMap<String, RuleTrafficState>,
}

#[derive(Debug, Default)]
struct RuleTrafficState {
    seconds: BTreeMap<i64, TrafficSample>,
    current_minute: Option<MinuteBucket>,
}

#[derive(Debug, Clone, Default)]
struct MinuteBucket {
    time_bucket: i64,
    bytes_in: u64,
    bytes_out: u64,
    connections: u64,
    requests: u64,
    total_duration_ms: u64,
}

impl MinuteBucket {
    fn add(
        &mut self,
        bytes_in: u64,
        bytes_out: u64,
        connections: u64,
        requests: u64,
        duration_ms: u64,
    ) {
        self.bytes_in = self.bytes_in.saturating_add(bytes_in);
        self.bytes_out = self.bytes_out.saturating_add(bytes_out);
        self.connections = self.connections.saturating_add(connections);
        self.requests = self.requests.saturating_add(requests);
        self.total_duration_ms = self.total_duration_ms.saturating_add(duration_ms);
    }

    fn into_persisted(self, rule_id: String) -> PersistedTrafficStat {
        let avg_duration_ms = if self.requests > 0 {
            self.total_duration_ms / self.requests
        } else if self.connections > 0 {
            self.total_duration_ms / self.connections
        } else {
            0
        };
        PersistedTrafficStat {
            rule_id,
            time_bucket: self.time_bucket,
            bytes_in: self.bytes_in,
            bytes_out: self.bytes_out,
            connections: self.connections,
            requests: self.requests,
            total_duration_ms: self.total_duration_ms,
            avg_duration_ms,
            created_at: Utc::now().timestamp_millis(),
        }
    }

    fn to_point(&self, rule_id: &str) -> TrafficStatsPoint {
        let avg_duration_ms = if self.requests > 0 {
            self.total_duration_ms / self.requests
        } else if self.connections > 0 {
            self.total_duration_ms / self.connections
        } else {
            0
        };
        TrafficStatsPoint {
            time_bucket: self.time_bucket,
            rule_id: rule_id.to_owned(),
            bytes_in: self.bytes_in,
            bytes_out: self.bytes_out,
            connections: self.connections,
            requests: self.requests,
            total_duration_ms: self.total_duration_ms,
            avg_duration_ms,
        }
    }
}

impl TrafficTracker {
    pub fn new(sqlite: Option<Arc<SqliteStore>>) -> Self {
        Self {
            sqlite,
            inner: Mutex::new(TrafficState::default()),
        }
    }

    pub fn recorder(
        self: &Arc<Self>,
        rule_id: impl Into<String>,
        logger: Arc<AppLogger>,
    ) -> TrafficRecorder {
        TrafficRecorder::new(rule_id, Arc::clone(self), logger)
    }

    pub fn record(
        &self,
        rule_id: &str,
        at: DateTime<Utc>,
        bytes_in: u64,
        bytes_out: u64,
        connections: u64,
        requests: u64,
        duration_ms: u64,
    ) {
        let second_bucket = at.timestamp();
        let minute_bucket = second_bucket - second_bucket.rem_euclid(60);

        let finalized = {
            let mut inner = self.inner.lock();
            let rule = inner.rules.entry(rule_id.to_owned()).or_default();

            let sample = rule
                .seconds
                .entry(second_bucket)
                .or_insert_with(|| TrafficSample {
                    timestamp: second_bucket,
                    bytes_in: 0,
                    bytes_out: 0,
                    connections: 0,
                    total_duration_ms: 0,
                });
            sample.bytes_in = sample.bytes_in.saturating_add(bytes_in);
            sample.bytes_out = sample.bytes_out.saturating_add(bytes_out);
            sample.connections = sample.connections.saturating_add(connections);
            sample.total_duration_ms = sample.total_duration_ms.saturating_add(duration_ms);

            let min_keep = second_bucket - (MAX_WINDOW_SECONDS - 1);
            rule.seconds.retain(|bucket, _| *bucket >= min_keep);

            let finalized = match rule.current_minute.as_ref() {
                Some(current) if current.time_bucket == minute_bucket => None,
                Some(_) => rule.current_minute.take(),
                None => None,
            };

            let current = rule.current_minute.get_or_insert_with(|| MinuteBucket {
                time_bucket: minute_bucket,
                ..MinuteBucket::default()
            });
            current.add(bytes_in, bytes_out, connections, requests, duration_ms);
            finalized.map(|bucket| bucket.into_persisted(rule_id.to_owned()))
        };

        if let Some(stat) = finalized {
            self.persist_rows(&[stat]);
        }
    }

    pub fn flush_rule(&self, rule_id: &str) {
        let flushed = {
            let mut inner = self.inner.lock();
            inner
                .rules
                .get_mut(rule_id)
                .and_then(|rule| rule.current_minute.take())
                .map(|bucket| bucket.into_persisted(rule_id.to_owned()))
        };
        if let Some(stat) = flushed {
            self.persist_rows(&[stat]);
        }
    }

    pub fn get_window_data(&self, rule_ids: &[String]) -> Vec<TrafficWindowData> {
        let inner = self.inner.lock();
        let selected = if rule_ids.is_empty() {
            inner.rules.keys().cloned().collect::<Vec<_>>()
        } else {
            rule_ids.to_vec()
        };

        let mut items = selected
            .into_iter()
            .filter_map(|rule_id| {
                inner.rules.get(&rule_id).map(|rule| TrafficWindowData {
                    rule_id,
                    samples: rule.seconds.values().cloned().collect(),
                })
            })
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.rule_id.cmp(&b.rule_id));
        items
    }

    pub fn query_stats(&self, req: &QueryTrafficStatsRequest) -> QueryTrafficStatsResult {
        let interval = req.interval.unwrap_or(TrafficStatsInterval::Minute);
        let start_bucket = req.start_time.map(|value| align_bucket(value, interval));
        let end_bucket = req.end_time.map(|value| align_bucket(value, interval));

        let mut stats = self
            .sqlite
            .as_ref()
            .and_then(|sqlite| {
                sqlite
                    .query_traffic_stats(&req.rule_id, start_bucket, end_bucket)
                    .ok()
            })
            .unwrap_or_default();

        let current_point = {
            let inner = self.inner.lock();
            inner
                .rules
                .get(&req.rule_id)
                .and_then(|rule| rule.current_minute.as_ref())
                .map(|bucket| bucket.to_point(&req.rule_id))
        };

        if let Some(point) = current_point {
            let in_range = start_bucket.map_or(true, |start| point.time_bucket >= start)
                && end_bucket.map_or(true, |end| point.time_bucket <= end);
            if in_range {
                stats.retain(|item| item.time_bucket != point.time_bucket);
                stats.push(point);
            }
        }

        stats.sort_by(|a, b| a.time_bucket.cmp(&b.time_bucket));
        let total_bytes_in = stats.iter().map(|item| item.bytes_in).sum();
        let total_bytes_out = stats.iter().map(|item| item.bytes_out).sum();
        let total_connections = stats.iter().map(|item| item.connections).sum();
        QueryTrafficStatsResult {
            stats,
            total_bytes_in,
            total_bytes_out,
            total_connections,
        }
    }

    fn persist_rows(&self, rows: &[PersistedTrafficStat]) {
        if rows.is_empty() {
            return;
        }
        if let Some(sqlite) = &self.sqlite {
            let _ = sqlite.upsert_traffic_stats(rows);
        }
    }
}

fn align_bucket(value: DateTime<Utc>, interval: TrafficStatsInterval) -> i64 {
    let seconds = value.timestamp();
    match interval {
        TrafficStatsInterval::Minute => seconds - seconds.rem_euclid(60),
    }
}
