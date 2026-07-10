import * as KButton from "@kobalte/core/button";
import * as KCheckbox from "@kobalte/core/checkbox";
import * as KSelect from "@kobalte/core/select";
import { Link } from "@tanstack/solid-router";
import { queryOptions, useQuery } from "@tanstack/solid-query";
import { createEffect, createSignal } from "solid-js";

import "./SettingsPage.css";

import { useI18n } from "../../i18n/context";
import { SUPPORTED_LOCALES, type AppLocale } from "../../i18n/locale";
import type { AppSettings, CloseBehavior, McpServerStatus } from "../../lib/types";
import { useTheme, type ThemeMode } from "../../lib/theme";
import { getAppSettings, getMcpServerStatus, updateAppSettings } from "./api";
import { useToast } from "../../lib/Toast";
import FlagCn from "../../assets/flag-cn.svg?url";
import FlagUs from "../../assets/flag-us.svg?url";
import FlagHk from "../../assets/flag-hk.svg?url";
import FlagJp from "../../assets/flag-jp.svg?url";
import IconDesktop from "../../assets/desktop.svg?url";
import IconSun from "../../assets/sun.svg?url";
import IconMoon from "../../assets/moon.svg?url";
import { MetricCard, PageHeader, SectionCard, SparkIcon, StatusBadge } from "../../lib/ui";

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
  light: IconSun,
  dark: IconMoon,
  auto: IconDesktop
};

const themeOptions: { value: ThemeMode; labelKey: string }[] = [
  { value: "light", labelKey: "settings.themeLight" },
  { value: "dark", labelKey: "settings.themeDark" },
  { value: "auto", labelKey: "settings.themeAuto" }
];

const closeBehaviorOptions: { value: CloseBehavior; labelKey: string }[] = [
  { value: "ask", labelKey: "settings.closeBehaviorAsk" },
  { value: "minimize", labelKey: "settings.closeBehaviorMinimize" },
  { value: "exit", labelKey: "settings.closeBehaviorExit" }
];

const EMPTY_APP_SETTINGS: AppSettings = {
  close_behavior: "ask",
  show_tray_on_start: true
};

export function SettingsPage() {
  const { locale, setLocale, t } = useI18n();
  const { mode: themeMode, setMode: setThemeMode } = useTheme();
  const toast = useToast();

  const [appSettingsDraft, setAppSettingsDraft] = createSignal<AppSettings>(EMPTY_APP_SETTINGS);
  const [appSettingsDirty, setAppSettingsDirty] = createSignal(false);
  const [appSettingsSaving, setAppSettingsSaving] = createSignal(false);

  const appSettingsQuery = useQuery(() =>
    queryOptions<AppSettings>({
      queryKey: ["settings", "app-settings"],
      queryFn: getAppSettings,
      staleTime: 0
    })
  );

  const mcpStatusQuery = useQuery(() =>
    queryOptions<McpServerStatus>({
      queryKey: ["settings", "mcp-status-summary"],
      queryFn: getMcpServerStatus,
      staleTime: 0
    })
  );

  createEffect(() => {
    const remote = appSettingsQuery.data;
    if (!remote || appSettingsDirty()) return;
    setAppSettingsDraft(remote);
  });

  async function saveAppSettings() {
    try {
      setAppSettingsSaving(true);
      await updateAppSettings(appSettingsDraft());
      setAppSettingsDirty(false);
      await appSettingsQuery.refetch();
      toast.info(t("settings.appSettingsSaved"));
    } catch (err) {
      toast.error(String(err));
    } finally {
      setAppSettingsSaving(false);
    }
  }

  function updateAppSettingsDraft<K extends keyof AppSettings>(key: K, value: AppSettings[K]) {
    setAppSettingsDraft((prev) => ({ ...prev, [key]: value }));
    setAppSettingsDirty(true);
  }

  return (
    <div class="page">
      <PageHeader title={t("settings.title")} />

      <div class="metric-grid">
        <MetricCard
          label={t("settings.themeLabel")}
          value={t(`settings.theme${themeMode().charAt(0).toUpperCase()}${themeMode().slice(1)}` as never)}
          detail={t(`locale.${locale()}`)}
        />
        <MetricCard
          label={t("settings.aiIntegrationMetric")}
          value={
            <StatusBadge
              state={mcpStatusQuery.data?.running ? "running" : "stopped"}
              label={mcpStatusQuery.data?.running ? t("common.running") : t("common.stopped")}
            />
          }
          detail={t("settings.aiIntegrationMetricDetail")}
        />
      </div>

      <SectionCard title={t("settings.appearanceTitle")}>
        <div class="settings-appearance-grid">
          <div class="settings-field-row">
            <label class="kb-label">{t("settings.themeLabel")}</label>
            <KSelect.Root<{ value: ThemeMode; labelKey: string }>
              value={themeOptions.find((opt) => opt.value === themeMode())}
              onChange={(option) => option && setThemeMode(option.value)}
              options={themeOptions}
              optionValue="value"
              optionTextValue="labelKey"
              itemComponent={(itemProps) => (
                <KSelect.Item item={itemProps.item} class="kb-select-item">
                  <img
                    src={THEME_ICONS[itemProps.item.rawValue.value]}
                    style="width:18px;height:18px;margin-right:6px;vertical-align:middle"
                  />
                  <KSelect.ItemLabel>{t(itemProps.item.rawValue.labelKey)}</KSelect.ItemLabel>
                </KSelect.Item>
              )}
            >
              <KSelect.Trigger class="kb-input kb-select-trigger settings-select-narrow">
                <KSelect.Value<{ value: ThemeMode; labelKey: string }>>{(state) => (
                  <>
                    <img
                      src={THEME_ICONS[state.selectedOption()?.value ?? "auto"]}
                      style="width:18px;height:18px;margin-right:6px;vertical-align:middle"
                    />
                    {t(state.selectedOption()?.labelKey ?? "settings.themeAuto")}
                  </>
                )}</KSelect.Value>
                <KSelect.Icon class="kb-select-icon"><span class="kb-select-icon-triangle"></span></KSelect.Icon>
              </KSelect.Trigger>
              <KSelect.Portal>
                <KSelect.Content class="kb-select-content">
                  <KSelect.Listbox class="kb-select-listbox" />
                </KSelect.Content>
              </KSelect.Portal>
            </KSelect.Root>
          </div>

          <div class="settings-field-row">
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
              <KSelect.Trigger class="kb-input kb-select-trigger settings-select-narrow">
                <KSelect.Value<{ value: AppLocale; label: string }>>{(state) => (
                  <>
                    <img
                      src={LOCALE_FLAG[state.selectedOption()?.value ?? "en-US"]}
                      style="width:20px;height:14px;margin-right:6px;vertical-align:middle"
                    />
                    {t(`locale.${state.selectedOption()?.value ?? "en-US"}`)}
                  </>
                )}</KSelect.Value>
                <KSelect.Icon class="kb-select-icon"><span class="kb-select-icon-triangle"></span></KSelect.Icon>
              </KSelect.Trigger>
              <KSelect.Portal>
                <KSelect.Content class="kb-select-content">
                  <KSelect.Listbox class="kb-select-listbox" />
                </KSelect.Content>
              </KSelect.Portal>
            </KSelect.Root>
          </div>
        </div>
      </SectionCard>

      <SectionCard
        title={t("settings.lifecycleTitle")}
        actions={
          <KButton.Root
            class="kb-btn accent"
            onClick={saveAppSettings}
            disabled={appSettingsSaving() || !appSettingsDirty()}
          >
            {t("settings.lifecycleSave")}
          </KButton.Root>
        }
      >
        <div class="settings-lifecycle-grid">
          <div class="settings-field-row">
            <label class="kb-label">{t("settings.closeBehaviorLabel")}</label>
            <KSelect.Root<{ value: CloseBehavior; labelKey: string }>
              value={closeBehaviorOptions.find((opt) => opt.value === appSettingsDraft().close_behavior)}
              onChange={(option) => option && updateAppSettingsDraft("close_behavior", option.value)}
              options={closeBehaviorOptions}
              optionValue="value"
              optionTextValue="labelKey"
              itemComponent={(itemProps) => (
                <KSelect.Item item={itemProps.item} class="kb-select-item">
                  <KSelect.ItemLabel>{t(itemProps.item.rawValue.labelKey)}</KSelect.ItemLabel>
                </KSelect.Item>
              )}
            >
              <KSelect.Trigger class="kb-input kb-select-trigger settings-select-narrow">
                <KSelect.Value<{ value: CloseBehavior; labelKey: string }>>{(state) =>
                  t(state.selectedOption()?.labelKey ?? "settings.closeBehaviorAsk")
                }</KSelect.Value>
                <KSelect.Icon class="kb-select-icon"><span class="kb-select-icon-triangle"></span></KSelect.Icon>
              </KSelect.Trigger>
              <KSelect.Portal>
                <KSelect.Content class="kb-select-content">
                  <KSelect.Listbox class="kb-select-listbox" />
                </KSelect.Content>
              </KSelect.Portal>
            </KSelect.Root>
          </div>

          <div class="settings-field-row settings-lifecycle-toggle">
            <div class="muted">{t("settings.showTrayOnStartLabel")}</div>
            <KCheckbox.Root
              checked={appSettingsDraft().show_tray_on_start}
              onChange={(checked) => updateAppSettingsDraft("show_tray_on_start", checked)}
              class="kb-checkbox"
            >
              <KCheckbox.Input />
              <KCheckbox.Control class="kb-checkbox-control">
                <KCheckbox.Indicator class="kb-checkbox-indicator" />
              </KCheckbox.Control>
              <KCheckbox.Label class="kb-checkbox-label">{t("settings.showTrayOnStartHint")}</KCheckbox.Label>
            </KCheckbox.Root>
          </div>
        </div>
      </SectionCard>

      <SectionCard
        title={t("settings.aiIntegrationTitle")}
        subtitle={t("settings.aiIntegrationSubtitle")}
        actions={
          <Link to="/ai" class="kb-btn accent">
            <SparkIcon size={15} />
            {t("settings.openAiIntegration")}
          </Link>
        }
      >
        <div class="settings-ai-redirect">
          <div>
            <strong>{t("settings.aiIntegrationMoved")}</strong>
            <p>{t("settings.aiIntegrationHint")}</p>
          </div>
          <StatusBadge
            state={mcpStatusQuery.data?.running ? "running" : "stopped"}
            label={mcpStatusQuery.data?.running ? t("common.running") : t("common.stopped")}
          />
        </div>
      </SectionCard>
    </div>
  );
}
