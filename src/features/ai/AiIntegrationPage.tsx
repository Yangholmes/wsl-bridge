import * as KButton from "@kobalte/core/button";
import * as KCheckbox from "@kobalte/core/checkbox";
import * as KTextField from "@kobalte/core/text-field";
import { queryOptions, useQuery } from "@tanstack/solid-query";
import { createEffect, createMemo, createSignal, For, Show } from "solid-js";

import "./AiIntegrationPage.css";

import { useI18n } from "../../i18n/context";
import type { McpClientPreset, McpServerConfig, McpServerStatus } from "../../lib/types";
import { CopyIcon, MetricCard, PageHeader, SectionCard, StatusBadge } from "../../lib/ui";
import { Hint } from "../../lib/Hint";
import { useToast } from "../../lib/Toast";
import { getMcpServerStatus, updateMcpServerConfig } from "../settings/api";
import {
  installAgentSkillPreview,
  listAgentTargets,
  type AgentSkillTargetsPayload,
  type AgentSkillPreviewPayload,
  type AgentSkillTarget
} from "./api";

const EMPTY_MCP_CONFIG: McpServerConfig = {
  enabled: false,
  server_name: "wsl-bridge",
  listen_port: 13746,
  api_token: "",
  expose_topology_read: true,
  expose_rule_config: true,
  expose_traffic_stats: true
};

function presetOptionLabel(preset: McpClientPreset) {
  return `${preset.label} (${preset.format})`;
}

function targetInstallLabel(target: AgentSkillTarget) {
  if (target.supportsNativeSkill) return "native-skill";
  return target.fallbackToAgentsDir ? ".agents/skills" : target.installType;
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
      queryKey: ["ai", "agent-targets", "project"],
      queryFn: () => listAgentTargets("project"),
      staleTime: 30_000
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

  const selectedAgentTarget = createMemo(
    () => agentTargetsQuery.data?.targets.find((target) => target.id === selectedAgentTargetId()) ?? null
  );

  const apiModeLabel = createMemo(() => t("ai.modePlanning"));

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
    if (!draft.api_token.trim()) {
      toast.error(t("settings.mcpValidationToken"));
      return;
    }

    try {
      setMcpSaving(true);
      await updateMcpServerConfig({
        ...draft,
        server_name: draft.server_name.trim(),
        api_token: draft.api_token.trim()
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

  function regenerateToken() {
    const token = `wb_${crypto.randomUUID().replaceAll("-", "")}${crypto.randomUUID().replaceAll("-", "")}`;
    updateDraft("api_token", token);
  }

  function runDiagnostics() {
    setDiagnosticsRan(true);
    toast.info(t("ai.diagnosticsCompleted"));
  }

  async function previewAgentSkillInstall(target: AgentSkillTarget) {
    try {
      setSelectedAgentTargetId(target.id);
      setSkillPreviewLoading(true);
      const preview = await installAgentSkillPreview({
        target: target.id,
        scope: agentTargetsQuery.data?.scope ?? "project",
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

  function updateDraft<K extends keyof McpServerConfig>(key: K, value: McpServerConfig[K]) {
    setMcpDraft((prev) => ({ ...prev, [key]: value }));
    setMcpDirty(true);
  }

  return (
    <div class="page ai-page">
      <PageHeader
        title={t("ai.title")}
        eyebrow={t("ai.eyebrow")}
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
            <KButton.Root class="kb-btn ghost" onClick={runDiagnostics}>
              {t("ai.runDiagnostics")}
            </KButton.Root>
          </div>
        }
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
        <MetricCard label={t("ai.apiVersion")} value="phase3.ai.v1" detail={apiModeLabel()} />
        <MetricCard
          label={t("ai.exposedCapabilities")}
          value={`${enabledResourceCount()} / ${enabledToolCount()}`}
          detail={t("ai.resourcesToolsDetail")}
        />
      </div>

      <Show when={mcpStatusQuery.data?.last_error}>
        {(err) => <Hint variant="error">{err()}</Hint>}
      </Show>

      <div class="ai-grid">
        <SectionCard
          title={t("ai.mcpServiceTitle")}
          subtitle={t("ai.mcpServiceSubtitle")}
          actions={
            <KButton.Root class="kb-btn ghost" onClick={refreshMcpStatus} disabled={mcpStatusQuery.isFetching}>
              {t("common.refresh")}
            </KButton.Root>
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

          <div class="ai-token-row">
            <KTextField.Root
              class="kb-field ai-token-field"
              value={mcpDraft().api_token}
              onChange={(value) => updateDraft("api_token", value)}
            >
              <KTextField.Label>{t("settings.mcpApiToken")}</KTextField.Label>
              <KTextField.Input class="kb-input" />
            </KTextField.Root>
            <div class="runtime-tools">
              <KButton.Root class="kb-btn ghost" onClick={regenerateToken}>
                {t("settings.mcpRegenerateToken")}
              </KButton.Root>
              <KButton.Root class="kb-btn ghost" onClick={() => void copyText(mcpDraft().api_token, "settings.mcpTokenCopied")}>
                <CopyIcon size={14} />
                {t("settings.mcpCopyToken")}
              </KButton.Root>
            </div>
          </div>

          <div class="ai-baseurl-row">
            <input class="kb-input" readonly value={mcpStatusQuery.data?.base_url ?? ""} />
            <KButton.Root
              class="kb-btn ghost"
              onClick={() => void copyText(mcpStatusQuery.data?.base_url ?? "", "settings.mcpBaseUrlCopied")}
              disabled={!mcpStatusQuery.data?.base_url}
            >
              <CopyIcon size={14} />
              {t("settings.mcpCopyBaseUrl")}
            </KButton.Root>
          </div>

          <div class="ai-save-row">
            <KButton.Root class="kb-btn accent" onClick={saveMcpConfig} disabled={mcpSaving() || !mcpDirty()}>
              {t("settings.mcpSave")}
            </KButton.Root>
          </div>
        </SectionCard>

        <SectionCard
          title={t("ai.agentSkillTitle")}
          subtitle={t("ai.agentSkillSubtitle")}
          actions={
            <KButton.Root
              class="kb-btn ghost"
              onClick={() => void agentTargetsQuery.refetch()}
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
                >
                  <div>
                    <div class="ai-agent-name">{target.displayName || t(`ai.agent.${target.id}` as never)}</div>
                    <div class="muted">{targetInstallLabel(target)}</div>
                  </div>
                  <div class="runtime-tools">
                    <StatusBadge
                      state={target.dryRunSupported ? "ready" : "unknown"}
                      label={target.dryRunSupported ? t("ai.dryRunSupported") : t("ai.unavailable")}
                    />
                    <KButton.Root
                      class="kb-btn ghost"
                      onClick={() => void previewAgentSkillInstall(target)}
                      disabled={!target.dryRunSupported || skillPreviewLoading()}
                    >
                      {skillPreviewLoading() && selectedAgentTargetId() === target.id
                        ? t("common.loading")
                        : t("ai.previewInstall")}
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
          <Show when={skillPreview()}>
            {(preview) => (
              <div class="ai-skill-preview">
                <div class="ai-skill-preview-header">
                  <div>
                    <span class="kb-label">{t("ai.previewResult")}</span>
                    <strong>{selectedAgentTarget()?.displayName ?? preview().targetAgent}</strong>
                  </div>
                  <StatusBadge state={preview().ok ? "ready" : "error"} label={preview().mode} />
                </div>
                <div class="ai-preview-meta">
                  <span>{preview().installType}</span>
                  <span>{preview().skill.canonicalPackage}</span>
                </div>
                <div class="ai-preview-list">
                  <For each={preview().writes}>
                    {(write) => (
                      <div class="ai-preview-row">
                        <span>{write.action}</span>
                        <code>{write.path}</code>
                      </div>
                    )}
                  </For>
                </div>
                <Show when={preview().warnings.length > 0}>
                  <div class="ai-preview-warnings">
                    <For each={preview().warnings}>
                      {(warning) => <Hint variant={warning.severity === "error" ? "error" : "info"}>{warning.message}</Hint>}
                    </For>
                  </div>
                </Show>
              </div>
            )}
          </Show>
        </SectionCard>

        <SectionCard title={t("ai.capabilitiesTitle")} subtitle={t("ai.capabilitiesSubtitle")}>
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
          <div class="ai-policy-card">
            <span class="muted">{t("ai.writeMode")}</span>
            <strong>{apiModeLabel()}</strong>
            <p>{t("ai.writeModeHint")}</p>
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
          <div class="ai-diagnostics-list">
            <div class="ai-diagnostic-item">
              <span>{t("ai.diagnosticMcp")}</span>
              <StatusBadge
                state={mcpStatusQuery.data?.running ? "running" : "stopped"}
                label={mcpStatusQuery.data?.running ? t("common.running") : t("common.stopped")}
              />
            </div>
            <div class="ai-diagnostic-item">
              <span>{t("ai.diagnosticTools")}</span>
              <StatusBadge state={enabledToolCount() > 0 ? "ready" : "unknown"} label={`${enabledToolCount()}`} />
            </div>
            <div class="ai-diagnostic-item">
              <span>{t("ai.diagnosticSkill")}</span>
              <StatusBadge state="ready" label={t("ai.genericFallbackReady")} />
            </div>
            <Show when={diagnosticsRan()}>
              <div class="ai-diagnostic-result">{t("ai.diagnosticsCompleted")}</div>
            </Show>
          </div>
        </SectionCard>

        <SectionCard title={t("ai.auditTitle")} subtitle={t("ai.auditSubtitle")}>
          <div class="ai-empty-state">{t("ai.auditEmpty")}</div>
        </SectionCard>
      </div>
    </div>
  );
}
