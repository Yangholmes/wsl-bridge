import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import * as KCheckbox from "@kobalte/core/checkbox";

import { getTrafficWindowData } from "../rules/api";
import type { ProxyRule, TrafficSample, TrafficWindowData } from "../../lib/types";
import { useI18n } from "../../i18n/context";
import { DropdownPanel } from "../../lib/DropdownPanel";

import "uplot/dist/uPlot.min.css";

type TrafficMetric = "total" | "in" | "out" | "connections";
type UPlotLike = {
  destroy: () => void;
  setData: (data: (number | null)[][]) => void;
  setSize: (size: { width: number; height: number }) => void;
};

const WINDOW_OPTIONS = [30, 60, 120] as const;
const REFRESH_OPTIONS = [1, 2, 5] as const;
const METRIC_OPTIONS: TrafficMetric[] = ["total", "in", "out", "connections"];
const SERIES_COLORS = ["#0a64ff", "#ff6b35", "#16a34a", "#a855f7", "#d97706", "#0f766e"];

function sameIds(left: string[], right: string[]) {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

function metricValue(metric: TrafficMetric, sample: TrafficSample | undefined) {
  if (!sample) return 0;
  if (metric === "in") return sample.bytes_in;
  if (metric === "out") return sample.bytes_out;
  if (metric === "connections") return sample.connections;
  return sample.bytes_in + sample.bytes_out;
}

function formatMetricValue(metric: TrafficMetric, value: number | null | undefined) {
  if (value == null) return "-";
  if (metric === "connections") return String(value);
  if (value >= 1024 * 1024) return `${(value / (1024 * 1024)).toFixed(1)} MB`;
  if (value >= 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${value} B`;
}

function pad2(value: number) {
  return String(value).padStart(2, "0");
}

function formatXAxisTick(value: number) {
  const date = new Date(value * 1000);
  const hours = pad2(date.getHours());
  const minutes = pad2(date.getMinutes());
  const seconds = pad2(date.getSeconds());
  if (date.getSeconds() === 0) {
    return `${hours}:${minutes}`;
  }
  return `:${seconds}`;
}

function normalizeSelectedRuleIds(current: string[], valid: string[]) {
  const validSet = new Set(valid);
  return current.filter((ruleId) => validSet.has(ruleId));
}

function buildRuleColorMap(ruleIds: string[]) {
  return new Map(ruleIds.map((ruleId, index) => [ruleId, SERIES_COLORS[index % SERIES_COLORS.length]]));
}

type TrafficChartProps = {
  rules: ProxyRule[];
};

export function TrafficChart(props: TrafficChartProps) {
  const { t } = useI18n();

  const [metric, setMetric] = createSignal<TrafficMetric>("total");
  const [windowSeconds, setWindowSeconds] = createSignal<(typeof WINDOW_OPTIONS)[number]>(60);
  const [refreshSeconds, setRefreshSeconds] = createSignal<(typeof REFRESH_OPTIONS)[number]>(1);
  const [selectedRuleIds, setSelectedRuleIds] = createSignal<string[]>([]);
  const [windowRows, setWindowRows] = createSignal<TrafficWindowData[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [nowEpochSeconds, setNowEpochSeconds] = createSignal(Math.floor(Date.now() / 1000));
  const [configOpen, setConfigOpen] = createSignal(false);
  const [selectionInitialized, setSelectionInitialized] = createSignal(false);

  let chartHost: HTMLDivElement | undefined;
  let chart: UPlotLike | null = null;
  let uplotModule: any = null;
  let resizeObserver: ResizeObserver | null = null;
  let chartSignature = "";
  let refreshAbortVersion = 0;

  const availableRules = createMemo(() =>
    [...props.rules].sort((a, b) => Number(b.enabled) - Number(a.enabled) || a.name.localeCompare(b.name))
  );

  const selectedRules = createMemo(() => {
    const selected = new Set(selectedRuleIds());
    return availableRules().filter((rule) => selected.has(rule.id));
  });

  const ruleColorMap = createMemo(() => buildRuleColorMap(availableRules().map((rule) => rule.id)));

  const chartData = createMemo<(number | null)[][]>(() => {
    const end = nowEpochSeconds();
    const start = end - windowSeconds() + 1;
    const xAxis = Array.from({ length: windowSeconds() }, (_, index) => start + index);
    const ruleMap = new Map(windowRows().map((item) => [item.rule_id, item]));

    const series = selectedRules().map((rule) => {
      const samples = new Map((ruleMap.get(rule.id)?.samples ?? []).map((sample) => [sample.timestamp, sample]));
      return Array.from({ length: windowSeconds() }, (_, index) => {
        const timestamp = start + index;
        return metricValue(metric(), samples.get(timestamp));
      });
    });

    return [xAxis, ...series];
  });

  const totals = createMemo(() => {
    const totalsByRule = new Map(
      windowRows().map((item) => [
        item.rule_id,
        item.samples.reduce(
          (acc, sample) => {
            acc.bytesIn += sample.bytes_in;
            acc.bytesOut += sample.bytes_out;
            acc.connections += sample.connections;
            return acc;
          },
          { bytesIn: 0, bytesOut: 0, connections: 0 }
        )
      ])
    );

    return selectedRules().map((rule, index) => ({
      id: rule.id,
      name: rule.name,
      color: ruleColorMap().get(rule.id) ?? SERIES_COLORS[index % SERIES_COLORS.length],
      bytesIn: totalsByRule.get(rule.id)?.bytesIn ?? 0,
      bytesOut: totalsByRule.get(rule.id)?.bytesOut ?? 0,
      connections: totalsByRule.get(rule.id)?.connections ?? 0
    }));
  });

  const totalSummary = createMemo(() =>
    totals().reduce(
      (acc, item) => {
        acc.bytesIn += item.bytesIn;
        acc.bytesOut += item.bytesOut;
        acc.connections += item.connections;
        return acc;
      },
      { bytesIn: 0, bytesOut: 0, connections: 0 }
    )
  );

  createEffect(() => {
    const validRuleIds = availableRules().map((rule) => rule.id);
    const currentSelected = selectedRuleIds();
    const nextSelected = normalizeSelectedRuleIds(currentSelected, validRuleIds);

    if (!selectionInitialized() && validRuleIds.length > 0 && currentSelected.length === 0) {
      const initialSelection = availableRules()
        .filter((rule) => rule.enabled)
        .slice(0, 3)
        .map((rule) => rule.id);
      const resolvedInitial = initialSelection.length > 0 ? initialSelection : validRuleIds.slice(0, 3);
      setSelectedRuleIds(resolvedInitial);
      setSelectionInitialized(true);
      return;
    }

    if (!sameIds(nextSelected, currentSelected)) {
      setSelectedRuleIds(nextSelected);
    }

    if (!selectionInitialized() && validRuleIds.length > 0) {
      setSelectionInitialized(true);
    }
  });

  async function refreshWindowData(ruleIds: string[], version: number) {
    if (ruleIds.length === 0) {
      setWindowRows([]);
      setLoading(false);
      setError(null);
      setNowEpochSeconds(Math.floor(Date.now() / 1000));
      return;
    }

    try {
      setError(null);
      const rows = await getTrafficWindowData(ruleIds);
      if (version !== refreshAbortVersion) {
        return;
      }
      setWindowRows(rows);
    } catch (fetchError) {
      if (version !== refreshAbortVersion) {
        return;
      }
      setError(String(fetchError));
    } finally {
      if (version === refreshAbortVersion) {
        setLoading(false);
      }
    }
  }

  function setRuleSelected(ruleId: string, checked: boolean) {
    setSelectedRuleIds((prev) => {
      const exists = prev.includes(ruleId);
      if (checked) {
        return exists ? prev : [...prev, ruleId];
      }
      return exists ? prev.filter((item) => item !== ruleId) : prev;
    });
  }

  async function ensureChart() {
    if (!chartHost) return;
    if (!uplotModule) {
      uplotModule = await import("uplot");
    }

    const nextSignature = `${metric()}::${selectedRules()
      .map((rule) => rule.id)
      .join(",")}`;

    if (chart && chartSignature === nextSignature) {
      chart.setData(chartData());
      return;
    }

    chart?.destroy();
    chart = null;
    chartSignature = nextSignature;

    if (selectedRules().length === 0) {
      return;
    }

    const uPlot = uplotModule.default;
    const hostWidth = Math.max(chartHost.clientWidth, 320);
    const series = [
      {},
      ...selectedRules().map((rule, index) => ({
        label: rule.name,
        stroke: ruleColorMap().get(rule.id) ?? SERIES_COLORS[index % SERIES_COLORS.length],
        width: 2,
        spanGaps: false,
        points: { show: false },
        value: (_plot: unknown, value: number | null) => formatMetricValue(metric(), value)
      }))
    ];

    chart = new uPlot(
      {
        width: hostWidth,
        height: 300,
        padding: [12, 12, 8, 8],
        legend: { show: false },
        cursor: { drag: { x: false, y: false } },
        scales: {
          x: { time: true },
          y: {
            auto: true,
            range: (_u: unknown, min: number, max: number) => {
              if (max <= 0) {
                return [0, 1];
              }
              const upper = Math.max(1, Math.ceil(max * 1.1));
              return [0, upper];
            }
          }
        },
        series,
        axes: [
          {
            values: (_plot: unknown, values: number[]) => values.map((value) => formatXAxisTick(value)),
            stroke: "rgba(148, 163, 184, 0.45)",
            grid: { stroke: "rgba(148, 163, 184, 0.12)" }
          },
          {
            stroke: "rgba(148, 163, 184, 0.45)",
            grid: { stroke: "rgba(148, 163, 184, 0.12)" },
            values: (_plot: unknown, values: number[]) =>
              values.map((value) => formatMetricValue(metric(), Math.max(0, value)))
          }
        ]
      },
      chartData(),
      chartHost
    );
  }

  onMount(() => {
    const clock = window.setInterval(() => {
      setNowEpochSeconds(Math.floor(Date.now() / 1000));
    }, 1000);

    resizeObserver = new ResizeObserver(() => {
      if (!chart || !chartHost) return;
      chart.setSize({ width: Math.max(chartHost.clientWidth, 320), height: 300 });
    });

    onCleanup(() => {
      window.clearInterval(clock);
    });
  });

  createEffect(() => {
    const ruleIds = selectedRuleIds();
    const intervalMs = refreshSeconds() * 1000;
    const version = ++refreshAbortVersion;

    if (ruleIds.length === 0) {
      setWindowRows([]);
      setLoading(false);
      setError(null);
      chart?.destroy();
      chart = null;
      chartSignature = "";
      return;
    }

    setLoading(true);
    void refreshWindowData(ruleIds, version);

    const timer = window.setInterval(() => {
      setNowEpochSeconds(Math.floor(Date.now() / 1000));
      void refreshWindowData(ruleIds, version);
    }, intervalMs);

    onCleanup(() => {
      window.clearInterval(timer);
    });
  });

  createEffect(() => {
    const _ = chartData();
    const __ = selectedRules();
    const ___ = metric();
    void ensureChart();
  });

  onCleanup(() => {
    resizeObserver?.disconnect();
    chart?.destroy();
    chart = null;
  });

  return (
    <div class="dashboard-section">
      <div class="panel-title dashboard-panel-header">
        <h3>{t("dashboard.trafficTitle")}</h3>
        <div class="traffic-header-actions">
          <DropdownPanel
            actionLabel={t("dashboard.trafficConfig")}
            open={configOpen()}
            onOpenChange={setConfigOpen}
            panelClass="traffic-config-panel"
          >
            <div class="traffic-config-section">
              <div class="traffic-config-title">{t("dashboard.trafficMetric")}</div>
              <div class="traffic-config-chip-row">
                <For each={METRIC_OPTIONS}>
                  {(option) => (
                    <button
                      type="button"
                      class={`traffic-config-chip ${metric() === option ? "active" : ""}`}
                      onClick={() => setMetric(option)}
                    >
                      {t(`dashboard.metric${option[0].toUpperCase()}${option.slice(1)}` as const)}
                    </button>
                  )}
                </For>
              </div>
            </div>

            <div class="traffic-config-section">
              <div class="traffic-config-title">{t("dashboard.trafficWindow")}</div>
              <div class="traffic-config-chip-row">
                <For each={WINDOW_OPTIONS}>
                  {(option) => (
                    <button
                      type="button"
                      class={`traffic-config-chip ${windowSeconds() === option ? "active" : ""}`}
                      onClick={() => setWindowSeconds(option)}
                    >
                      {t("dashboard.windowSeconds", { count: option })}
                    </button>
                  )}
                </For>
              </div>
            </div>

            <div class="traffic-config-section">
              <div class="traffic-config-title">{t("dashboard.trafficRefreshRate")}</div>
              <div class="traffic-config-chip-row">
                <For each={REFRESH_OPTIONS}>
                  {(option) => (
                    <button
                      type="button"
                      class={`traffic-config-chip ${refreshSeconds() === option ? "active" : ""}`}
                      onClick={() => setRefreshSeconds(option)}
                    >
                      {t("dashboard.windowSeconds", { count: option })}
                    </button>
                  )}
                </For>
              </div>
            </div>

            <div class="traffic-config-section">
              <div class="traffic-config-title">{t("dashboard.trafficRules")}</div>
              <Show
                when={availableRules().length > 0}
                fallback={<div class="traffic-empty-inline compact">{t("dashboard.noRulesForTraffic")}</div>}
              >
                <div class="traffic-config-rule-list">
                  <For each={availableRules()}>
                    {(rule, index) => (
                      <KCheckbox.Root
                        checked={selectedRuleIds().includes(rule.id)}
                        onChange={(checked) => setRuleSelected(rule.id, checked)}
                        class={`kb-checkbox traffic-config-rule ${selectedRuleIds().includes(rule.id) ? "selected" : ""}`}
                      >
                        <KCheckbox.Input />
                        <KCheckbox.Control class="kb-checkbox-control">
                          <KCheckbox.Indicator class="kb-checkbox-indicator" />
                        </KCheckbox.Control>
                        <span
                          class="traffic-legend-dot"
                          style={{ background: ruleColorMap().get(rule.id) ?? SERIES_COLORS[index() % SERIES_COLORS.length] }}
                        />
                        <KCheckbox.Label class="kb-checkbox-label traffic-config-rule-label">
                          {rule.name}
                        </KCheckbox.Label>
                      </KCheckbox.Root>
                    )}
                  </For>
                </div>
              </Show>
            </div>
          </DropdownPanel>
        </div>
      </div>

      <div class="traffic-chart-shell">
        <div
          ref={(element: HTMLDivElement) => {
            chartHost = element;
            resizeObserver?.observe(element);
            void ensureChart();
          }}
          class={`traffic-chart-host ${selectedRules().length === 0 ? "is-empty" : ""}`}
        />
        <Show when={!error() && selectedRules().length === 0}>
          <div class="traffic-chart-empty-overlay">
            <span>{t("dashboard.trafficNoVisibleSeries")}</span>
          </div>
        </Show>
        <Show when={!!error()}>
          <div class="traffic-chart-empty-overlay error">
            <span>{error()}</span>
          </div>
        </Show>
        <Show when={loading()}>
          <div class="traffic-chart-loading-overlay">{t("common.loading")}</div>
        </Show>
      </div>

      <div class="traffic-summary-strip">
        <div class="traffic-summary-card">
          <span class="traffic-summary-label">{t("dashboard.metricIn")}</span>
          <strong>{formatMetricValue("in", totalSummary().bytesIn)}</strong>
        </div>
        <div class="traffic-summary-card">
          <span class="traffic-summary-label">{t("dashboard.metricOut")}</span>
          <strong>{formatMetricValue("out", totalSummary().bytesOut)}</strong>
        </div>
        <div class="traffic-summary-card">
          <span class="traffic-summary-label">{t("dashboard.metricConnections")}</span>
          <strong>{formatMetricValue("connections", totalSummary().connections)}</strong>
        </div>
      </div>
    </div>
  );
}
