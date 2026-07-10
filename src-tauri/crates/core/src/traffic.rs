use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use wsl_bridge_shared::{
    QueryTrafficStatsRequest, QueryTrafficStatsResult, TrafficEntityType, TrafficSample,
    TrafficStatsInterval, TrafficStatsPoint, TrafficWindowData, TrafficWindowQueryEntity,
};

use crate::app_logs::{AccessLogEntry, AppLogger, ErrorLogEntry};
use crate::sqlite_store::SqliteStore;

const MAX_WINDOW_SECONDS: i64 = 120;

#[derive(Debug, Clone)]
pub struct TrafficRecorder {
    entity_type: TrafficEntityType,
    entity_id: String,
    tracker: Arc<TrafficTracker>,
    logger: Arc<AppLogger>,
}

impl TrafficRecorder {
    pub fn new(
        entity_type: TrafficEntityType,
        entity_id: impl Into<String>,
        tracker: Arc<TrafficTracker>,
        logger: Arc<AppLogger>,
    ) -> Self {
        Self {
            entity_type,
            entity_id: entity_id.into(),
            tracker,
            logger,
        }
    }

    pub fn rule_id(&self) -> &str {
        &self.entity_id
    }

    pub fn scoped(
        &self,
        entity_type: TrafficEntityType,
        entity_id: impl Into<String>,
    ) -> TrafficRecorder {
        TrafficRecorder::new(
            entity_type,
            entity_id,
            Arc::clone(&self.tracker),
            Arc::clone(&self.logger),
        )
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
            self.entity_type,
            &self.entity_id,
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
            entry.with_rule_id(self.entity_id.clone())
        };
        self.logger.log_error(entry);
    }
}

#[derive(Debug, Clone)]
pub struct PersistedTrafficStat {
    pub entity_type: TrafficEntityType,
    pub entity_id: String,
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
    entities: HashMap<TrafficEntityKey, EntityTrafficState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TrafficEntityKey {
    entity_type: TrafficEntityType,
    entity_id: String,
}

#[derive(Debug, Default)]
struct EntityTrafficState {
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

    fn into_persisted(self, entity_key: &TrafficEntityKey) -> PersistedTrafficStat {
        let avg_duration_ms = if self.requests > 0 {
            self.total_duration_ms / self.requests
        } else if self.connections > 0 {
            self.total_duration_ms / self.connections
        } else {
            0
        };
        PersistedTrafficStat {
            entity_type: entity_key.entity_type,
            entity_id: entity_key.entity_id.clone(),
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

    fn to_point(&self, entity_key: &TrafficEntityKey) -> TrafficStatsPoint {
        let avg_duration_ms = if self.requests > 0 {
            self.total_duration_ms / self.requests
        } else if self.connections > 0 {
            self.total_duration_ms / self.connections
        } else {
            0
        };
        TrafficStatsPoint {
            time_bucket: self.time_bucket,
            entity_type: entity_key.entity_type,
            entity_id: entity_key.entity_id.clone(),
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
        entity_type: TrafficEntityType,
        entity_id: impl Into<String>,
        logger: Arc<AppLogger>,
    ) -> TrafficRecorder {
        TrafficRecorder::new(entity_type, entity_id, Arc::clone(self), logger)
    }

    pub fn record(
        &self,
        entity_type: TrafficEntityType,
        entity_id: &str,
        at: DateTime<Utc>,
        bytes_in: u64,
        bytes_out: u64,
        connections: u64,
        requests: u64,
        duration_ms: u64,
    ) {
        let second_bucket = at.timestamp();
        let minute_bucket = second_bucket - second_bucket.rem_euclid(60);
        let entity_key = TrafficEntityKey {
            entity_type,
            entity_id: entity_id.to_owned(),
        };

        let finalized = {
            let mut inner = self.inner.lock();
            let entity = inner.entities.entry(entity_key.clone()).or_default();

            let sample = entity
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
            entity.seconds.retain(|bucket, _| *bucket >= min_keep);

            let finalized = match entity.current_minute.as_ref() {
                Some(current) if current.time_bucket == minute_bucket => None,
                Some(_) => entity.current_minute.take(),
                None => None,
            };

            let current = entity.current_minute.get_or_insert_with(|| MinuteBucket {
                time_bucket: minute_bucket,
                ..MinuteBucket::default()
            });
            current.add(bytes_in, bytes_out, connections, requests, duration_ms);
            finalized.map(|bucket| bucket.into_persisted(&entity_key))
        };

        if let Some(stat) = finalized {
            self.persist_rows(&[stat]);
        }
    }

    pub fn flush_entity(&self, entity_type: TrafficEntityType, entity_id: &str) {
        let entity_key = TrafficEntityKey {
            entity_type,
            entity_id: entity_id.to_owned(),
        };
        let flushed = {
            let mut inner = self.inner.lock();
            inner
                .entities
                .get_mut(&entity_key)
                .and_then(|rule| rule.current_minute.take())
                .map(|bucket| bucket.into_persisted(&entity_key))
        };
        if let Some(stat) = flushed {
            self.persist_rows(&[stat]);
        }
    }

    pub fn flush_entities_of_type(&self, entity_type: TrafficEntityType) {
        let flushed = {
            let mut inner = self.inner.lock();
            let keys = inner
                .entities
                .keys()
                .filter(|key| key.entity_type == entity_type)
                .cloned()
                .collect::<Vec<_>>();
            let mut rows = Vec::new();
            for key in keys {
                if let Some(entity) = inner.entities.get_mut(&key) {
                    if let Some(bucket) = entity.current_minute.take() {
                        rows.push(bucket.into_persisted(&key));
                    }
                }
            }
            rows
        };
        self.persist_rows(&flushed);
    }

    pub fn get_window_data(&self, entities: &[TrafficWindowQueryEntity]) -> Vec<TrafficWindowData> {
        let inner = self.inner.lock();
        let selected = if entities.is_empty() {
            inner.entities.keys().cloned().collect::<Vec<_>>()
        } else {
            entities
                .iter()
                .map(|entity| TrafficEntityKey {
                    entity_type: entity.entity_type,
                    entity_id: entity.entity_id.clone(),
                })
                .collect::<Vec<_>>()
        };

        let mut items = selected
            .into_iter()
            .filter_map(|entity_key| {
                inner
                    .entities
                    .get(&entity_key)
                    .map(|rule| TrafficWindowData {
                        entity_type: entity_key.entity_type,
                        entity_id: entity_key.entity_id,
                        samples: rule.seconds.values().cloned().collect(),
                    })
            })
            .collect::<Vec<_>>();
        items.sort_by(|a, b| {
            (traffic_entity_type_rank(a.entity_type), &a.entity_id)
                .cmp(&(traffic_entity_type_rank(b.entity_type), &b.entity_id))
        });
        items
    }

    pub fn query_stats(&self, req: &QueryTrafficStatsRequest) -> QueryTrafficStatsResult {
        let interval = req.interval.unwrap_or(TrafficStatsInterval::Minute);
        let start_bucket = req.start_time.map(|value| align_bucket(value, interval));
        let end_bucket = req.end_time.map(|value| align_bucket(value, interval));
        let entity_key = TrafficEntityKey {
            entity_type: req.entity_type,
            entity_id: req.entity_id.clone(),
        };

        let mut stats = self
            .sqlite
            .as_ref()
            .and_then(|sqlite| {
                sqlite
                    .query_traffic_stats(req.entity_type, &req.entity_id, start_bucket, end_bucket)
                    .ok()
            })
            .unwrap_or_default();

        let current_point = {
            let inner = self.inner.lock();
            inner
                .entities
                .get(&entity_key)
                .and_then(|rule| rule.current_minute.as_ref())
                .map(|bucket| bucket.to_point(&entity_key))
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

fn traffic_entity_type_rank(value: TrafficEntityType) -> u8 {
    match value {
        TrafficEntityType::LegacyRule => 0,
        TrafficEntityType::ProxyUpstream => 1,
    }
}
