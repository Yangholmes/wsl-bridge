import { invoke } from "@tauri-apps/api/core";

import type {
  ApplyRulesResult,
  AuditLog,
  CreateRuleRequest,
  ProxyRule,
  RulePatch,
  RuntimeStatusItem,
  StopRulesResult,
  TailLogsResult,
  TopologySnapshot
} from "./types";

type MockStore = {
  rules: ProxyRule[];
  runtime: RuntimeStatusItem[];
  logs: AuditLog[];
  seq: number;
};

const mockStore: MockStore = {
  rules: [],
  runtime: [],
  logs: [],
  seq: 0
};

function nowIso() {
  return new Date().toISOString();
}

function appendMockLog(level: string, event: string, detail: string) {
  mockStore.seq += 1;
  mockStore.logs.push({
    id: mockStore.seq,
    time: nowIso(),
    level,
    module: "mock",
    event,
    detail
  });
}

function upsertRuntime(ruleId: string, patch: Partial<RuntimeStatusItem>) {
  const idx = mockStore.runtime.findIndex((v) => v.rule_id === ruleId);
  const base: RuntimeStatusItem =
    idx >= 0
      ? mockStore.runtime[idx]
      : { rule_id: ruleId, state: "stopped", last_error: null, last_apply_at: null };
  const next = { ...base, ...patch };
  if (idx >= 0) mockStore.runtime[idx] = next;
  else mockStore.runtime.push(next);
}

async function invokeMock<T>(command: string, payload: Record<string, unknown>): Promise<T> {
  switch (command) {
    case "scan_topology": {
      const data: TopologySnapshot = {
        adapters: [
          {
            id: "mock-eth0",
            name: "Mock Ethernet",
            ipv4: ["192.168.31.100"],
            ipv6: []
          },
          {
            id: "mock-wlan0",
            name: "Mock Wi-Fi",
            ipv4: ["10.0.0.10"],
            ipv6: []
          }
        ],
        wsl: [],
        hyperv: [],
        timestamp: nowIso()
      };
      return data as T;
    }
    case "list_rules":
      return structuredClone(mockStore.rules) as T;
    case "create_rule": {
      const req = payload.req as CreateRuleRequest;
      const id = `mock-${Math.random().toString(36).slice(2, 10)}`;
      const item: ProxyRule = {
        id,
        name: req.rule.name,
        type: req.rule.type,
        listen_host: req.rule.listen_host,
        listen_port: req.rule.listen_port,
        target_kind: req.rule.target_kind,
        target_ref: req.rule.target_ref,
        target_host: req.rule.target_host,
        target_port: req.rule.target_port,
        bind_mode: req.rule.bind_mode,
        nic_id: req.rule.nic_id,
        enabled: req.rule.enabled,
        created_at: nowIso(),
        updated_at: nowIso()
      };
      mockStore.rules.push(item);
      upsertRuntime(id, { state: "stopped", last_error: null, last_apply_at: null });
      appendMockLog("info", "rule_created", `rule_id=${id}`);
      return id as T;
    }
    case "update_rule": {
      const id = payload.id as string;
      const patch = payload.patch as RulePatch;
      const item = mockStore.rules.find((v) => v.id === id);
      if (!item) throw new Error(`rule not found: ${id}`);
      Object.assign(item, patch, { updated_at: nowIso() });
      appendMockLog("info", "rule_updated", `rule_id=${id}`);
      return undefined as T;
    }
    case "delete_rule": {
      const id = payload.id as string;
      mockStore.rules = mockStore.rules.filter((v) => v.id !== id);
      mockStore.runtime = mockStore.runtime.filter((v) => v.rule_id !== id);
      appendMockLog("info", "rule_deleted", `rule_id=${id}`);
      return undefined as T;
    }
    case "enable_rule": {
      const id = payload.id as string;
      const enabled = Boolean(payload.enabled);
      const item = mockStore.rules.find((v) => v.id === id);
      if (!item) throw new Error(`rule not found: ${id}`);
      item.enabled = enabled;
      item.updated_at = nowIso();
      appendMockLog("info", "rule_toggled", `rule_id=${id},enabled=${enabled}`);
      return undefined as T;
    }
    case "apply_rules": {
      const seen = new Map<string, string>();
      const failed: string[] = [];
      for (const rule of mockStore.rules) {
        if (!rule.enabled) {
          upsertRuntime(rule.id, {
            state: "stopped",
            last_error: null,
            last_apply_at: nowIso()
          });
          continue;
        }
        const key = `${rule.listen_host}:${rule.listen_port}`;
        if (seen.has(key)) {
          failed.push(rule.id);
          upsertRuntime(rule.id, {
            state: "error",
            last_error: `listen conflict with ${seen.get(key)}`,
            last_apply_at: nowIso()
          });
        } else {
          seen.set(key, rule.id);
          upsertRuntime(rule.id, {
            state: "running",
            last_error: null,
            last_apply_at: nowIso()
          });
        }
      }
      appendMockLog("info", "apply_rules", `failed=${failed.length}`);
      const result: ApplyRulesResult = {
        applied: mockStore.runtime.filter((v) => v.state === "running").length,
        failed
      };
      return result as T;
    }
    case "stop_rules": {
      let stopped = 0;
      for (const item of mockStore.runtime) {
        if (item.state !== "stopped") stopped += 1;
        item.state = "stopped";
        item.last_error = null;
        item.last_apply_at = nowIso();
      }
      appendMockLog("info", "stop_rules", `stopped=${stopped}`);
      const result: StopRulesResult = { stopped };
      return result as T;
    }
    case "get_runtime_status":
      return structuredClone(mockStore.runtime) as T;
    case "tail_logs": {
      const cursor = Number(payload.cursor ?? 0);
      const events = mockStore.logs.slice(cursor);
      const result: TailLogsResult = { events, next_cursor: mockStore.logs.length };
      return result as T;
    }
    default:
      throw new Error(`unsupported mock command: ${command}`);
  }
}

export async function invokeBridge<T>(
  command: string,
  payload: Record<string, unknown> = {}
): Promise<T> {
  if ((window as { __TAURI__?: unknown }).__TAURI__) {
    return invoke<T>(command, payload);
  }
  return invokeMock<T>(command, payload);
}

