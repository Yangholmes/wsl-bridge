import { invokeBridge } from "../../lib/bridge";
import type {
  CreateProxyCertificateRequest,
  CreateProxyListenerRequest,
  CreateProxyRouteRequest,
  CreateProxyUpstreamRequest,
  ProxyCertificate,
  ProxyListener,
  ProxyRoute,
  ProxyRouteRuntimeItem,
  ProxyRuntimeStatusItem,
  ProxyUpstream,
  ProxyUpstreamRuntimeItem,
  UpdateProxyCertificateRequest,
  UpdateProxyListenerRequest,
  UpdateProxyRouteRequest,
  UpdateProxyUpstreamRequest
} from "../../lib/types";

export function listProxyListeners() {
  return invokeBridge<ProxyListener[]>("list_proxy_listeners");
}

export function listProxyCertificates() {
  return invokeBridge<ProxyCertificate[]>("list_proxy_certificates");
}

export function listProxyRoutes(listenerId: string) {
  return invokeBridge<ProxyRoute[]>("list_proxy_routes", { listenerId });
}

export function listProxyUpstreams(routeId: string) {
  return invokeBridge<ProxyUpstream[]>("list_proxy_upstreams", { routeId });
}

export function getProxyRuntimeStatus() {
  return invokeBridge<ProxyRuntimeStatusItem[]>("get_proxy_runtime_status");
}

export function listProxyRouteRuntime(listenerId: string) {
  return invokeBridge<ProxyRouteRuntimeItem[]>("list_proxy_route_runtime", { listenerId });
}

export function listProxyUpstreamRuntime(routeId: string) {
  return invokeBridge<ProxyUpstreamRuntimeItem[]>("list_proxy_upstream_runtime", { routeId });
}

export function createProxyListener(req: CreateProxyListenerRequest) {
  return invokeBridge<string>("create_proxy_listener", { req });
}

export function createProxyCertificate(req: CreateProxyCertificateRequest) {
  return invokeBridge<string>("create_proxy_certificate", { req });
}

export function updateProxyCertificate(id: string, req: UpdateProxyCertificateRequest) {
  return invokeBridge<void>("update_proxy_certificate", { id, req });
}

export function deleteProxyCertificate(id: string) {
  return invokeBridge<void>("delete_proxy_certificate", { id });
}

export function updateProxyListener(id: string, req: UpdateProxyListenerRequest) {
  return invokeBridge<void>("update_proxy_listener", { id, req });
}

export function deleteProxyListener(id: string) {
  return invokeBridge<void>("delete_proxy_listener", { id });
}

export function createProxyRoute(req: CreateProxyRouteRequest) {
  return invokeBridge<string>("create_proxy_route", { req });
}

export function updateProxyRoute(id: string, req: UpdateProxyRouteRequest) {
  return invokeBridge<void>("update_proxy_route", { id, req });
}

export function deleteProxyRoute(id: string) {
  return invokeBridge<void>("delete_proxy_route", { id });
}

export function createProxyUpstream(req: CreateProxyUpstreamRequest) {
  return invokeBridge<string>("create_proxy_upstream", { req });
}

export function updateProxyUpstream(id: string, req: UpdateProxyUpstreamRequest) {
  return invokeBridge<void>("update_proxy_upstream", { id, req });
}

export function deleteProxyUpstream(id: string) {
  return invokeBridge<void>("delete_proxy_upstream", { id });
}
