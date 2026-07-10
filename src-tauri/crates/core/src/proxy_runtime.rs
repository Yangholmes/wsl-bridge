#![allow(dead_code)]

use chrono::{DateTime, Utc};
use wsl_bridge_shared::{ProxyRoute, ProxyUpstream};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServerNameMatchKind {
    Exact,
    Wildcard,
    Default,
}

#[derive(Debug, Clone)]
pub struct MatchedRoute<'a> {
    pub route: &'a ProxyRoute,
    pub server_name: String,
}

pub fn normalize_server_name(value: &str) -> String {
    value
        .trim()
        .trim_end_matches('.')
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub fn select_route<'a>(
    routes: &'a [ProxyRoute],
    host: &str,
    path: &str,
) -> Option<MatchedRoute<'a>> {
    let host = normalize_server_name(host);
    let mut best: Option<(
        ServerNameMatchKind,
        usize,
        DateTime<Utc>,
        &'a ProxyRoute,
        String,
    )> = None;

    for route in routes.iter().filter(|route| route.enabled) {
        if let Some((kind, matched_name)) = classify_route_match(route, &host, path) {
            let candidate = (
                kind,
                route_path_prefix_len(route),
                route.created_at,
                route,
                matched_name,
            );
            if is_better_route_match(best.as_ref(), &candidate) {
                best = Some(candidate);
            }
        }
    }

    best.map(|(_, _, _, route, server_name)| MatchedRoute { route, server_name })
}

pub fn select_upstream(upstreams: &[ProxyUpstream]) -> Option<&ProxyUpstream> {
    upstreams
        .iter()
        .filter(|upstream| upstream.enabled)
        .max_by(|left, right| left.created_at.cmp(&right.created_at))
}

pub fn rewrite_path(path: &str, rewrite_from: Option<&str>, rewrite_to: Option<&str>) -> String {
    let Some(from) = rewrite_from.filter(|value| !value.is_empty()) else {
        return path.to_owned();
    };
    if !path.starts_with(from) {
        return path.to_owned();
    }

    let to = rewrite_to.unwrap_or("/");
    let suffix = &path[from.len()..];
    join_path_prefix(to, suffix)
}

fn classify_route_match(
    route: &ProxyRoute,
    host: &str,
    path: &str,
) -> Option<(ServerNameMatchKind, String)> {
    if !route_path_matches(route, path) {
        return None;
    }
    if route.is_default {
        return Some((ServerNameMatchKind::Default, String::new()));
    }

    let mut best: Option<(ServerNameMatchKind, String)> = None;
    for server_name in &route.server_names {
        if let Some(kind) = classify_server_name_match(server_name, host) {
            let candidate = (kind, server_name.clone());
            if is_better_server_name_match(best.as_ref(), &candidate) {
                best = Some(candidate);
            }
        }
    }
    best
}

fn is_better_route_match(
    current: Option<&(
        ServerNameMatchKind,
        usize,
        DateTime<Utc>,
        &ProxyRoute,
        String,
    )>,
    candidate: &(
        ServerNameMatchKind,
        usize,
        DateTime<Utc>,
        &ProxyRoute,
        String,
    ),
) -> bool {
    let Some(current) = current else {
        return true;
    };
    rank_match_kind(candidate.0) > rank_match_kind(current.0)
        || (rank_match_kind(candidate.0) == rank_match_kind(current.0)
            && (candidate.1 > current.1 || (candidate.1 == current.1 && candidate.2 > current.2)))
}

fn is_better_server_name_match(
    current: Option<&(ServerNameMatchKind, String)>,
    candidate: &(ServerNameMatchKind, String),
) -> bool {
    let Some(current) = current else {
        return true;
    };
    rank_match_kind(candidate.0) > rank_match_kind(current.0)
        || (rank_match_kind(candidate.0) == rank_match_kind(current.0)
            && candidate.1.len() > current.1.len())
}

fn rank_match_kind(kind: ServerNameMatchKind) -> u8 {
    match kind {
        ServerNameMatchKind::Exact => 3,
        ServerNameMatchKind::Wildcard => 2,
        ServerNameMatchKind::Default => 1,
    }
}

fn classify_server_name_match(pattern: &str, host: &str) -> Option<ServerNameMatchKind> {
    let pattern = normalize_server_name(pattern);
    if pattern.is_empty() || host.is_empty() {
        return None;
    }
    if pattern == host {
        return Some(ServerNameMatchKind::Exact);
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return host
            .ends_with(&format!(".{suffix}"))
            .then_some(ServerNameMatchKind::Wildcard);
    }
    if let Some(suffix) = pattern.strip_prefix('.') {
        return (host == suffix || host.ends_with(&format!(".{suffix}")))
            .then_some(ServerNameMatchKind::Wildcard);
    }
    None
}

fn route_path_matches(route: &ProxyRoute, path: &str) -> bool {
    match route.path_prefix.as_deref() {
        None => true,
        Some("/") => true,
        Some(prefix) => path.starts_with(prefix),
    }
}

fn route_path_prefix_len(route: &ProxyRoute) -> usize {
    route.path_prefix.as_deref().unwrap_or("").len()
}

fn join_path_prefix(base: &str, suffix: &str) -> String {
    let base = if base.is_empty() { "/" } else { base };
    let suffix = suffix.trim_start_matches('/');
    if base == "/" {
        if suffix.is_empty() {
            "/".to_owned()
        } else {
            format!("/{suffix}")
        }
    } else if suffix.is_empty() {
        base.to_owned()
    } else {
        format!("{}/{}", base.trim_end_matches('/'), suffix)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use wsl_bridge_shared::{ProxyRoute, ProxyUpstream, TargetKind, UpstreamScheme};

    use super::{normalize_server_name, rewrite_path, select_route, select_upstream};

    fn route(
        id: &str,
        server_names: &[&str],
        path_prefix: Option<&str>,
        is_default: bool,
        enabled: bool,
        seconds_offset: i64,
    ) -> ProxyRoute {
        let created_at = Utc::now() + Duration::seconds(seconds_offset);
        ProxyRoute {
            id: id.to_owned(),
            listener_id: "listener-1".to_owned(),
            server_names: server_names
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            path_prefix: path_prefix.map(str::to_owned),
            is_default,
            enabled,
            created_at,
            updated_at: created_at,
        }
    }

    fn upstream(id: &str, enabled: bool, seconds_offset: i64) -> ProxyUpstream {
        let created_at = Utc::now() + Duration::seconds(seconds_offset);
        ProxyUpstream {
            id: id.to_owned(),
            route_id: "route-1".to_owned(),
            target_kind: TargetKind::Static,
            target_ref: None,
            target_host: Some("127.0.0.1".to_owned()),
            target_port: 3000,
            upstream_scheme: UpstreamScheme::Http,
            path_rewrite_from: None,
            path_rewrite_to: None,
            enabled,
            created_at,
            updated_at: created_at,
        }
    }

    #[test]
    fn normalize_server_name_strips_port_and_trailing_dot() {
        assert_eq!(normalize_server_name("Example.COM:8080."), "example.com");
    }

    #[test]
    fn exact_match_beats_wildcard_and_default() {
        let routes = vec![
            route("wild", &["*.example.com"], None, false, true, 1),
            route("exact", &["a.example.com"], None, false, true, 0),
            route("default", &[], None, true, true, 2),
        ];

        let matched = select_route(&routes, "a.example.com", "/").expect("route");
        assert_eq!(matched.route.id, "exact");
        assert_eq!(matched.server_name, "a.example.com");
    }

    #[test]
    fn dot_prefix_matches_root_and_subdomain() {
        let routes = vec![route("root", &[".example.com"], None, false, true, 0)];
        assert_eq!(
            select_route(&routes, "example.com", "/").map(|match_item| match_item.route.id.clone()),
            Some("root".to_owned())
        );
        assert_eq!(
            select_route(&routes, "api.example.com", "/")
                .map(|match_item| match_item.route.id.clone()),
            Some("root".to_owned())
        );
    }

    #[test]
    fn newer_route_wins_for_same_match_class() {
        let routes = vec![
            route("older", &["a.example.com"], None, false, true, 0),
            route("newer", &["a.example.com"], None, false, true, 10),
        ];

        let matched = select_route(&routes, "a.example.com", "/").expect("route");
        assert_eq!(matched.route.id, "newer");
    }

    #[test]
    fn longer_path_prefix_wins_before_created_time() {
        let routes = vec![
            route("short", &["a.example.com"], Some("/api"), false, true, 10),
            route(
                "long",
                &["a.example.com"],
                Some("/api/admin"),
                false,
                true,
                0,
            ),
        ];

        let matched = select_route(&routes, "a.example.com", "/api/admin/users").expect("route");
        assert_eq!(matched.route.id, "long");
    }

    #[test]
    fn default_route_matches_when_no_server_name_matches() {
        let routes = vec![route("default", &[], None, true, true, 0)];

        let matched = select_route(&routes, "unknown.example.com", "/").expect("route");
        assert_eq!(matched.route.id, "default");
    }

    #[test]
    fn disabled_routes_do_not_participate() {
        let routes = vec![
            route("disabled", &["a.example.com"], None, false, false, 10),
            route("default", &[], None, true, true, 0),
        ];

        let matched = select_route(&routes, "a.example.com", "/").expect("route");
        assert_eq!(matched.route.id, "default");
    }

    #[test]
    fn rewrite_path_rewrites_prefix_only() {
        assert_eq!(
            rewrite_path("/api/users", Some("/api"), Some("/")),
            "/users"
        );
        assert_eq!(
            rewrite_path("/api/users", Some("/api"), Some("/backend")),
            "/backend/users"
        );
        assert_eq!(
            rewrite_path("/users", Some("/api"), Some("/backend")),
            "/users"
        );
    }

    #[test]
    fn newest_enabled_upstream_is_selected() {
        let upstreams = vec![
            upstream("older", true, 0),
            upstream("disabled", false, 20),
            upstream("newer", true, 10),
        ];

        let matched = select_upstream(&upstreams).expect("upstream");
        assert_eq!(matched.id, "newer");
    }
}
