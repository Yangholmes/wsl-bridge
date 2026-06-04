export type RuleType = "tcp_fwd" | "udp_fwd" | "http_proxy" | "socks5_proxy";
export type TargetKind = "wsl" | "hyperv" | "static";
export type BindMode = "single_nic" | "all_nics";
export type RuntimeState = "running" | "stopped" | "error";
export type ProxyProtocol = "http" | "https";
export type ProxyTlsMode = "disabled" | "manual_cert" | "local_ca";
export type UpstreamScheme = "http" | "https" | "ws" | "wss" | "grpc" | "grpcs";
export type ProxyCertificateSourceType = "manual_upload" | "local_ca";
export type TrafficEntityType = "legacy_rule" | "proxy_upstream";

export type ProxyRule = {
  id: string;
  name: string;
  type: RuleType;
  listen_host: string;
  listen_port: number;
  target_kind: TargetKind;
  target_ref: string | null;
  target_host: string | null;
  target_port: number | null;
  bind_mode: BindMode;
  nic_id: string | null;
  enabled: boolean;
  created_at: string;
  updated_at: string;
};

export type RuntimeStatusItem = {
  rule_id: string;
  state: RuntimeState;
  last_error: string | null;
  last_apply_at: string | null;
};

export type ProxyRuntimeStatusItem = {
  listener_id: string;
  state: RuntimeState;
  last_error: string | null;
  last_apply_at: string | null;
};

export type ProxyRouteRuntimeItem = {
  route_id: string;
  listener_id: string;
  hit_count: number;
  error_count: number;
  last_match_at: string | null;
  last_server_name: string | null;
  last_request_path: string | null;
  last_error: string | null;
};

export type ProxyUpstreamRuntimeItem = {
  upstream_id: string;
  route_id: string;
  hit_count: number;
  error_count: number;
  last_used_at: string | null;
  last_target: string | null;
  last_request_path: string | null;
  last_error: string | null;
};

export type RuleMigrationStatus = "pending" | "migrated" | "rollbacked";

export type RuleMigrationRecord = {
  rule_id: string;
  status: RuleMigrationStatus;
  original_rule_enabled: boolean;
  proxy_listener_id: string;
  proxy_route_id: string;
  proxy_upstream_id: string | null;
  detail: string | null;
  migrated_at: string;
  rollbacked_at: string | null;
};

export type FirewallPolicyInput = {
  allow_domain: boolean;
  allow_private: boolean;
  allow_public: boolean;
  direction?: "inbound" | "outbound" | "in" | "out";
  action?: "allow" | "block" | "bypass";
};

export type CreateRuleRequest = {
  rule: {
    name: string;
    type: RuleType;
    listen_host: string;
    listen_port: number;
    target_kind: TargetKind;
    target_ref: string | null;
    target_host: string | null;
    target_port: number | null;
    bind_mode: BindMode;
    nic_id: string | null;
    enabled: boolean;
  };
  firewall: FirewallPolicyInput | null;
};

export type RulePatch = {
  name?: string;
  listen_host?: string;
  listen_port?: number;
  target_ref?: string | null;
  target_host?: string | null;
  target_port?: number | null;
  bind_mode?: BindMode;
  nic_id?: string | null;
  enabled?: boolean;
};

export type ApplyRulesResult = {
  applied: number;
  failed: string[];
};

export type StopRulesResult = {
  stopped: number;
};

export type AdapterInfo = {
  id: string;
  name: string;
  ipv4: string[];
  ipv6: string[];
};

export type WslInfo = {
  distro: string;
  networking_mode: string;
  ip: string | null;
};

export type HyperVVmInfo = {
  vm_name: string;
  v_switch: string | null;
  ip: string | null;
};

export type TopologySnapshot = {
  adapters: AdapterInfo[];
  wsl: WslInfo[];
  hyperv: HyperVVmInfo[];
  hyperv_error: string | null;
  timestamp: string;
};

export type HyperVProbeStep = {
  source: string;
  executable: string;
  ok: boolean;
  status_code: number;
  parsed_vm_names: string[];
  raw_stdout: string;
  raw_stderr: string;
};

export type HyperVProbeDebug = {
  timestamp: string;
  selected_vm_names: string[];
  steps: HyperVProbeStep[];
};

export type BuildFlavor = "standard" | "su";

export type AppRuntimeStatus = {
  build_flavor: BuildFlavor;
  is_admin: boolean;
  admin_features_available: boolean;
};

export type CloseBehavior = "ask" | "minimize" | "exit";
export type HostsGroupSourceType = "system_imported" | "copied" | "manual" | "file_imported";

export type AppSettings = {
  close_behavior: CloseBehavior;
  show_tray_on_start: boolean;
};

export type HostsGroup = {
  id: string;
  name: string;
  description: string | null;
  source_type: HostsGroupSourceType;
  is_active: boolean;
  created_at: string;
  updated_at: string;
};

export type HostsEntry = {
  id: string;
  group_id: string;
  ip: string;
  domain: string;
  comment: string | null;
  enabled: boolean;
  order_index: number;
  created_at: string;
  updated_at: string;
};

export type ProxyListener = {
  id: string;
  name: string;
  listen_host: string;
  listen_port: number;
  protocol: ProxyProtocol;
  tls_mode: ProxyTlsMode;
  cert_id: string | null;
  bind_mode: BindMode;
  nic_id: string | null;
  enabled: boolean;
  created_at: string;
  updated_at: string;
};

export type ProxyRoute = {
  id: string;
  listener_id: string;
  server_names: string[];
  path_prefix: string | null;
  is_default: boolean;
  enabled: boolean;
  created_at: string;
  updated_at: string;
};

export type ProxyUpstream = {
  id: string;
  route_id: string;
  target_kind: TargetKind;
  target_ref: string | null;
  target_host: string | null;
  target_port: number;
  upstream_scheme: UpstreamScheme;
  path_rewrite_from: string | null;
  path_rewrite_to: string | null;
  enabled: boolean;
  created_at: string;
  updated_at: string;
};

export type ProxyCertificate = {
  id: string;
  name: string;
  source_type: ProxyCertificateSourceType;
  cert_path: string;
  key_path: string;
  domains: string[];
  created_at: string;
  updated_at: string;
};

export type CreateProxyListenerRequest = {
  name: string;
  listen_host: string;
  listen_port: number;
  protocol: ProxyProtocol;
  tls_mode: ProxyTlsMode;
  cert_id?: string | null;
  bind_mode: BindMode;
  nic_id?: string | null;
  enabled: boolean;
};

export type UpdateProxyListenerRequest = CreateProxyListenerRequest;

export type CreateProxyRouteRequest = {
  listener_id: string;
  server_names: string[];
  path_prefix?: string | null;
  is_default: boolean;
  enabled: boolean;
};

export type UpdateProxyRouteRequest = {
  server_names: string[];
  path_prefix?: string | null;
  is_default: boolean;
  enabled: boolean;
};

export type CreateProxyUpstreamRequest = {
  route_id: string;
  target_kind: TargetKind;
  target_ref?: string | null;
  target_host?: string | null;
  target_port: number;
  upstream_scheme: UpstreamScheme;
  path_rewrite_from?: string | null;
  path_rewrite_to?: string | null;
  enabled: boolean;
};

export type UpdateProxyUpstreamRequest = CreateProxyUpstreamRequest;

export type CreateProxyCertificateRequest = {
  name: string;
  source_type: ProxyCertificateSourceType;
  cert_path: string;
  key_path: string;
  domains: string[];
};

export type UpdateProxyCertificateRequest = CreateProxyCertificateRequest;

export type CreateHostsGroupRequest = {
  name: string;
  description?: string | null;
};

export type UpdateHostsGroupRequest = {
  name: string;
  description?: string | null;
};

export type CopyHostsGroupRequest = {
  source_group_id: string;
  name: string;
  description?: string | null;
};

export type HostsEntryInput = {
  id?: string | null;
  ip: string;
  domain: string;
  comment?: string | null;
  enabled: boolean;
  order_index: number;
};

export type SaveHostsEntriesRequest = {
  group_id: string;
  entries: HostsEntryInput[];
};

export type ImportHostsGroupRequest = {
  path: string;
  name?: string | null;
  description?: string | null;
};

export type ExportHostsGroupRequest = {
  group_id: string;
  path: string;
};

export type McpServerConfig = {
  enabled: boolean;
  server_name: string;
  listen_port: number;
  expose_topology_read: boolean;
  expose_rule_config: boolean;
  expose_traffic_stats: boolean;
};

export type McpToolDescriptor = {
  name: string;
  description_key: string;
  enabled: boolean;
};

export type McpServerStatus = {
  config: McpServerConfig;
  base_url: string;
  running: boolean;
  last_error: string | null;
  tools: McpToolDescriptor[];
  client_presets: McpClientPreset[];
};

export type McpClientPreset = {
  id: string;
  label: string;
  format: string;
  content: string;
};

export type TrafficSample = {
  timestamp: number;
  bytes_in: number;
  bytes_out: number;
  connections: number;
  total_duration_ms: number;
};

export type TrafficMonitorEntity = {
  entity_type: TrafficEntityType;
  entity_id: string;
  label: string;
  enabled: boolean;
};

export type TrafficWindowQueryEntity = {
  entity_type: TrafficEntityType;
  entity_id: string;
};

export type TrafficWindowData = {
  entity_type: TrafficEntityType;
  entity_id: string;
  samples: TrafficSample[];
};

export type TrafficStatsInterval = "minute";

export type QueryTrafficStatsRequest = {
  entity_type: TrafficEntityType;
  entity_id: string;
  start_time?: string | null;
  end_time?: string | null;
  interval?: TrafficStatsInterval | null;
};

export type TrafficStatsPoint = {
  time_bucket: number;
  entity_type: TrafficEntityType;
  entity_id: string;
  bytes_in: number;
  bytes_out: number;
  connections: number;
  requests: number;
  total_duration_ms: number;
  avg_duration_ms: number;
};

export type QueryTrafficStatsResult = {
  stats: TrafficStatsPoint[];
  total_bytes_in: number;
  total_bytes_out: number;
  total_connections: number;
};
