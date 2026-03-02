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

export type TopologySnapshot = {
  adapters: AdapterInfo[];
  wsl: Array<Record<string, unknown>>;
  hyperv: Array<Record<string, unknown>>;
  timestamp: string;
};

