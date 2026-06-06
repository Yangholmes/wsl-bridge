import * as KButton from "@kobalte/core/button";
import * as KCheckbox from "@kobalte/core/checkbox";
import * as KTextField from "@kobalte/core/text-field";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { queryOptions, useQuery } from "@tanstack/solid-query";
import { createEffect, createMemo, createSignal, For, Show } from "solid-js";

import "./AiIntegrationPage.css";

import { useI18n } from "../../i18n/context";
import { Modal } from "../../lib/Modal";
import type { McpClientPreset, McpServerConfig, McpServerStatus } from "../../lib/types";
import { CopyIcon, MetricCard, PageHeader, SectionCard, StatusBadge } from "../../lib/ui";
import { Hint } from "../../lib/Hint";
import { useToast } from "../../lib/Toast";
import { getMcpServerStatus, updateMcpServerConfig } from "../settings/api";
import {
  installAgentMcpClient,
  installAgentSkill,
  installAgentSkillPreview,
  listAgentTargets,
  queryAuditLogs,
  uninstallAgentMcpClient,
  uninstallAgentSkill,
  uninstallAgentSkillPreview,
  type AuditLog,
  type AgentSkillScope,
  type LogQueryResult,
  type AgentSkillTargetsPayload,
  type AgentSkillPreviewPayload,
  type AgentSkillTarget
} from "./api";

const EMPTY_MCP_CONFIG: McpServerConfig = {
  enabled: false,
  server_name: "wsl-bridge",
  listen_port: 13746,
  expose_topology_read: true,
  expose_rule_config: true,
  expose_traffic_stats: true
};

function presetOptionLabel(preset: McpClientPreset) {
  return `${preset.label} (${preset.format})`;
}

function targetIdFromPresetId(presetId: string) {
  if (presetId === "claude-code") return "claude-code";
  if (presetId === "codex") return "codex";
  if (presetId === "cursor") return "cursor";
  if (presetId === "opencode") return "opencode";
  return "";
}

function targetDetectedTone(detected: string) {
  if (detected === "installed") return "ready";
  if (detected === "conflict") return "error";
  return "unknown";
}

function formatAuditTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(undefined, {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit"
  }).format(date);
}

type SkillTreeNode = {
  children: Map<string, SkillTreeNode>;
};

function normalizeWindowsPath(path?: string) {
  return path ? path.replaceAll("/", "\\").replace(/\\+$/, "") : "";
}

function parentWindowsPath(path?: string) {
  const normalized = normalizeWindowsPath(path);
  const index = normalized.lastIndexOf("\\");
  if (index <= 0) return "";
  return normalized.slice(0, index);
}

function displayTreeBasePath(
  rootPath: string | undefined,
  scope: AgentSkillScope | undefined,
  targetAgent: string | undefined
) {
  if (!rootPath) return "";
  if (scope === "user" && targetAgent === "opencode") {
    return parentWindowsPath(parentWindowsPath(rootPath));
  }
  return parentWindowsPath(rootPath);
}

function createSkillTree(
  paths: string[],
  rootPath?: string,
  scope?: AgentSkillScope,
  targetAgent?: string
) {
  const root: SkillTreeNode = { children: new Map() };
  const normalizedRoot = displayTreeBasePath(rootPath, scope, targetAgent);
  const normalizedPaths = paths
    .map((item) => item.replaceAll("/", "\\"))
    .filter((item) => item.length > 0);

  for (const path of normalizedPaths) {
    const relative =
      normalizedRoot && path.toLowerCase().startsWith(normalizedRoot.toLowerCase())
        ? path.slice(normalizedRoot.length).replace(/^\\+/, "")
        : path;
    const segments = relative.split("\\").filter(Boolean);
    let cursor = root;
    for (const segment of segments) {
      if (!cursor.children.has(segment)) {
        cursor.children.set(segment, { children: new Map() });
      }
      cursor = cursor.children.get(segment)!;
    }
  }

  return root;
}

function buildSkillTreeText(
  node: SkillTreeNode,
  rootLabel: string,
  comments: Record<string, string>
) {
  const lines = [`${rootLabel}/`];

  const walk = (current: SkillTreeNode, prefix: string) => {
    const entries = Array.from(current.children.entries()).sort(([leftName, leftNode], [rightName, rightNode]) => {
      if (leftNode.children.size === 0 && rightNode.children.size > 0) return 1;
      if (leftNode.children.size > 0 && rightNode.children.size === 0) return -1;
      return leftName.localeCompare(rightName);
    });
    entries.forEach(([label, child], index) => {
      const isLast = index === entries.length - 1;
      const connector = isLast ? "└── " : "├── ";
      const nextPrefix = `${prefix}${isLast ? "    " : "│   "}`;
      const isDirectory = child.children.size > 0;
      const displayLabel = isDirectory ? `${label}/` : label;
      const comment = comments[label];
      lines.push(`${prefix}${connector}${displayLabel}${comment ? `    # ${comment}` : ""}`);
      if (isDirectory) {
        walk(child, nextPrefix);
      }
    });
  };

  walk(node, "");
  return lines.join("\n");
}

export function AiIntegrationPage() {
  const { t } = useI18n();
  const toast = useToast();

  const [mcpDraft, setMcpDraft] = createSignal<McpServerConfig>(EMPTY_MCP_CONFIG);
  const [mcpDirty, setMcpDirty] = createSignal(false);
  const [mcpSaving, setMcpSaving] = createSignal(false);
  const [selectedPresetId, setSelectedPresetId] = createSignal("claude-code");
  const [selectedAgentTargetId, setSelectedAgentTargetId] = createSignal("generic");
  const [skillPreview, setSkillPreview] = createSignal<AgentSkillPreviewPayload | null>(null);
  const [skillPreviewLoading, setSkillPreviewLoading] = createSignal(false);
  const [skillPreviewDialogOpen, setSkillPreviewDialogOpen] = createSignal(false);
  const [skillInstallDialogOpen, setSkillInstallDialogOpen] = createSignal(false);
  const [skillInstalling, setSkillInstalling] = createSignal(false);
  const [skillAction, setSkillAction] = createSignal<"install" | "uninstall">("install");
  const [showApiDetails, setShowApiDetails] = createSignal(false);
  const [diagnosticsRan, setDiagnosticsRan] = createSignal(false);

  const mcpStatusQuery = useQuery(() =>
    queryOptions<McpServerStatus>({
      queryKey: ["ai", "mcp-status"],
      queryFn: getMcpServerStatus,
      staleTime: 0
    })
  );

  const agentTargetsQuery = useQuery(() =>
    queryOptions<AgentSkillTargetsPayload>({
      queryKey: ["ai", "agent-targets", "user"],
      queryFn: () => listAgentTargets("user"),
      staleTime: 30_000
    })
  );

  const auditLogsQuery = useQuery(() =>
    queryOptions<LogQueryResult>({
      queryKey: ["ai", "audit-logs"],
      queryFn: () =>
        queryAuditLogs({
          limit: 12,
          newest_first: true
        }),
      staleTime: 15_000
    })
  );

  createEffect(() => {
    const remote = mcpStatusQuery.data?.config;
    if (!remote || mcpDirty()) return;
    setMcpDraft(remote);
  });

  createEffect(() => {
    const presets = mcpStatusQuery.data?.client_presets ?? [];
    if (presets.length === 0) return;
    const selected = presets.some((item) => item.id === selectedPresetId());
    if (!selected) {
      setSelectedPresetId(presets[0].id);
    }
  });

  createEffect(() => {
    const targets = agentTargetsQuery.data?.targets ?? [];
    if (targets.length === 0) return;
    const selected = targets.some((item) => item.id === selectedAgentTargetId());
    if (!selected) {
      setSelectedAgentTargetId(targets[0].id);
    }
  });

  const enabledToolCount = createMemo(
    () => mcpStatusQuery.data?.tools.filter((tool) => tool.enabled).length ?? 0
  );

  const enabledResourceCount = createMemo(() => {
    const config = mcpDraft();
    return [
      config.expose_topology_read,
      config.expose_rule_config,
      config.expose_traffic_stats
    ].filter(Boolean).length + 3;
  });

  const selectedPreset = createMemo(
    () => mcpStatusQuery.data?.client_presets.find((preset) => preset.id === selectedPresetId()) ?? null
  );
  const selectedPresetTarget = createMemo(() => {
    const targetId = targetIdFromPresetId(selectedPresetId());
    if (!targetId) return null;
    return agentTargetsQuery.data?.targets.find((target) => target.id === targetId) ?? null;
  });

  const selectedAgentTarget = createMemo(
    () => agentTargetsQuery.data?.targets.find((target) => target.id === selectedAgentTargetId()) ?? null
  );

  const aiApiVersionLabel = createMemo(() => t("ai.apiVersionV1"));
  const recentErrorCount = createMemo(
    () =>
      auditLogsQuery.data?.events.filter((event) => {
        const level = event.level.toLowerCase();
        return level === "error" || level === "warn" || level === "warning";
      }).length ?? 0
  );
  const diagnosticsItems = createMemo(() => {
    const items = [
      {
        label: t("ai.diagnosticMcp"),
        state:
          mcpStatusQuery.data?.running && !mcpStatusQuery.data?.last_error ? "ready" : "stopped",
        detail: mcpStatusQuery.data?.running ? t("common.running") : t("common.stopped")
      },
      {
        label: t("ai.diagnosticTools"),
        state: enabledToolCount() > 0 ? "ready" : "unknown",
        detail: `${enabledToolCount()}`
      },
      {
        label: t("ai.diagnosticSkill"),
        state: (agentTargetsQuery.data?.targets.length ?? 0) > 0 ? "ready" : "unknown",
        detail:
          (agentTargetsQuery.data?.targets.length ?? 0) > 0
            ? t("ai.genericFallbackReady")
            : t("ai.unavailable")
      },
      {
        label: t("ai.diagnosticPreset"),
        state: (mcpStatusQuery.data?.client_presets.length ?? 0) > 0 ? "ready" : "unknown",
        detail: `${mcpStatusQuery.data?.client_presets.length ?? 0}`
      },
      {
        label: t("ai.diagnosticPreview"),
        state: skillPreview()?.ok ? "ready" : "unknown",
        detail: skillPreview()?.mode ? previewModeLabel(skillPreview()!.mode) : t("ai.previewPending")
      }
    ] as const;
    return items;
  });
  const diagnosticsReport = createMemo(() =>
    diagnosticsItems()
      .map((item) => `${item.label}: ${item.detail}`)
      .join("\n")
  );
  const canInstallPreview = createMemo(() => {
    const preview = skillPreview();
    const target = selectedAgentTarget();
    if (!preview || !target) return false;
    return (
      preview.targetAgent === target.id &&
      preview.operation === "install" &&
      target.applySupported &&
      preview.detectedState !== "conflict"
    );
  });
  const canUninstallPreview = createMemo(() => {
    const preview = skillPreview();
    const target = selectedAgentTarget();
    if (!preview || !target) return false;
    return (
      preview.targetAgent === target.id &&
      preview.operation === "uninstall" &&
      target.applySupported &&
      preview.detectedState === "installed"
    );
  });
  const canUninstallGlobalSkill = createMemo(() => {
    const target = selectedAgentTarget();
    if (!target) return false;
    return target.applySupported && target.detected === "installed";
  });
  const previewPaths = createMemo(() => skillPreview()?.resolvedPaths ?? []);
  const previewTree = createMemo(() =>
    createSkillTree(
      previewPaths(),
      skillPreview()?.rootPath,
      skillPreview()?.scope,
      skillPreview()?.targetAgent
    )
  );
  const previewTreeText = createMemo(() =>
    buildSkillTreeText(
      previewTree(),
      skillPreview()?.scope === "user" ? "~" : t("ai.projectRootVirtual"),
      {
        ".agents": t("ai.skillTreeCommentGenericRoot"),
        ".claude": t("ai.skillTreeCommentClaudeRoot"),
        ".codex": t("ai.skillTreeCommentCodexRoot"),
        ".cursor": t("ai.skillTreeCommentCursorRoot"),
        ".copilot": t("ai.skillTreeCommentCopilotRoot"),
        ".opencode": t("ai.skillTreeCommentOpenCodeProjectRoot"),
        ".config": t("ai.skillTreeCommentConfigRoot"),
        "opencode": t("ai.skillTreeCommentOpenCodeRoot"),
        "opencode.json": t("ai.skillTreeCommentOpenCodeClientConfig"),
        "skills": t("ai.skillTreeCommentSkillsDir"),
        "wsl-bridge-operator": t("ai.skillTreeCommentSkillPackage"),
        "SKILL.md": t("ai.skillTreeCommentSkillEntry"),
        "manifest.json": t("ai.skillTreeCommentManifest"),
        "references": t("ai.skillTreeCommentReferencesDir"),
        "concepts.md": t("ai.skillTreeCommentConcepts"),
        "proxy-recipes.md": t("ai.skillTreeCommentProxyRecipes"),
        "hosts-recipes.md": t("ai.skillTreeCommentHostsRecipes"),
        "rules-legacy.md": t("ai.skillTreeCommentRulesLegacy"),
        "troubleshooting.md": t("ai.skillTreeCommentTroubleshooting"),
        "patch-schema.md": t("ai.skillTreeCommentPatchSchema"),
        "safety.md": t("ai.skillTreeCommentSafety")
      }
    )
  );
  const isPreviewEmpty = createMemo(() => previewPaths().length === 0);
  const skillDialogTitle = createMemo(() =>
    skillAction() === "install" ? t("ai.skillInstallConfirmTitle") : t("ai.skillUninstallConfirmTitle")
  );
  const skillDialogDescription = createMemo(() =>
    skillAction() === "install"
      ? t("ai.skillInstallConfirmDescription")
      : t("ai.skillUninstallConfirmDescription")
  );
  const skillDialogHint = createMemo(() =>
    skillAction() === "install" ? t("ai.skillInstallConfirmHint") : t("ai.skillUninstallConfirmHint")
  );
  const skillDialogSubmitLabel = createMemo(() =>
    skillInstalling()
      ? t("common.loading")
      : skillAction() === "install"
        ? t("ai.installSkill")
        : t("ai.uninstallSkill")
  );
  const skillPreviewTitle = createMemo(
    () => `${selectedAgentTarget()?.displayName ?? t("ai.agentSkillTitle")} · ${t("ai.previewResult")}`
  );
  const skillPreviewDescription = createMemo(() =>
    t("ai.skillPreviewInstallDescription")
  );

  function agentInstallTypeLabel(target: AgentSkillTarget) {
    if (target.fallbackToAgentsDir) return t("ai.installTypeAgentsFallback");
    if (target.supportsNativeSkill) return t("ai.nativeSkill");
    if (target.installType === "skill-directory") return t("ai.installTypeSkillDirectory");
    return target.installType;
  }

  function previewModeLabel(mode: string) {
    if (mode === "dryRun") return t("ai.previewModeDryRun");
    if (mode === "apply") return t("ai.previewModeApply");
    return mode;
  }

  function previewInstallTypeLabel(preview: AgentSkillPreviewPayload) {
    if (preview.targetAgent === "generic") return t("ai.installTypeAgentsFallback");
    if (preview.targetAgent === "claude-code") return t("ai.nativeSkill");
    if (preview.installType === "skill-directory") return t("ai.installTypeSkillDirectory");
    return preview.installType;
  }

  function skillWarningMessage(
    warning: AgentSkillPreviewPayload["warnings"][number],
    operation: AgentSkillPreviewPayload["operation"]
  ) {
    switch (warning.code) {
      case "SENSITIVE_INSTALL":
        return t("ai.skillWarningSensitiveInstall");
      case "SENSITIVE_UNINSTALL":
        return t("ai.skillWarningSensitiveUninstall");
      case "USER_SCOPE_AFFECTS_ALL_PROJECTS":
        return operation === "install"
          ? t("ai.skillWarningUserScopeInstall")
          : t("ai.skillWarningUserScopeUninstall");
      case "GENERIC_SKILL_FALLBACK":
        return t("ai.skillWarningGenericFallback");
      case "NOT_INSTALLED":
        return t("ai.skillWarningNotInstalled");
      case "UNMANAGED_FILES_SKIPPED": {
        const detail = warning.message.split(":").slice(1).join(":").trim();
        return detail
          ? t("ai.skillWarningUnmanagedFilesSkipped", { paths: detail } as never)
          : t("ai.skillWarningUnmanagedFilesSkippedFallback");
      }
      default:
        return warning.message;
    }
  }

  function detectedLabel(state: string) {
    if (state === "installed") return t("ai.installed");
    if (state === "conflict") return t("ai.skillConflict");
    if (state === "not_installed") return t("ai.notInstalled");
    if (state === "unsupported") return t("ai.mcpUnsupported");
    return t("ai.unavailable");
  }

  async function refreshMcpStatus() {
    await mcpStatusQuery.refetch();
    toast.info(t("settings.mcpReloaded"));
  }

  async function saveMcpConfig() {
    const draft = mcpDraft();
    if (!draft.server_name.trim()) {
      toast.error(t("settings.mcpValidationServerName"));
      return;
    }

    try {
      setMcpSaving(true);
      await updateMcpServerConfig({
        ...draft,
        server_name: draft.server_name.trim()
      });
      setMcpDirty(false);
      await mcpStatusQuery.refetch();
      toast.info(t("settings.mcpSaved"));
    } catch (err) {
      toast.error(String(err));
    } finally {
      setMcpSaving(false);
    }
  }

  async function copyText(text: string, successKey: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast.info(t(successKey as never));
    } catch (err) {
      toast.error(String(err));
    }
  }
  function runDiagnostics() {
    setDiagnosticsRan(true);
    void auditLogsQuery.refetch();
    toast.info(t("ai.diagnosticsCompleted"));
  }

  async function copyDiagnosticsReport() {
    await copyText(diagnosticsReport(), "ai.diagnosticsReportCopied");
  }

  async function refreshAuditLogs() {
    await auditLogsQuery.refetch();
  }

  async function refreshAgentTargets() {
    await agentTargetsQuery.refetch();
    const target = selectedAgentTarget();
    if (skillPreviewDialogOpen() && target) {
      await previewAgentSkillInstall(target);
    }
  }

  async function installGlobalMcpClient(target: AgentSkillTarget) {
    try {
      const result = await installAgentMcpClient(target.id);
      await refreshAgentTargets();
      toast.info(t("ai.mcpInstallApplied"));
      return result;
    } catch (err) {
      toast.error(String(err));
      return null;
    }
  }

  async function uninstallGlobalMcpClient(target: AgentSkillTarget) {
    try {
      const result = await uninstallAgentMcpClient(target.id);
      await refreshAgentTargets();
      toast.info(t("ai.mcpUninstallApplied"));
      return result;
    } catch (err) {
      toast.error(String(err));
      return null;
    }
  }

  async function ensureGlobalMcpClientForSkill(target: AgentSkillTarget) {
    if (!target.mcpInstallSupported) return true;
    if (target.mcpDetected === "installed") return true;
    if (target.mcpDetected === "conflict") {
      toast.error(t("ai.mcpConflictBeforeSkill"));
      return false;
    }
    const result = await installGlobalMcpClient(target);
    if (!result?.ok) return false;
    toast.info(t("ai.mcpAutoInstalledBeforeSkill"));
    return true;
  }

  async function previewAgentSkillInstall(target: AgentSkillTarget) {
    try {
      setSelectedAgentTargetId(target.id);
      setSkillPreviewLoading(true);
      setSkillAction("install");
      const preview = await installAgentSkillPreview({
        target: target.id,
        scope: "user",
        mode: "dryRun",
        fallbackToAgentsDir: target.fallbackToAgentsDir
      });
      setSkillPreview(preview);
      toast.info(t("ai.skillPreviewReady"));
    } catch (err) {
      toast.error(String(err));
    } finally {
      setSkillPreviewLoading(false);
    }
  }

  async function previewAgentSkillUninstall(target: AgentSkillTarget) {
    try {
      setSelectedAgentTargetId(target.id);
      setSkillPreviewLoading(true);
      setSkillAction("uninstall");
      const preview = await uninstallAgentSkillPreview({
        target: target.id,
        scope: "user",
        mode: "dryRun",
        fallbackToAgentsDir: target.fallbackToAgentsDir
      });
      setSkillPreview(preview);
      toast.info(t("ai.skillUninstallPreviewReady"));
    } catch (err) {
      toast.error(String(err));
    } finally {
      setSkillPreviewLoading(false);
    }
  }

  async function openSkillPreviewDialog(target: AgentSkillTarget) {
    setSelectedAgentTargetId(target.id);
    setSkillPreviewDialogOpen(true);
    await previewAgentSkillInstall(target);
  }

  function openSkillInstallDialog() {
    if (!canInstallPreview()) {
      toast.error(t("ai.skillInstallPreviewRequired"));
      return;
    }
    setSkillAction("install");
    setSkillPreviewDialogOpen(false);
    setSkillInstallDialogOpen(true);
  }

  async function openSkillUninstallDialog() {
    const target = selectedAgentTarget();
    if (!target) return;
    try {
      await previewAgentSkillUninstall(target);
      if (!canUninstallPreview()) {
        toast.error(t("ai.skillUninstallPreviewRequired"));
        return;
      }
      setSkillAction("uninstall");
      setSkillPreviewDialogOpen(false);
      setSkillInstallDialogOpen(true);
    } catch (err) {
      toast.error(String(err));
    }
  }

  async function installSkillToProject() {
    const target = selectedAgentTarget();
    if (!target) return;

    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: t("ai.selectProjectRoot")
    });
    if (!selected || Array.isArray(selected)) return;

    try {
      setSkillInstalling(true);
      if (!(await ensureGlobalMcpClientForSkill(target))) {
        return;
      }
      const preview = await installAgentSkillPreview({
        target: target.id,
        scope: "project",
        mode: "dryRun",
        fallbackToAgentsDir: target.fallbackToAgentsDir,
        projectRoot: selected
      });
      if (preview.detectedState === "conflict") {
        setSkillPreview(preview);
        toast.error(t("ai.skillConflict"));
        return;
      }
      const result = await installAgentSkill({
        target: target.id,
        scope: "project",
        mode: "apply",
        fallbackToAgentsDir: target.fallbackToAgentsDir,
        projectRoot: selected
      });
      setSkillPreview(result);
      await agentTargetsQuery.refetch();
      await auditLogsQuery.refetch();
      toast.info(t("ai.projectSkillInstallApplied"));
    } catch (err) {
      toast.error(String(err));
    } finally {
      setSkillInstalling(false);
    }
  }

  async function applyAgentSkillAction() {
    const target = selectedAgentTarget();
    if (!target) return;

    try {
      setSkillInstalling(true);
      if (skillAction() === "install" && !(await ensureGlobalMcpClientForSkill(target))) {
        return;
      }
      const result =
        skillAction() === "install"
          ? await installAgentSkill({
              target: target.id,
              scope: "user",
              mode: "apply",
              fallbackToAgentsDir: target.fallbackToAgentsDir
            })
          : await uninstallAgentSkill({
              target: target.id,
              scope: "user",
              mode: "apply",
              fallbackToAgentsDir: target.fallbackToAgentsDir
            });
      setSkillPreview(result);
      setSkillInstallDialogOpen(false);
      await agentTargetsQuery.refetch();
      await auditLogsQuery.refetch();
      toast.info(
        skillAction() === "install" ? t("ai.skillInstallApplied") : t("ai.skillUninstallApplied")
      );
    } catch (err) {
      toast.error(String(err));
    } finally {
      setSkillInstalling(false);
    }
  }

  function updateDraft<K extends keyof McpServerConfig>(key: K, value: McpServerConfig[K]) {
    setMcpDraft((prev) => ({ ...prev, [key]: value }));
    setMcpDirty(true);
  }

  return (
    <div class="page ai-page">
      <PageHeader
        title={t("ai.title")}
        eyebrow={t("ai.eyebrow")}
      />

      <div class="metric-grid">
        <MetricCard
          label={t("ai.mcpStatus")}
          value={
            <StatusBadge
              state={mcpStatusQuery.data?.running ? "running" : "stopped"}
              label={mcpStatusQuery.data?.running ? t("common.running") : t("common.stopped")}
            />
          }
          detail={mcpStatusQuery.data?.base_url ?? "127.0.0.1"}
        />
        <MetricCard
          label={t("ai.resourcesCountLabel")}
          value={String(enabledResourceCount())}
          detail={t("ai.exposedCapabilities")}
        />
        <MetricCard
          label={t("ai.toolsCountLabel")}
          value={String(enabledToolCount())}
          detail={t("ai.exposedCapabilities")}
        />
        <MetricCard
          label={t("ai.recentIssues")}
          value={`${recentErrorCount()}`}
          detail={t("ai.recentIssuesDetail")}
        />
      </div>

      <Show when={mcpStatusQuery.data?.last_error}>
        {(err) => <Hint variant="error">{err()}</Hint>}
      </Show>

      <div class="ai-grid">
        <SectionCard
          title={t("ai.agentSkillTitle")}
          subtitle={t("ai.agentSkillSubtitle")}
          actions={
            <KButton.Root
              class="kb-btn ghost"
              onClick={() => void refreshAgentTargets()}
              disabled={agentTargetsQuery.isFetching}
            >
              {t("common.refresh")}
            </KButton.Root>
          }
        >
          <div class="ai-agent-list">
            <For each={agentTargetsQuery.data?.targets ?? []}>
              {(target) => (
                <div
                  class="ai-agent-item"
                  data-active={selectedAgentTargetId() === target.id ? "true" : undefined}
                  onClick={() => setSelectedAgentTargetId(target.id)}
                >
                    <div>
                      <div class="ai-agent-name">{target.displayName || t(`ai.agent.${target.id}` as never)}</div>
                      <div class="muted">{agentInstallTypeLabel(target)}</div>
                    </div>
                  <div class="runtime-tools">
                    <StatusBadge
                      state={targetDetectedTone(target.detected)}
                      label={detectedLabel(target.detected)}
                    />
                    <KButton.Root
                      class="kb-btn ghost"
                      onClick={() => void openSkillPreviewDialog(target)}
                      disabled={!target.dryRunSupported || skillPreviewLoading()}
                    >
                      {skillPreviewLoading() && selectedAgentTargetId() === target.id
                        ? t("common.loading")
                        : t("ai.openSkillPreview")}
                    </KButton.Root>
                  </div>
                </div>
              )}
            </For>
          </div>
          <Show when={!agentTargetsQuery.isLoading && (agentTargetsQuery.data?.targets.length ?? 0) === 0}>
            <div class="ai-empty-state">{t("ai.agentTargetsEmpty")}</div>
          </Show>
          <Hint>{t("ai.agentSkillHint")}</Hint>
        </SectionCard>

        <SectionCard
          title={t("ai.mcpServiceTitle")}
          subtitle={t("ai.mcpServiceSubtitle")}
          actions={
            <div class="runtime-tools">
              <KButton.Root
                class="kb-btn ghost"
                onClick={() => void copyText(mcpStatusQuery.data?.base_url ?? "", "settings.mcpBaseUrlCopied")}
                disabled={!mcpStatusQuery.data?.base_url}
              >
                <CopyIcon size={14} />
                {t("ai.copyMcpConfig")}
              </KButton.Root>
              <KButton.Root class="kb-btn ghost" onClick={refreshMcpStatus} disabled={mcpStatusQuery.isFetching}>
                {t("common.refresh")}
              </KButton.Root>
            </div>
          }
        >
          <div class="ai-form-grid">
            <KTextField.Root
              class="kb-field"
              value={mcpDraft().server_name}
              onChange={(value) => updateDraft("server_name", value)}
            >
              <KTextField.Label>{t("settings.mcpServerName")}</KTextField.Label>
              <KTextField.Input class="kb-input" />
            </KTextField.Root>

            <div class="kb-field">
              <label class="kb-label">{t("settings.mcpPort")}</label>
              <input class="kb-input" readonly value={String(mcpStatusQuery.data?.config.listen_port ?? mcpDraft().listen_port)} />
            </div>
          </div>

          <div class="ai-switch-row">
            <KCheckbox.Root
              checked={mcpDraft().enabled}
              onChange={(checked) => updateDraft("enabled", checked)}
              class="kb-checkbox"
            >
              <KCheckbox.Input />
              <KCheckbox.Control class="kb-checkbox-control">
                <KCheckbox.Indicator class="kb-checkbox-indicator" />
              </KCheckbox.Control>
              <KCheckbox.Label class="kb-checkbox-label">{t("settings.mcpEnabledHint")}</KCheckbox.Label>
            </KCheckbox.Root>
          </div>

          <div class="ai-baseurl-row">
            <input class="kb-input" readonly value={mcpStatusQuery.data?.base_url ?? ""} />
          </div>

          <div class="ai-api-summary">
            <div class="ai-api-summary-main">
              <div>
                <span class="muted">{t("ai.apiVersion")}</span>
                <strong>{aiApiVersionLabel()}</strong>
              </div>
              <div>
                <span class="muted">{t("ai.exposedCapabilities")}</span>
                <strong>{t("ai.resourcesToolsSummary", { resources: enabledResourceCount(), tools: enabledToolCount() } as never)}</strong>
              </div>
            </div>
            <KButton.Root
              class="kb-btn ghost ai-inline-toggle"
              onClick={() => setShowApiDetails((current) => !current)}
            >
              {showApiDetails() ? t("ai.hideAdvanced") : t("ai.showAdvanced")}
            </KButton.Root>
          </div>

          <Show when={showApiDetails()}>
            <div class="ai-advanced-panel">
              <div class="ai-capability-list">
                <KCheckbox.Root
                  checked={mcpDraft().expose_topology_read}
                  onChange={(checked) => updateDraft("expose_topology_read", checked)}
                  class="kb-checkbox"
                >
                  <KCheckbox.Input />
                  <KCheckbox.Control class="kb-checkbox-control">
                    <KCheckbox.Indicator class="kb-checkbox-indicator" />
                  </KCheckbox.Control>
                  <KCheckbox.Label class="kb-checkbox-label">{t("settings.mcpCapabilityTopology")}</KCheckbox.Label>
                </KCheckbox.Root>

                <KCheckbox.Root
                  checked={mcpDraft().expose_rule_config}
                  onChange={(checked) => updateDraft("expose_rule_config", checked)}
                  class="kb-checkbox"
                >
                  <KCheckbox.Input />
                  <KCheckbox.Control class="kb-checkbox-control">
                    <KCheckbox.Indicator class="kb-checkbox-indicator" />
                  </KCheckbox.Control>
                  <KCheckbox.Label class="kb-checkbox-label">{t("settings.mcpCapabilityRules")}</KCheckbox.Label>
                </KCheckbox.Root>

                <KCheckbox.Root
                  checked={mcpDraft().expose_traffic_stats}
                  onChange={(checked) => updateDraft("expose_traffic_stats", checked)}
                  class="kb-checkbox"
                >
                  <KCheckbox.Input />
                  <KCheckbox.Control class="kb-checkbox-control">
                    <KCheckbox.Indicator class="kb-checkbox-indicator" />
                  </KCheckbox.Control>
                  <KCheckbox.Label class="kb-checkbox-label">{t("settings.mcpCapabilityTraffic")}</KCheckbox.Label>
                </KCheckbox.Root>
              </div>
            </div>
          </Show>

          <div class="ai-save-row">
            <KButton.Root class="kb-btn accent" onClick={saveMcpConfig} disabled={mcpSaving() || !mcpDirty()}>
              {t("settings.mcpSave")}
            </KButton.Root>
          </div>
        </SectionCard>

        <SectionCard title={t("ai.clientReferenceTitle")} subtitle={t("ai.clientReferenceSubtitle")}>
          <div class="ai-preset-list">
            <For each={mcpStatusQuery.data?.client_presets ?? []}>
              {(preset) => (
                <KButton.Root
                  class="kb-btn ghost ai-preset-button"
                  data-active={selectedPresetId() === preset.id ? "true" : undefined}
                  onClick={() => setSelectedPresetId(preset.id)}
                >
                  {presetOptionLabel(preset)}
                </KButton.Root>
              )}
            </For>
          </div>
          <div class="ai-client-install-row">
            <div class="ai-client-install-status">
              <span class="muted">{t("ai.mcpInstallStatus")}</span>
              <StatusBadge
                state={targetDetectedTone(selectedPresetTarget()?.mcpDetected ?? "unknown")}
                label={detectedLabel(selectedPresetTarget()?.mcpDetected ?? "unknown")}
              />
            </div>
            <div class="runtime-tools">
              <KButton.Root
                class="kb-btn ghost"
                onClick={() => selectedPresetTarget() && void installGlobalMcpClient(selectedPresetTarget()!)}
                disabled={
                  !selectedPresetTarget()?.mcpInstallSupported ||
                  selectedPresetTarget()?.mcpDetected === "installed" ||
                  selectedPresetTarget()?.mcpDetected === "conflict"
                }
              >
                {t("ai.installMcpToGlobal")}
              </KButton.Root>
              <KButton.Root
                class="kb-btn ghost danger"
                onClick={() => selectedPresetTarget() && void uninstallGlobalMcpClient(selectedPresetTarget()!)}
                disabled={
                  !selectedPresetTarget()?.mcpInstallSupported ||
                  selectedPresetTarget()?.mcpDetected !== "installed"
                }
              >
                {t("ai.uninstallGlobalMcp")}
              </KButton.Root>
            </div>
          </div>
          <div class="ai-code-header">
            <span class="kb-label">{t("settings.mcpClientConfig")}</span>
            <KButton.Root
              class="kb-btn ghost"
              onClick={() => void copyText(selectedPreset()?.content ?? "", "settings.mcpPresetCopied")}
              disabled={!selectedPreset()?.content}
            >
              <CopyIcon size={14} />
              {t("settings.mcpCopyConfig")}
            </KButton.Root>
          </div>
          <code class="settings-mcp-config-code">{selectedPreset()?.content ?? ""}</code>
        </SectionCard>

        <SectionCard title={t("ai.diagnosticsTitle")} subtitle={t("ai.diagnosticsSubtitle")}>
          <div class="ai-diagnostics-actions">
            <KButton.Root class="kb-btn ghost" onClick={runDiagnostics}>
              {t("ai.runDiagnostics")}
            </KButton.Root>
            <KButton.Root class="kb-btn ghost" onClick={copyDiagnosticsReport}>
              <CopyIcon size={14} />
              {t("ai.copyDiagnosticsReport")}
            </KButton.Root>
          </div>
          <div class="ai-diagnostics-list">
            <For each={diagnosticsItems()}>
              {(item) => (
                <div class="ai-diagnostic-item">
                  <span>{item.label}</span>
                  <StatusBadge state={item.state} label={item.detail} />
                </div>
              )}
            </For>
            <Show when={diagnosticsRan()}>
              <div class="ai-diagnostic-result">{t("ai.diagnosticsCompleted")}</div>
            </Show>
          </div>
        </SectionCard>

        <div class="ai-audit-panel">
          <SectionCard
            title={t("ai.auditTitle")}
            subtitle={t("ai.auditSubtitle")}
            actions={
              <div class="runtime-tools">
                <span class="muted ai-audit-count">{t("ai.auditRecentWindow")}</span>
                <KButton.Root
                  class="kb-btn ghost"
                  onClick={() => void refreshAuditLogs()}
                  disabled={auditLogsQuery.isFetching}
                >
                  {t("common.refresh")}
                </KButton.Root>
              </div>
            }
          >
          <Show when={(auditLogsQuery.data?.events.length ?? 0) > 0} fallback={<div class="ai-empty-state">{t("ai.auditEmpty")}</div>}>
            <div class="ai-audit-list">
              <For each={auditLogsQuery.data?.events ?? ([] as AuditLog[])}>
                {(event) => (
                  <div class="ai-audit-item">
                    <div class="ai-audit-main">
                      <div class="ai-audit-title-group">
                        <strong title={event.event}>{event.event}</strong>
                        <span class="muted">{event.module}</span>
                      </div>
                      <StatusBadge
                        state={
                          event.level.toLowerCase() === "error"
                            ? "error"
                            : event.level.toLowerCase() === "warn" || event.level.toLowerCase() === "warning"
                              ? "stopped"
                              : "ready"
                        }
                        label={event.level}
                      />
                    </div>
                    <div class="ai-audit-meta">
                      <span>{formatAuditTime(event.time)}</span>
                      <span class="muted ai-audit-detail" title={event.detail}>{event.detail}</span>
                    </div>
                  </div>
                )}
              </For>
            </div>
          </Show>
          </SectionCard>
        </div>
      </div>
      <Modal
        open={skillPreviewDialogOpen()}
        onOpenChange={setSkillPreviewDialogOpen}
        title={skillPreviewTitle()}
        description={skillPreviewDescription()}
        footer={
          <div class="runtime-tools ai-skill-preview-footer">
            <div class="runtime-tools">
              <KButton.Root class="kb-btn ghost" onClick={() => setSkillPreviewDialogOpen(false)}>
                {t("common.close")}
              </KButton.Root>
              <KButton.Root
                class="kb-btn accent"
                onClick={openSkillInstallDialog}
                disabled={!canInstallPreview() || skillInstalling() || skillPreview()?.detectedState === "conflict"}
              >
                {t("ai.installSkillToGlobal")}
              </KButton.Root>
              <KButton.Root
                class="kb-btn ghost danger"
                onClick={() => void openSkillUninstallDialog()}
                disabled={!canUninstallGlobalSkill() || skillInstalling()}
              >
                {t("ai.uninstallGlobalSkill")}
              </KButton.Root>
              <KButton.Root
                class="kb-btn ghost"
                onClick={() => void installSkillToProject()}
                disabled={!selectedAgentTarget() || skillInstalling()}
              >
                {t("ai.installSkillToProject")}
              </KButton.Root>
            </div>
          </div>
        }
      >
        <Show when={skillPreview()}>
          {(preview) => (
            <div class="ai-skill-preview">
              <div class="ai-skill-preview-header">
                <div>
                  <span class="kb-label">{t("ai.previewResult")}</span>
                  <strong>{selectedAgentTarget()?.displayName ?? preview().targetAgent}</strong>
                </div>
                <StatusBadge state={preview().ok ? "ready" : "error"} label={previewModeLabel(preview().mode)} />
              </div>
              <div class="ai-preview-meta">
                <span>{detectedLabel(preview().detectedState ?? "unknown")}</span>
                <span>{previewInstallTypeLabel(preview())}</span>
                <span>{preview().skill.canonicalPackage}</span>
              </div>
              <div class="ai-preview-location">
                <span class="kb-label">{t("ai.skillPreviewLocation")}</span>
                <code>{preview().rootPath || selectedAgentTarget()?.globalPath || "-"}</code>
              </div>
              <div class="ai-preview-tree">
                <span class="kb-label">{t("ai.skillPreviewTree")}</span>
                <Show when={!isPreviewEmpty()} fallback={<div class="ai-empty-state">{t("ai.skillPreviewEmpty")}</div>}>
                  <code class="settings-mcp-config-code ai-tree-code">{previewTreeText()}</code>
                </Show>
              </div>
              <Show when={preview().detectedState === "conflict"}>
                <Hint variant="error">{t("ai.skillConflictHint")}</Hint>
              </Show>
              <Show when={preview().warnings.length > 0}>
                <div class="ai-preview-warnings">
                  <For each={preview().warnings}>
                    {(warning) => (
                      <Hint variant={warning.severity === "error" ? "error" : "info"}>
                        {skillWarningMessage(warning, preview().operation)}
                      </Hint>
                    )}
                  </For>
                </div>
              </Show>
            </div>
          )}
        </Show>
      </Modal>
      <Modal
        open={skillInstallDialogOpen()}
        onOpenChange={setSkillInstallDialogOpen}
        title={skillDialogTitle()}
        description={skillDialogDescription()}
        busy={skillInstalling()}
        footer={
          <div class="runtime-tools">
            <KButton.Root
              class="kb-btn ghost"
              onClick={() => setSkillInstallDialogOpen(false)}
              disabled={skillInstalling()}
            >
              {t("ai.skillDialogCancel")}
            </KButton.Root>
            <KButton.Root
              class={skillAction() === "install" ? "kb-btn accent" : "kb-btn danger"}
              onClick={() => void applyAgentSkillAction()}
              disabled={skillInstalling()}
            >
              {skillDialogSubmitLabel()}
            </KButton.Root>
          </div>
        }
      >
        <Show when={skillPreview()}>
          {(preview) => (
            <div class="ai-skill-preview">
              <div class="ai-skill-preview-header">
                <div>
                  <span class="kb-label">{t("ai.previewResult")}</span>
                  <strong>{selectedAgentTarget()?.displayName ?? preview().targetAgent}</strong>
                </div>
                <StatusBadge
                  state={targetDetectedTone(preview().detectedState ?? "unknown")}
                  label={previewInstallTypeLabel(preview())}
                />
              </div>
              <Hint>{skillDialogHint()}</Hint>
              <div class="ai-preview-location">
                <span class="kb-label">{t("ai.skillPreviewLocation")}</span>
                <code>{preview().rootPath || selectedAgentTarget()?.globalPath || "-"}</code>
              </div>
              <div class="ai-preview-tree">
                <span class="kb-label">{t("ai.skillPreviewTree")}</span>
                <Show when={!isPreviewEmpty()} fallback={<div class="ai-empty-state">{t("ai.skillPreviewEmpty")}</div>}>
                  <code class="settings-mcp-config-code ai-tree-code">{previewTreeText()}</code>
                </Show>
              </div>
              <Show when={preview().warnings.length > 0}>
                <div class="ai-preview-warnings">
                  <For each={preview().warnings}>
                    {(warning) => (
                      <Hint variant={warning.severity === "error" ? "error" : "info"}>
                        {skillWarningMessage(warning, preview().operation)}
                      </Hint>
                    )}
                  </For>
                </div>
              </Show>
            </div>
          )}
        </Show>
      </Modal>
    </div>
  );
}
