import * as KButton from "@kobalte/core/button";
import * as KCheckbox from "@kobalte/core/checkbox";
import * as KSelect from "@kobalte/core/select";
import * as KTextField from "@kobalte/core/text-field";
import { queryOptions, useQuery } from "@tanstack/solid-query";
import { createEffect, createMemo, createSignal, For, Show } from "solid-js";

import { useI18n } from "../../i18n/context";
import { SUPPORTED_LOCALES, type AppLocale } from "../../i18n/locale";
import type {
  McpClientPreset,
  McpServerConfig,
  McpServerStatus,
  ProxyRule,
  TopologySnapshot
} from "../../lib/types";
import { useTheme, type ThemeMode } from "../../lib/theme";
import { listRules, scanTopology } from "../rules/api";
import { getMcpServerStatus, updateMcpServerConfig } from "./api";
import FlagCn from "../../assets/flag-cn.svg?url";
import FlagUs from "../../assets/flag-us.svg?url";
import FlagHk from "../../assets/flag-hk.svg?url";
import FlagJp from "../../assets/flag-jp.svg?url";

const LOCALE_FLAG: Record<AppLocale, string> = {
  "zh-CN": FlagCn,
  "en-US": FlagUs,
  "zh-HK": FlagHk,
  "ja-JP": FlagJp
};

const localeOptions: { value: AppLocale; label: string }[] = SUPPORTED_LOCALES.map((locale) => ({
  value: locale,
  label: locale
}));

const THEME_ICONS: Record<ThemeMode, string> = {
  light: "☀️",
  dark: "🌙",
  auto: "⚙️"
};

const themeOptions: { value: ThemeMode; labelKey: string }[] = [
  { value: "light", labelKey: "settings.themeLight" },
  { value: "dark", labelKey: "settings.themeDark" },
  { value: "auto", labelKey: "settings.themeAuto" }
];

const EMPTY_MCP_CONFIG: McpServerConfig = {
  enabled: false,
  server_name: "wsl-bridge",
  listen_port: 13746,
  api_token: "",
  expose_topology_read: true,
  expose_rule_config: true
};

function toLocalTime(value: string | null | undefined) {
  if (!value) return "-";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

function presetOptionLabel(preset: McpClientPreset) {
  return `${preset.label} (${preset.format})`;
}

export function SettingsPage() {
  const { locale, setLocale, t } = useI18n();
  const { mode: themeMode, setMode: setThemeMode } = useTheme();

  const [mcpDraft, setMcpDraft] = createSignal<McpServerConfig>(EMPTY_MCP_CONFIG);
  const [mcpDirty, setMcpDirty] = createSignal(false);
  const [mcpSaving, setMcpSaving] = createSignal(false);
  const [message, setMessage] = createSignal<{ type: "info" | "error"; text: string } | null>(null);
  const [selectedPresetId, setSelectedPresetId] = createSignal("claude-code");

  const topologyQuery = useQuery(() =>
    queryOptions<TopologySnapshot>({
      queryKey: ["settings", "topology"],
      queryFn: scanTopology,
      staleTime: 60_000
    })
  );

  const rulesQuery = useQuery(() =>
    queryOptions<ProxyRule[]>({
      queryKey: ["settings", "rules"],
      queryFn: listRules,
      staleTime: 60_000
    })
  );

  const mcpStatusQuery = useQuery(() =>
    queryOptions<McpServerStatus>({
      queryKey: ["settings", "mcp-status"],
      queryFn: getMcpServerStatus,
      staleTime: 0
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

  const enabledToolCount = createMemo(
    () => mcpStatusQuery.data?.tools.filter((tool) => tool.enabled).length ?? 0
  );

  const topologySummary = createMemo(() => ({
    wsl: topologyQuery.data?.wsl.length ?? 0,
    hyperv: topologyQuery.data?.hyperv.length ?? 0,
    rules: rulesQuery.data?.filter((rule) => rule.type === "tcp_fwd" || rule.type === "udp_fwd").length ?? 0,
    lastScan: topologyQuery.data?.timestamp ?? null
  }));

  const selectedPreset = createMemo(
    () => mcpStatusQuery.data?.client_presets.find((preset) => preset.id === selectedPresetId()) ?? null
  );

  async function refreshMcpStatus() {
    await mcpStatusQuery.refetch();
    setMessage({ type: "info", text: t("settings.mcpReloaded") });
  }

  async function saveMcpConfig() {
    const draft = mcpDraft();
    if (!draft.server_name.trim()) {
      setMessage({ type: "error", text: t("settings.mcpValidationServerName") });
      return;
    }
    if (!draft.api_token.trim()) {
      setMessage({ type: "error", text: t("settings.mcpValidationToken") });
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
      setMessage({ type: "info", text: t("settings.mcpSaved") });
    } catch (err) {
      setMessage({ type: "error", text: String(err) });
    } finally {
      setMcpSaving(false);
    }
  }

  async function copyText(text: string, successKey: string) {
    try {
      await navigator.clipboard.writeText(text);
      setMessage({ type: "info", text: t(successKey) });
    } catch (err) {
      setMessage({ type: "error", text: String(err) });
    }
  }

  function regenerateToken() {
    const token = `wb_${crypto.randomUUID().replaceAll("-", "")}${crypto.randomUUID().replaceAll("-", "")}`;
    updateDraft("api_token", token);
  }

  function updateDraft<K extends keyof McpServerConfig>(key: K, value: McpServerConfig[K]) {
    setMcpDraft((prev) => ({ ...prev, [key]: value }));
    setMcpDirty(true);
  }

  return (
    <div class="page">
      <section class="panel">
        <div class="panel-title">
          <h2>{t("settings.title")}</h2>
        </div>
        <div class="muted">{t("settings.subtitle")}</div>
      </section>

      <section class="panel">
        <h2>{t("settings.themeTitle")}</h2>
        <div class="settings-lang-row">
          <label class="kb-label">{t("settings.themeLabel")}</label>
          <KSelect.Root<{ value: ThemeMode; labelKey: string }>
            value={themeOptions.find((opt) => opt.value === themeMode())}
            onChange={(option) => option && setThemeMode(option.value)}
            options={themeOptions}
            optionValue="value"
            optionTextValue="labelKey"
            itemComponent={(itemProps) => (
              <KSelect.Item item={itemProps.item} class="kb-select-item">
                <span style="margin-right:6px">{THEME_ICONS[itemProps.item.rawValue.value]}</span>
                <KSelect.ItemLabel>{t(itemProps.item.rawValue.labelKey)}</KSelect.ItemLabel>
              </KSelect.Item>
            )}
          >
            <KSelect.Trigger class="kb-input settings-language-select">
              <KSelect.Value<{ value: ThemeMode; labelKey: string }>>{(state) => (
                <>
                  <span style="margin-right:6px">{THEME_ICONS[state.selectedOption()?.value ?? "auto"]}</span>
                  {t(state.selectedOption()?.labelKey ?? "settings.themeAuto")}
                </>
              )}</KSelect.Value>
              <KSelect.Icon class="kb-select-icon">▾</KSelect.Icon>
            </KSelect.Trigger>
            <KSelect.Portal>
              <KSelect.Content class="kb-select-content">
                <KSelect.Listbox class="kb-select-listbox" />
              </KSelect.Content>
            </KSelect.Portal>
          </KSelect.Root>
        </div>
      </section>

      <section class="panel">
        <h2>{t("settings.languageTitle")}</h2>
        <div class="settings-lang-row">
          <label class="kb-label">{t("settings.languageLabel")}</label>
          <KSelect.Root<{ value: AppLocale; label: string }>
            value={localeOptions.find((opt) => opt.value === locale())}
            onChange={(option) => option && setLocale(option.value)}
            options={localeOptions}
            optionValue="value"
            optionTextValue="label"
            itemComponent={(itemProps) => (
              <KSelect.Item item={itemProps.item} class="kb-select-item">
                <img
                  src={LOCALE_FLAG[itemProps.item.rawValue.value]}
                  style="width:20px;height:14px;margin-right:6px;vertical-align:middle"
                />
                <KSelect.ItemLabel>{t(`locale.${itemProps.item.rawValue.value}`)}</KSelect.ItemLabel>
              </KSelect.Item>
            )}
          >
            <KSelect.Trigger class="kb-input settings-language-select">
              <KSelect.Value<{ value: AppLocale; label: string }>>{(state) => (
                <>
                  <img
                    src={LOCALE_FLAG[state.selectedOption()?.value ?? "en-US"]}
                    style="width:20px;height:14px;margin-right:6px;vertical-align:middle"
                  />
                  {t(`locale.${state.selectedOption()?.value ?? "en-US"}`)}
                </>
              )}</KSelect.Value>
              <KSelect.Icon class="kb-select-icon">▾</KSelect.Icon>
            </KSelect.Trigger>
            <KSelect.Portal>
              <KSelect.Content class="kb-select-content">
                <KSelect.Listbox class="kb-select-listbox" />
              </KSelect.Content>
            </KSelect.Portal>
          </KSelect.Root>
        </div>
      </section>

      <section class="panel">
        <div class="panel-title">
          <div>
            <h2>{t("settings.mcpTitle")}</h2>
            <div class="muted settings-mcp-subtitle">{t("settings.mcpSubtitle")}</div>
          </div>
          <div class="runtime-tools">
            <KButton.Root class="kb-btn ghost" onClick={refreshMcpStatus} disabled={mcpStatusQuery.isFetching}>
              {t("common.refresh")}
            </KButton.Root>
            <KButton.Root
              class="kb-btn ghost"
              onClick={() => void copyText(mcpStatusQuery.data?.base_url ?? "", "settings.mcpBaseUrlCopied")}
              disabled={!mcpStatusQuery.data?.base_url}
            >
              {t("settings.mcpCopyBaseUrl")}
            </KButton.Root>
            <KButton.Root class="kb-btn accent" onClick={saveMcpConfig} disabled={mcpSaving()}>
              {t("settings.mcpSave")}
            </KButton.Root>
          </div>
        </div>

        <div class="settings-mcp-summary">
          <div class="dashboard-card">
            <div class="muted">{t("settings.mcpSummaryStatus")}</div>
            <div class="dashboard-stat">
              {mcpStatusQuery.data?.running ? t("common.running") : t("common.stopped")}
            </div>
          </div>
          <div class="dashboard-card">
            <div class="muted">{t("settings.mcpSummaryTopology")}</div>
            <div class="dashboard-stat">
              {t("settings.mcpSummaryTopologyValue", {
                wsl: topologySummary().wsl,
                hyperv: topologySummary().hyperv
              })}
            </div>
          </div>
          <div class="dashboard-card">
            <div class="muted">{t("settings.mcpSummaryRules")}</div>
            <div class="dashboard-stat">
              {t("settings.mcpSummaryRulesValue", { count: topologySummary().rules })}
            </div>
          </div>
          <div class="dashboard-card">
            <div class="muted">{t("settings.mcpSummaryTools")}</div>
            <div class="dashboard-stat">
              {t("settings.mcpSummaryToolsValue", { count: enabledToolCount() })}
            </div>
          </div>
        </div>

        <div class="settings-mcp-grid">
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

        <div class="settings-mcp-grid">
          <div class="kb-field">
            <label class="kb-label">{t("settings.mcpEnabled")}</label>
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

          <div class="kb-field">
            <label class="kb-label">{t("settings.mcpCapabilities")}</label>
            <div class="checks">
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
            </div>
          </div>
        </div>

        <div class="settings-mcp-token">
          <KTextField.Root
            class="kb-field"
            value={mcpDraft().api_token}
            onChange={(value) => updateDraft("api_token", value)}
          >
            <KTextField.Label>{t("settings.mcpApiToken")}</KTextField.Label>
            <KTextField.Input class="kb-input settings-mcp-token-input" />
          </KTextField.Root>
          <div class="runtime-tools">
            <KButton.Root class="kb-btn ghost" onClick={regenerateToken}>
              {t("settings.mcpRegenerateToken")}
            </KButton.Root>
            <KButton.Root class="kb-btn ghost" onClick={() => void copyText(mcpDraft().api_token, "settings.mcpTokenCopied")}>
              {t("settings.mcpCopyToken")}
            </KButton.Root>
          </div>
        </div>

        <div class="settings-mcp-paths">
          <div class="kb-field">
            <label class="kb-label">{t("settings.mcpBaseUrl")}</label>
            <input class="kb-input" readonly value={mcpStatusQuery.data?.base_url ?? ""} />
          </div>
        </div>

        <Show when={mcpStatusQuery.data?.last_error}>
          {(err) => <div class="hint error">{err()}</div>}
        </Show>

        <div class="settings-mcp-tool-list">
          <div class="kb-field">
            <label class="kb-label">{t("settings.mcpToolList")}</label>
          </div>
          <div class="table-wrap">
            <table class="rules-table">
              <thead>
                <tr>
                  <th>{t("settings.mcpToolName")}</th>
                  <th>{t("settings.mcpToolDescription")}</th>
                  <th>{t("settings.mcpToolStatus")}</th>
                </tr>
              </thead>
              <tbody>
                <Show
                  when={(mcpStatusQuery.data?.tools.length ?? 0) > 0}
                  fallback={
                    <tr>
                      <td colspan={3} class="muted">{t("common.loading")}</td>
                    </tr>
                  }
                >
                  <For each={mcpStatusQuery.data?.tools ?? []}>
                    {(tool) => (
                      <tr>
                        <td>{tool.name}</td>
                        <td>{tool.description}</td>
                        <td>{tool.enabled ? t("common.enabled") : t("common.disabled")}</td>
                      </tr>
                    )}
                  </For>
                </Show>
              </tbody>
            </table>
          </div>
        </div>

        <div class="settings-mcp-client-header">
          <div class="kb-field">
            <label class="kb-label">{t("settings.mcpClientPresets")}</label>
          </div>
          <div class="runtime-tools">
            <KSelect.Root<McpClientPreset>
              value={selectedPreset()}
              onChange={(option) => option && setSelectedPresetId(option.id)}
              options={mcpStatusQuery.data?.client_presets ?? []}
              optionValue="id"
              optionTextValue="label"
              itemComponent={(itemProps) => (
                <KSelect.Item item={itemProps.item} class="kb-select-item">
                  <KSelect.ItemLabel>{presetOptionLabel(itemProps.item.rawValue)}</KSelect.ItemLabel>
                </KSelect.Item>
              )}
            >
              <KSelect.Trigger class="kb-input settings-mcp-preset-select">
                <KSelect.Value<McpClientPreset>>{(state) =>
                  state.selectedOption() ? presetOptionLabel(state.selectedOption() as McpClientPreset) : ""
                }</KSelect.Value>
                <KSelect.Icon class="kb-select-icon">▾</KSelect.Icon>
              </KSelect.Trigger>
              <KSelect.Portal>
                <KSelect.Content class="kb-select-content">
                  <KSelect.Listbox class="kb-select-listbox" />
                </KSelect.Content>
              </KSelect.Portal>
            </KSelect.Root>
            <KButton.Root
              class="kb-btn ghost"
              onClick={() => void copyText(selectedPreset()?.content ?? "", "settings.mcpPresetCopied")}
              disabled={!selectedPreset()?.content}
            >
              {t("settings.mcpCopyConfig")}
            </KButton.Root>
          </div>
        </div>

        <div class="kb-field">
          <label class="kb-label">{t("settings.mcpClientConfig")}</label>
          <textarea
            class="kb-input settings-mcp-config"
            readonly
            value={selectedPreset()?.content ?? ""}
          />
        </div>

        <div class="hint info">
          {t("settings.mcpHint")}
        </div>

        <Show when={message()}>
          {(msg) => <div class={`hint ${msg().type === "error" ? "error" : "info"}`}>{msg().text}</div>}
        </Show>
      </section>
    </div>
  );
}
