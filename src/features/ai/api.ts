import { invokeBridge } from "../../lib/bridge";

export type AgentSkillScope = "project" | "user";

export type AgentSkillTarget = {
  id: string;
  displayName: string;
  scope: AgentSkillScope;
  detected: string;
  mcpDetected: string;
  mcpInstallSupported: boolean;
  mcpGlobalPath?: string;
  globalPath: string;
  supportsNativeSkill: boolean;
  supportsProjectInstall: boolean;
  supportsUserInstall: boolean;
  installType: string;
  fallbackToAgentsDir: boolean;
  dryRunSupported: boolean;
  applySupported: boolean;
};

export type AgentSkillInfo = {
  id: string;
  name: string;
  version: string;
  requiresWslBridgeAiApi: string;
  canonicalPackage: string;
};

export type AgentMcpClientPayload = {
  ok: boolean;
  targetAgent: string;
  detectedState: string;
  path?: string;
  metadataPath?: string;
};

export type AgentSkillTargetsPayload = {
  skill: AgentSkillInfo;
  scope: AgentSkillScope;
  targets: AgentSkillTarget[];
};

export type AgentSkillPreviewWrite = {
  path: string;
  action: string;
  source: string;
};

export type AgentSkillPreviewWarning = {
  severity: "info" | "warning" | "error";
  code: string;
  message: string;
};

export type AgentSkillPreviewPayload = {
  ok: boolean;
  mode: string;
  operation: "install" | "uninstall";
  skill: AgentSkillInfo;
  targetAgent: string;
  scope: AgentSkillScope;
  installType: string;
  detectedState?: string;
  rootPath?: string;
  resolvedPaths: string[];
  writes: AgentSkillPreviewWrite[];
  deletes: AgentSkillPreviewWrite[];
  warnings: AgentSkillPreviewWarning[];
  appliedPaths?: string[];
  deletedPaths?: string[];
};

export type AuditLog = {
  id: number;
  time: string;
  level: string;
  module: string;
  event: string;
  detail: string;
};

export type LogQueryRequest = {
  level?: string;
  module?: string;
  rule_id?: string;
  keyword?: string;
  start_time?: string;
  end_time?: string;
  limit?: number;
  newest_first?: boolean;
};

export type LogQueryResult = {
  total: number;
  events: AuditLog[];
};

export function listAgentTargets(scope?: AgentSkillScope) {
  return invokeBridge<AgentSkillTargetsPayload>("list_agent_targets", { scope });
}

export function installAgentMcpClient(target: string) {
  return invokeBridge<AgentMcpClientPayload>("install_agent_mcp_client", { target });
}

export function uninstallAgentMcpClient(target: string) {
  return invokeBridge<AgentMcpClientPayload>("uninstall_agent_mcp_client", { target });
}

export function installAgentSkillPreview(input: {
  target: string;
  scope?: AgentSkillScope;
  mode?: "dryRun";
  fallbackToAgentsDir?: boolean;
  projectRoot?: string;
}) {
  return invokeBridge<AgentSkillPreviewPayload>("install_agent_skill_preview", {
    target: input.target,
    scope: input.scope,
    mode: input.mode ?? "dryRun",
    fallbackToAgentsDir: input.fallbackToAgentsDir,
    projectRoot: input.projectRoot
  });
}

export function installAgentSkill(input: {
  target: string;
  scope?: AgentSkillScope;
  mode?: "dryRun" | "apply";
  fallbackToAgentsDir?: boolean;
  projectRoot?: string;
}) {
  return invokeBridge<AgentSkillPreviewPayload>("install_agent_skill", {
    target: input.target,
    scope: input.scope,
    mode: input.mode ?? "apply",
    fallbackToAgentsDir: input.fallbackToAgentsDir,
    projectRoot: input.projectRoot
  });
}

export function uninstallAgentSkillPreview(input: {
  target: string;
  scope?: AgentSkillScope;
  mode?: "dryRun";
  fallbackToAgentsDir?: boolean;
  projectRoot?: string;
}) {
  return invokeBridge<AgentSkillPreviewPayload>("uninstall_agent_skill_preview", {
    target: input.target,
    scope: input.scope,
    mode: input.mode ?? "dryRun",
    fallbackToAgentsDir: input.fallbackToAgentsDir,
    projectRoot: input.projectRoot
  });
}

export function uninstallAgentSkill(input: {
  target: string;
  scope?: AgentSkillScope;
  mode?: "dryRun" | "apply";
  fallbackToAgentsDir?: boolean;
  projectRoot?: string;
}) {
  return invokeBridge<AgentSkillPreviewPayload>("uninstall_agent_skill", {
    target: input.target,
    scope: input.scope,
    mode: input.mode ?? "apply",
    fallbackToAgentsDir: input.fallbackToAgentsDir,
    projectRoot: input.projectRoot
  });
}

export function queryAuditLogs(req: LogQueryRequest) {
  return invokeBridge<LogQueryResult>("query_logs", { req });
}
