export type RuleType = "tcp_fwd" | "udp_fwd" | "http_proxy" | "socks5_proxy";
export type TargetKind = "wsl" | "hyperv" | "static";
export type BindMode = "single_nic" | "all_nics";
export type RuntimeState = "running" | "stopped" | "error";

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

export type TailLogsResult = {
  events: AuditLog[];
  next_cursor: number;
};

export type LogQueryRequest = {
  level?: string | null;
  module?: string | null;
  rule_id?: string | null;
  keyword?: string | null;
  start_time?: string | null;
  end_time?: string | null;
  limit?: number | null;
  newest_first?: boolean | null;
};

export type LogQueryResult = {
  total: number;
  events: AuditLog[];
};

export type RuleLogStatsRequest = {
  rule_ids?: string[] | null;
  since_minutes?: number | null;
};

export type RuleLogStatsItem = {
  rule_id: string;
  total: number;
  errors: number;
  last_time: string | null;
  last_error: string | null;
};

export type AuditLog = {
  id: number;
  time: string;
  level: string;
  module: string;
  event: string;
  detail: string;
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

export type McpServerConfig = {
  enabled: boolean;
  server_name: string;
  listen_port: number;
  api_token: string;
  expose_topology_read: boolean;
  expose_rule_config: boolean;
};

export type McpToolDescriptor = {
  name: string;
  description: string;
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
