import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import * as KCheckbox from "@kobalte/core/checkbox";
import * as KButton from "@kobalte/core/button";
import * as KTextField from "@kobalte/core/text-field";

import { getTrafficWindowData } from "../rules/api";
import type {
  TrafficMonitorEntity,
  TrafficSample,
  TrafficWindowData,
  TrafficWindowQueryEntity
} from "../../lib/types";
import { useI18n } from "../../i18n/context";
import { DropdownPanel } from "../../lib/DropdownPanel";
import { SearchIcon } from "../../lib/ui";

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

function entityKey(entityType: string, entityId: string) {
  return `${entityType}:${entityId}`;
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

function normalizeSelectedEntityKeys(current: string[], valid: string[]) {
  const validSet = new Set(valid);
  return current.filter((value) => validSet.has(value));
}

function buildEntityColorMap(entityKeys: string[]) {
  return new Map(entityKeys.map((key, index) => [key, SERIES_COLORS[index % SERIES_COLORS.length]]));
}

type TrafficChartProps = {
  entities: TrafficMonitorEntity[];
};

export function TrafficChart(props: TrafficChartProps) {
  const { t } = useI18n();

  const [metric, setMetric] = createSignal<TrafficMetric>("total");
  const [windowSeconds, setWindowSeconds] = createSignal<(typeof WINDOW_OPTIONS)[number]>(60);
  const [refreshSeconds, setRefreshSeconds] = createSignal<(typeof REFRESH_OPTIONS)[number]>(1);
  const [selectedEntityKeys, setSelectedEntityKeys] = createSignal<string[]>([]);
  const [windowRows, setWindowRows] = createSignal<TrafficWindowData[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [nowEpochSeconds, setNowEpochSeconds] = createSignal(Math.floor(Date.now() / 1000));
  const [configOpen, setConfigOpen] = createSignal(false);
  const [selectionInitialized, setSelectionInitialized] = createSignal(false);
  const [searchKeyword, setSearchKeyword] = createSignal("");

  let chartHost: HTMLDivElement | undefined;
  let chart: UPlotLike | null = null;
  let uplotModule: any = null;
  let resizeObserver: ResizeObserver | null = null;
  let chartSignature = "";
  let refreshAbortVersion = 0;

  const availableEntities = createMemo(() =>
    [...props.entities].sort(
      (a, b) =>
        Number(b.enabled) - Number(a.enabled) ||
        a.label.localeCompare(b.label) ||
        entityKey(a.entity_type, a.entity_id).localeCompare(entityKey(b.entity_type, b.entity_id))
    )
  );

  const selectedEntities = createMemo(() => {
    const selected = new Set(selectedEntityKeys());
    return availableEntities().filter((entity) => selected.has(entityKey(entity.entity_type, entity.entity_id)));
  });

  const entityColorMap = createMemo(() =>
    buildEntityColorMap(
      availableEntities().map((entity) => entityKey(entity.entity_type, entity.entity_id))
    )
  );

  const ruleEntities = createMemo(() =>
    availableEntities().filter((entity) => entity.entity_type === "legacy_rule")
  );

  const proxyEntities = createMemo(() =>
    availableEntities().filter((entity) => entity.entity_type === "proxy_upstream")
  );

  const selectedSummary = createMemo(() => ({
    selected: selectedEntities().length,
    total: availableEntities().length
  }));

  const normalizedSearchKeyword = createMemo(() => searchKeyword().trim().toLocaleLowerCase());

  const filteredRuleEntities = createMemo(() => {
    const keyword = normalizedSearchKeyword();
    if (!keyword) return ruleEntities();
    return ruleEntities().filter((entity) => entity.label.toLocaleLowerCase().includes(keyword));
  });

  const filteredProxyEntities = createMemo(() => {
    const keyword = normalizedSearchKeyword();
    if (!keyword) return proxyEntities();
    return proxyEntities().filter((entity) => entity.label.toLocaleLowerCase().includes(keyword));
  });

  const filteredEntityCount = createMemo(
    () => filteredRuleEntities().length + filteredProxyEntities().length
  );

  const windowRowMap = createMemo(
    () => new Map(windowRows().map((item) => [entityKey(item.entity_type, item.entity_id), item]))
  );

  const chartData = createMemo<(number | null)[][]>(() => {
    const end = nowEpochSeconds();
    const start = end - windowSeconds() + 1;
    const xAxis = Array.from({ length: windowSeconds() }, (_, index) => start + index);
    const series = selectedEntities().map((entity) => {
      const samples = new Map(
        (
          windowRowMap().get(entityKey(entity.entity_type, entity.entity_id))?.samples ?? []
        ).map((sample) => [sample.timestamp, sample])
      );
      return Array.from({ length: windowSeconds() }, (_, index) => {
        const timestamp = start + index;
        return metricValue(metric(), samples.get(timestamp));
      });
    });

    return [xAxis, ...series];
  });

  const totals = createMemo(() => {
    const totalsByEntity = new Map(
      windowRows().map((item) => [
        entityKey(item.entity_type, item.entity_id),
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

    return selectedEntities().map((entity, index) => ({
      id: entityKey(entity.entity_type, entity.entity_id),
      name: entity.label,
      color:
        entityColorMap().get(entityKey(entity.entity_type, entity.entity_id)) ??
        SERIES_COLORS[index % SERIES_COLORS.length],
      bytesIn:
        totalsByEntity.get(entityKey(entity.entity_type, entity.entity_id))?.bytesIn ?? 0,
      bytesOut:
        totalsByEntity.get(entityKey(entity.entity_type, entity.entity_id))?.bytesOut ?? 0,
      connections:
        totalsByEntity.get(entityKey(entity.entity_type, entity.entity_id))?.connections ?? 0
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

  const totalsByEntity = createMemo(
    () => new Map(totals().map((item) => [item.id, item]))
  );

  const selectedEntityPreview = createMemo(() => selectedEntities().slice(0, 8));
  const hiddenSelectedCount = createMemo(() => Math.max(0, selectedEntities().length - selectedEntityPreview().length));

  createEffect(() => {
    const validEntityKeys = availableEntities().map((entity) =>
      entityKey(entity.entity_type, entity.entity_id)
    );
    const currentSelected = selectedEntityKeys();
    const nextSelected = normalizeSelectedEntityKeys(currentSelected, validEntityKeys);

    if (!selectionInitialized() && validEntityKeys.length > 0 && currentSelected.length === 0) {
      const initialSelection = availableEntities()
        .filter((entity) => entity.enabled)
        .map((entity) => entityKey(entity.entity_type, entity.entity_id));
      const resolvedInitial =
        initialSelection.length > 0 ? initialSelection : validEntityKeys;
      setSelectedEntityKeys(resolvedInitial);
      setSelectionInitialized(true);
      return;
    }

    if (!sameIds(nextSelected, currentSelected)) {
      setSelectedEntityKeys(nextSelected);
    }

    if (!selectionInitialized() && validEntityKeys.length > 0) {
      setSelectionInitialized(true);
    }
  });

  async function refreshWindowData(entities: TrafficWindowQueryEntity[], version: number) {
    if (entities.length === 0) {
      setWindowRows([]);
      setLoading(false);
      setError(null);
      setNowEpochSeconds(Math.floor(Date.now() / 1000));
      return;
    }

    try {
      setError(null);
      const rows = await getTrafficWindowData(entities);
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

  function setEntitySelected(entity: TrafficMonitorEntity, checked: boolean) {
    const key = entityKey(entity.entity_type, entity.entity_id);
    setSelectedEntityKeys((prev) => {
      const exists = prev.includes(key);
      if (checked) {
        return exists ? prev : [...prev, key];
      }
      return exists ? prev.filter((item) => item !== key) : prev;
    });
  }

  function setSelectionMode(mode: "all" | "enabled" | "none") {
    if (mode === "none") {
      setSelectedEntityKeys([]);
      return;
    }
    const next = availableEntities()
      .filter((entity) => (mode === "all" ? true : entity.enabled))
      .map((entity) => entityKey(entity.entity_type, entity.entity_id));
    setSelectedEntityKeys(next);
  }

  function removeSelectedEntity(entity: TrafficMonitorEntity) {
    setEntitySelected(entity, false);
  }

  async function ensureChart() {
    if (!chartHost) return;
    if (!uplotModule) {
      uplotModule = await import("uplot");
    }

    const nextSignature = `${metric()}::${selectedEntities()
      .map((entity) => entityKey(entity.entity_type, entity.entity_id))
      .join(",")}`;

    if (chart && chartSignature === nextSignature) {
      chart.setData(chartData());
      return;
    }

    chart?.destroy();
    chart = null;
    chartSignature = nextSignature;

    if (selectedEntities().length === 0) {
      return;
    }

    const uPlot = uplotModule.default;
    const hostWidth = Math.max(chartHost.clientWidth, 320);
    const series = [
      {},
      ...selectedEntities().map((entity, index) => ({
        label: entity.label,
        stroke:
          entityColorMap().get(entityKey(entity.entity_type, entity.entity_id)) ??
          SERIES_COLORS[index % SERIES_COLORS.length],
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
    const entities = selectedEntities().map((entity) => ({
      entity_type: entity.entity_type,
      entity_id: entity.entity_id
    }));
    const intervalMs = refreshSeconds() * 1000;
    const version = ++refreshAbortVersion;

    if (entities.length === 0) {
      setWindowRows([]);
      setLoading(false);
      setError(null);
      chart?.destroy();
      chart = null;
      chartSignature = "";
      return;
    }

    setLoading(true);
    void refreshWindowData(entities, version);

    const timer = window.setInterval(() => {
      setNowEpochSeconds(Math.floor(Date.now() / 1000));
      void refreshWindowData(entities, version);
    }, intervalMs);

    onCleanup(() => {
      window.clearInterval(timer);
    });
  });

  createEffect(() => {
    const _ = chartData();
    const __ = selectedEntities();
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
              <div class="traffic-config-title">{t("dashboard.trafficEntities")}</div>
              <div class="traffic-config-toolbar">
                <div class="traffic-config-toolbar-copy">
                  <div class="traffic-config-selection-summary">
                    {t("dashboard.trafficSelectedSummary", selectedSummary())}
                  </div>
                  <Show when={normalizedSearchKeyword()}>
                    <div class="traffic-config-filter-summary">
                      {t("dashboard.trafficFilteredSummary", {
                        visible: filteredEntityCount(),
                        total: availableEntities().length
                      })}
                    </div>
                  </Show>
                </div>
                <div class="traffic-config-quick-actions">
                  <KButton.Root class="kb-btn ghost small" onClick={() => setSelectionMode("enabled")}>
                    {t("dashboard.trafficSelectEnabled")}
                  </KButton.Root>
                  <KButton.Root class="kb-btn ghost small" onClick={() => setSelectionMode("all")}>
                    {t("dashboard.trafficSelectAll")}
                  </KButton.Root>
                  <KButton.Root class="kb-btn ghost small" onClick={() => setSelectionMode("none")}>
                    {t("dashboard.trafficClearSelection")}
                  </KButton.Root>
                </div>
              </div>
              <KTextField.Root class="kb-field traffic-config-search" value={searchKeyword()} onChange={setSearchKeyword}>
                <div class="traffic-config-search-shell">
                  <SearchIcon size={16} class="traffic-config-search-icon" />
                  <KTextField.Input
                    class="kb-input traffic-config-search-input"
                    placeholder={t("dashboard.trafficSearchPlaceholder")}
                  />
                  <Show when={searchKeyword().trim().length > 0}>
                    <KButton.Root class="kb-btn ghost small" onClick={() => setSearchKeyword("")}>
                      {t("dashboard.trafficClearSearch")}
                    </KButton.Root>
                  </Show>
                </div>
              </KTextField.Root>
              <Show
                when={availableEntities().length > 0}
                fallback={<div class="traffic-empty-inline compact">{t("dashboard.noTrafficEntities")}</div>}
              >
                <Show
                  when={filteredEntityCount() > 0}
                  fallback={<div class="traffic-empty-inline compact">{t("dashboard.trafficSearchEmpty")}</div>}
                >
                <Show when={filteredRuleEntities().length > 0}>
                  <div class="traffic-config-group-heading">
                    <span>{t("dashboard.trafficRules")}</span>
                    <span>{filteredRuleEntities().length}</span>
                  </div>
                  <div class="traffic-config-rule-list">
                    <For each={filteredRuleEntities()}>
                      {(entity, index) => (
                        <KCheckbox.Root
                          checked={selectedEntityKeys().includes(entityKey(entity.entity_type, entity.entity_id))}
                          onChange={(checked) => setEntitySelected(entity, checked)}
                          class={`kb-checkbox traffic-config-rule ${selectedEntityKeys().includes(entityKey(entity.entity_type, entity.entity_id)) ? "selected" : ""} ${entity.enabled ? "" : "is-disabled"}`}
                        >
                          <KCheckbox.Input />
                          <KCheckbox.Control class="kb-checkbox-control">
                            <KCheckbox.Indicator class="kb-checkbox-indicator" />
                          </KCheckbox.Control>
                          <span
                            class="traffic-legend-dot"
                            style={{
                              background:
                                entityColorMap().get(entityKey(entity.entity_type, entity.entity_id)) ??
                                SERIES_COLORS[index() % SERIES_COLORS.length]
                            }}
                          />
                          <KCheckbox.Label class="kb-checkbox-label traffic-config-rule-label" title={entity.label}>
                            <span class="traffic-config-rule-name">{entity.label}</span>
                            <Show when={!entity.enabled}>
                              <span class="traffic-config-rule-meta">{t("common.disabled")}</span>
                            </Show>
                          </KCheckbox.Label>
                        </KCheckbox.Root>
                      )}
                    </For>
                  </div>
                </Show>
                <Show when={filteredProxyEntities().length > 0}>
                  <div class="traffic-config-group-heading">
                    <span>{t("dashboard.trafficProxy")}</span>
                    <span>{filteredProxyEntities().length}</span>
                  </div>
                  <div class="traffic-config-rule-list">
                    <For each={filteredProxyEntities()}>
                      {(entity, index) => (
                        <KCheckbox.Root
                          checked={selectedEntityKeys().includes(entityKey(entity.entity_type, entity.entity_id))}
                          onChange={(checked) => setEntitySelected(entity, checked)}
                          class={`kb-checkbox traffic-config-rule ${selectedEntityKeys().includes(entityKey(entity.entity_type, entity.entity_id)) ? "selected" : ""} ${entity.enabled ? "" : "is-disabled"}`}
                        >
                          <KCheckbox.Input />
                          <KCheckbox.Control class="kb-checkbox-control">
                            <KCheckbox.Indicator class="kb-checkbox-indicator" />
                          </KCheckbox.Control>
                          <span
                            class="traffic-legend-dot"
                            style={{
                              background:
                                entityColorMap().get(entityKey(entity.entity_type, entity.entity_id)) ??
                                SERIES_COLORS[(ruleEntities().length + index()) % SERIES_COLORS.length]
                            }}
                          />
                          <KCheckbox.Label class="kb-checkbox-label traffic-config-rule-label" title={entity.label}>
                            <span class="traffic-config-rule-name">{entity.label}</span>
                            <Show when={!entity.enabled}>
                              <span class="traffic-config-rule-meta">{t("common.disabled")}</span>
                            </Show>
                          </KCheckbox.Label>
                        </KCheckbox.Root>
                      )}
                    </For>
                  </div>
                </Show>
                </Show>
              </Show>
            </div>
          </DropdownPanel>
        </div>
      </div>

      <div class="traffic-chart-shell">
        <div class="traffic-chart-context-strip">
          <div class="traffic-chart-context-item">
            <span class="traffic-chart-context-label">{t("dashboard.trafficMetric")}</span>
            <strong>{t(`dashboard.metric${metric()[0].toUpperCase()}${metric().slice(1)}` as const)}</strong>
          </div>
          <div class="traffic-chart-context-item">
            <span class="traffic-chart-context-label">{t("dashboard.trafficWindow")}</span>
            <strong>{t("dashboard.windowSeconds", { count: windowSeconds() })}</strong>
          </div>
          <div class="traffic-chart-context-item">
            <span class="traffic-chart-context-label">{t("dashboard.trafficRefreshRate")}</span>
            <strong>{t("dashboard.windowSeconds", { count: refreshSeconds() })}</strong>
          </div>
          <div class="traffic-chart-context-item accent">
            <span class="traffic-chart-context-label">{t("dashboard.trafficEntities")}</span>
            <strong>{t("dashboard.trafficSelectedSummary", selectedSummary())}</strong>
          </div>
        </div>
        <div
          ref={(element: HTMLDivElement) => {
            chartHost = element;
            resizeObserver?.observe(element);
            void ensureChart();
          }}
          class={`traffic-chart-host ${selectedEntities().length === 0 ? "is-empty" : ""}`}
        />
        <Show when={!error() && selectedEntities().length === 0}>
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

      <Show when={selectedEntities().length > 0}>
        <div class="traffic-selected-strip">
          <For each={selectedEntityPreview()}>
            {(entity) => (
              <button
                type="button"
                class="traffic-selected-chip"
                title={t("dashboard.trafficLegendValue", {
                  in: formatMetricValue("in", totalsByEntity().get(entityKey(entity.entity_type, entity.entity_id))?.bytesIn ?? 0),
                  out: formatMetricValue("out", totalsByEntity().get(entityKey(entity.entity_type, entity.entity_id))?.bytesOut ?? 0),
                  connections: formatMetricValue("connections", totalsByEntity().get(entityKey(entity.entity_type, entity.entity_id))?.connections ?? 0)
                })}
                onClick={() => removeSelectedEntity(entity)}
              >
                <span
                  class="traffic-legend-dot"
                  style={{
                    background:
                      entityColorMap().get(entityKey(entity.entity_type, entity.entity_id)) ?? SERIES_COLORS[0]
                  }}
                />
                <span class="traffic-selected-chip-label">{entity.label}</span>
              </button>
            )}
          </For>
          <Show when={hiddenSelectedCount() > 0}>
            <div class="traffic-selected-chip muted">
              +{hiddenSelectedCount()} {t("dashboard.trafficMoreSelected")}
            </div>
          </Show>
        </div>
      </Show>

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
