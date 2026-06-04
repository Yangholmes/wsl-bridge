import { invokeBridge } from "../../lib/bridge";

export type AgentSkillScope = "project" | "user";

export type AgentSkillTarget = {
  id: string;
  displayName: string;
  scope: AgentSkillScope;
  detected: string;
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
  skill: AgentSkillInfo;
  targetAgent: string;
  scope: AgentSkillScope;
  installType: string;
  writes: AgentSkillPreviewWrite[];
  warnings: AgentSkillPreviewWarning[];
};

export function listAgentTargets(scope?: AgentSkillScope) {
  return invokeBridge<AgentSkillTargetsPayload>("list_agent_targets", { scope });
}

export function installAgentSkillPreview(input: {
  target: string;
  scope?: AgentSkillScope;
  mode?: "dryRun";
  fallbackToAgentsDir?: boolean;
}) {
  return invokeBridge<AgentSkillPreviewPayload>("install_agent_skill_preview", {
    target: input.target,
    scope: input.scope,
    mode: input.mode ?? "dryRun",
    fallbackToAgentsDir: input.fallbackToAgentsDir
  });
}
