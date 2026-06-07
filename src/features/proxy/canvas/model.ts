import type {
  ProxyListener,
  ProxyRoute,
  ProxyRouteRuntimeItem,
  ProxyRuntimeStatusItem,
  ProxyUpstream,
  ProxyUpstreamRuntimeItem
} from "../../../lib/types";

export type ProxyCanvasNodeKind = "listener" | "route" | "upstream";

export type SelectedProxyNode = {
  kind: ProxyCanvasNodeKind;
  id: string;
} | null;

export type ProxyCanvasNode = {
  key: string;
  id: string;
  kind: ProxyCanvasNodeKind;
  parentKey: string | null;
  title: string;
  subtitle: string;
  enabled: boolean;
  hasError: boolean;
  runtimeError: string | null;
  source: ProxyListener | ProxyRoute | ProxyUpstream;
};

export type ProxyCanvasEdge = {
  key: string;
  from: string;
  to: string;
  enabled: boolean;
  hasError: boolean;
};

export type ProxyTopologyData = {
  listeners: ProxyListener[];
  routesByListener: Map<string, ProxyRoute[]>;
  upstreamsByRoute: Map<string, ProxyUpstream[]>;
  listenerRuntime: Map<string, ProxyRuntimeStatusItem>;
  routeRuntime: Map<string, ProxyRouteRuntimeItem>;
  upstreamRuntime: Map<string, ProxyUpstreamRuntimeItem>;
};

export function nodeKey(kind: ProxyCanvasNodeKind, id: string) {
  return `${kind}:${id}`;
}

export function parseNodeKey(key: string): SelectedProxyNode {
  const [kind, id] = key.split(":");
  if ((kind === "listener" || kind === "route" || kind === "upstream") && id) {
    return { kind, id };
  }
  return null;
}

export function routeLabel(route: ProxyRoute, defaultRouteLabel: string) {
  if (route.is_default) {
    return defaultRouteLabel;
  }
  const serverName = route.server_names[0]?.trim() ?? "";
  return serverName || defaultRouteLabel;
}

export function upstreamTargetLabel(upstream: ProxyUpstream) {
  const target = upstream.target_host ?? upstream.target_ref ?? "-";
  return `${target}:${upstream.target_port}`;
}

export function sortListeners(listeners: ProxyListener[]) {
  return [...listeners].sort((left, right) => {
    const byCreated = left.created_at.localeCompare(right.created_at);
    return byCreated === 0 ? left.id.localeCompare(right.id) : byCreated;
  });
}

export function sortRoutes(routes: ProxyRoute[]) {
  return [...routes].sort((left, right) => {
    if (left.is_default !== right.is_default) {
      return left.is_default ? 1 : -1;
    }
    const leftPathLength = left.path_prefix?.length ?? 0;
    const rightPathLength = right.path_prefix?.length ?? 0;
    if (leftPathLength !== rightPathLength) {
      return rightPathLength - leftPathLength;
    }
    const leftWildcard = left.server_names.some(isWildcardServerName);
    const rightWildcard = right.server_names.some(isWildcardServerName);
    if (leftWildcard !== rightWildcard) {
      return leftWildcard ? 1 : -1;
    }
    const byCreated = right.created_at.localeCompare(left.created_at);
    return byCreated === 0 ? left.id.localeCompare(right.id) : byCreated;
  });
}

export function sortUpstreams(upstreams: ProxyUpstream[]) {
  return [...upstreams].sort((left, right) => {
    if (left.enabled !== right.enabled) return left.enabled ? -1 : 1;
    const byCreated = right.created_at.localeCompare(left.created_at);
    return byCreated === 0 ? left.id.localeCompare(right.id) : byCreated;
  });
}

export function buildProxyCanvasGraph(
  data: ProxyTopologyData,
  labels: {
    defaultRoute: string;
  }
) {
  const nodes: ProxyCanvasNode[] = [];
  const edges: ProxyCanvasEdge[] = [];

  for (const listener of sortListeners(data.listeners)) {
    const listenerRuntime = data.listenerRuntime.get(listener.id);
    const listenerKey = nodeKey("listener", listener.id);
    nodes.push({
      key: listenerKey,
      id: listener.id,
      kind: "listener",
      parentKey: null,
      title: listener.name,
      subtitle: `${listener.protocol}://${listener.listen_host}:${listener.listen_port}`,
      enabled: listener.enabled,
      hasError: listenerRuntime?.state === "error",
      runtimeError: listenerRuntime?.last_error ?? null,
      source: listener
    });

    const routes = sortRoutes(data.routesByListener.get(listener.id) ?? []);
    for (const route of routes) {
      const routeRuntime = data.routeRuntime.get(route.id);
      const routeKey = nodeKey("route", route.id);
      const routeHasError = Boolean(routeRuntime?.last_error) || (routeRuntime?.error_count ?? 0) > 0;
      nodes.push({
        key: routeKey,
        id: route.id,
        kind: "route",
        parentKey: listenerKey,
        title: routeLabel(route, labels.defaultRoute),
        subtitle: route.path_prefix ?? "/",
        enabled: listener.enabled && route.enabled,
        hasError: routeHasError,
        runtimeError: routeRuntime?.last_error ?? null,
        source: route
      });
      edges.push({
        key: `${listenerKey}->${routeKey}`,
        from: listenerKey,
        to: routeKey,
        enabled: listener.enabled && route.enabled,
        hasError: routeHasError
      });

      const upstreams = sortUpstreams(data.upstreamsByRoute.get(route.id) ?? []);
      for (const upstream of upstreams) {
        const upstreamRuntime = data.upstreamRuntime.get(upstream.id);
        const upstreamKey = nodeKey("upstream", upstream.id);
        const upstreamHasError =
          Boolean(upstreamRuntime?.last_error) || (upstreamRuntime?.error_count ?? 0) > 0;
        nodes.push({
          key: upstreamKey,
          id: upstream.id,
          kind: "upstream",
          parentKey: routeKey,
          title: upstream.target_kind,
          subtitle: `${upstream.upstream_scheme} ${upstreamTargetLabel(upstream)}`,
          enabled: listener.enabled && route.enabled && upstream.enabled,
          hasError: upstreamHasError,
          runtimeError: upstreamRuntime?.last_error ?? null,
          source: upstream
        });
        edges.push({
          key: `${routeKey}->${upstreamKey}`,
          from: routeKey,
          to: upstreamKey,
          enabled: listener.enabled && route.enabled && upstream.enabled,
          hasError: upstreamHasError
        });
      }
    }
  }

  return { nodes, edges };
}

function isWildcardServerName(value: string) {
  return value.startsWith("*.") || value.startsWith(".");
}
