use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use parking_lot::Mutex;
use wsl_bridge_shared::{ProxyRouteRuntimeItem, ProxyUpstreamRuntimeItem};

#[derive(Debug, Clone)]
pub struct ProxyMetricsRecorder {
    tracker: Arc<ProxyMetricsTracker>,
}

impl ProxyMetricsRecorder {
    pub fn new(tracker: Arc<ProxyMetricsTracker>) -> Self {
        Self { tracker }
    }

    pub fn record_route_match(
        &self,
        route_id: &str,
        listener_id: &str,
        server_name: &str,
        request_path: &str,
    ) {
        self.tracker
            .record_route_match(route_id, listener_id, server_name, request_path);
    }

    pub fn record_route_error(&self, route_id: &str, listener_id: &str, error: &str) {
        self.tracker
            .record_route_error(route_id, listener_id, error);
    }

    pub fn record_upstream_success(
        &self,
        upstream_id: &str,
        route_id: &str,
        target: &str,
        request_path: &str,
    ) {
        self.tracker
            .record_upstream_success(upstream_id, route_id, target, request_path);
    }

    pub fn record_upstream_error(
        &self,
        upstream_id: &str,
        route_id: &str,
        target: &str,
        request_path: &str,
        error: &str,
    ) {
        self.tracker
            .record_upstream_error(upstream_id, route_id, target, request_path, error);
    }
}

#[derive(Debug, Default)]
pub struct ProxyMetricsTracker {
    inner: Mutex<ProxyMetricsState>,
}

#[derive(Debug, Default)]
struct ProxyMetricsState {
    routes: HashMap<String, ProxyRouteRuntimeItem>,
    upstreams: HashMap<String, ProxyUpstreamRuntimeItem>,
}

impl ProxyMetricsTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn recorder(self: &Arc<Self>) -> ProxyMetricsRecorder {
        ProxyMetricsRecorder::new(Arc::clone(self))
    }

    pub fn list_route_runtime(&self, listener_id: &str) -> Vec<ProxyRouteRuntimeItem> {
        let inner = self.inner.lock();
        let mut items = inner
            .routes
            .values()
            .filter(|item| item.listener_id == listener_id)
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.route_id.cmp(&b.route_id));
        items
    }

    pub fn list_upstream_runtime(&self, route_id: &str) -> Vec<ProxyUpstreamRuntimeItem> {
        let inner = self.inner.lock();
        let mut items = inner
            .upstreams
            .values()
            .filter(|item| item.route_id == route_id)
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.upstream_id.cmp(&b.upstream_id));
        items
    }

    fn record_route_match(
        &self,
        route_id: &str,
        listener_id: &str,
        server_name: &str,
        request_path: &str,
    ) {
        let mut inner = self.inner.lock();
        let item =
            inner
                .routes
                .entry(route_id.to_owned())
                .or_insert_with(|| ProxyRouteRuntimeItem {
                    route_id: route_id.to_owned(),
                    listener_id: listener_id.to_owned(),
                    hit_count: 0,
                    error_count: 0,
                    last_match_at: None,
                    last_server_name: None,
                    last_request_path: None,
                    last_error: None,
                });
        item.hit_count = item.hit_count.saturating_add(1);
        item.last_match_at = Some(Utc::now());
        item.last_server_name = Some(server_name.to_owned());
        item.last_request_path = Some(request_path.to_owned());
        item.last_error = None;
    }

    fn record_route_error(&self, route_id: &str, listener_id: &str, error: &str) {
        let mut inner = self.inner.lock();
        let item =
            inner
                .routes
                .entry(route_id.to_owned())
                .or_insert_with(|| ProxyRouteRuntimeItem {
                    route_id: route_id.to_owned(),
                    listener_id: listener_id.to_owned(),
                    hit_count: 0,
                    error_count: 0,
                    last_match_at: None,
                    last_server_name: None,
                    last_request_path: None,
                    last_error: None,
                });
        item.error_count = item.error_count.saturating_add(1);
        item.last_error = Some(error.to_owned());
    }

    fn record_upstream_success(
        &self,
        upstream_id: &str,
        route_id: &str,
        target: &str,
        request_path: &str,
    ) {
        let mut inner = self.inner.lock();
        let item = inner
            .upstreams
            .entry(upstream_id.to_owned())
            .or_insert_with(|| ProxyUpstreamRuntimeItem {
                upstream_id: upstream_id.to_owned(),
                route_id: route_id.to_owned(),
                hit_count: 0,
                error_count: 0,
                last_used_at: None,
                last_target: None,
                last_request_path: None,
                last_error: None,
            });
        item.hit_count = item.hit_count.saturating_add(1);
        item.last_used_at = Some(Utc::now());
        item.last_target = Some(target.to_owned());
        item.last_request_path = Some(request_path.to_owned());
        item.last_error = None;
    }

    fn record_upstream_error(
        &self,
        upstream_id: &str,
        route_id: &str,
        target: &str,
        request_path: &str,
        error: &str,
    ) {
        let mut inner = self.inner.lock();
        let item = inner
            .upstreams
            .entry(upstream_id.to_owned())
            .or_insert_with(|| ProxyUpstreamRuntimeItem {
                upstream_id: upstream_id.to_owned(),
                route_id: route_id.to_owned(),
                hit_count: 0,
                error_count: 0,
                last_used_at: None,
                last_target: None,
                last_request_path: None,
                last_error: None,
            });
        item.error_count = item.error_count.saturating_add(1);
        item.last_used_at = Some(Utc::now());
        item.last_target = Some(target.to_owned());
        item.last_request_path = Some(request_path.to_owned());
        item.last_error = Some(error.to_owned());
    }
}
