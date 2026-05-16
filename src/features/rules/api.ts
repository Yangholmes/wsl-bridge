import { invokeBridge } from "../../lib/bridge";
import type {
  ApplyRulesResult,
  CreateRuleRequest,
  ProxyRule,
  RuleMigrationRecord,
  RulePatch,
  RuntimeStatusItem,
  StopRulesResult,
  HyperVProbeDebug,
  TopologySnapshot,
  QueryTrafficStatsRequest,
  QueryTrafficStatsResult,
  TrafficWindowData
} from "../../lib/types";

export function listRules() {
  return invokeBridge<ProxyRule[]>("list_rules");
}

export function listRuleMigrations() {
  return invokeBridge<RuleMigrationRecord[]>("list_rule_migrations");
}

export function createRule(req: CreateRuleRequest) {
  return invokeBridge<string>("create_rule", { req });
}

export function migrateRuleToProxy(ruleId: string) {
  return invokeBridge<RuleMigrationRecord>("migrate_rule_to_proxy", { ruleId });
}

export function rollbackRuleMigration(ruleId: string) {
  return invokeBridge<RuleMigrationRecord>("rollback_rule_migration", { ruleId });
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

export function getTrafficWindowData(ruleIds: string[]) {
  return invokeBridge<TrafficWindowData[]>("get_traffic_window_data", { ruleIds });
}

export function queryTrafficStats(req: QueryTrafficStatsRequest) {
  return invokeBridge<QueryTrafficStatsResult>("query_traffic_stats", { req });
}

export function scanTopology() {
  return invokeBridge<TopologySnapshot>("scan_topology");
}

export function debugHyperVProbe() {
  return invokeBridge<HyperVProbeDebug>("debug_hyperv_probe");
}
