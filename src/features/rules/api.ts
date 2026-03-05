import { invokeBridge } from "../../lib/bridge";
import type {
  ApplyRulesResult,
  CreateRuleRequest,
  LogQueryRequest,
  LogQueryResult,
  ProxyRule,
  RuleLogStatsItem,
  RuleLogStatsRequest,
  RulePatch,
  RuntimeStatusItem,
  StopRulesResult,
  TailLogsResult,
  HyperVProbeDebug,
  TopologySnapshot
} from "../../lib/types";

export function listRules() {
  return invokeBridge<ProxyRule[]>("list_rules");
}

export function createRule(req: CreateRuleRequest) {
  return invokeBridge<string>("create_rule", { req });
}

export function updateRule(id: string, patch: RulePatch) {
  return invokeBridge<void>("update_rule", { id, patch });
}

export function deleteRule(id: string) {
  return invokeBridge<void>("delete_rule", { id });
}

export function enableRule(id: string, enabled: boolean) {
  return invokeBridge<void>("enable_rule", { id, enabled });
}

export function applyRules() {
  return invokeBridge<ApplyRulesResult>("apply_rules");
}

export function stopRules() {
  return invokeBridge<StopRulesResult>("stop_rules");
}

export function getRuntimeStatus() {
  return invokeBridge<RuntimeStatusItem[]>("get_runtime_status");
}

export function tailLogs(cursor = 0) {
  return invokeBridge<TailLogsResult>("tail_logs", { cursor });
}

export function queryLogs(req: LogQueryRequest) {
  return invokeBridge<LogQueryResult>("query_logs", { req });
}

export function getRuleLogStats(req: RuleLogStatsRequest) {
  return invokeBridge<RuleLogStatsItem[]>("get_rule_log_stats", { req });
}

export function scanTopology() {
  return invokeBridge<TopologySnapshot>("scan_topology");
}

export function debugHyperVProbe() {
  return invokeBridge<HyperVProbeDebug>("debug_hyperv_probe");
}
