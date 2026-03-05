import { createMemo, createSignal, For, Show } from "solid-js";
import { Link } from "@tanstack/solid-router";
import { queryOptions, useQuery } from "@tanstack/solid-query";
import * as KButton from "@kobalte/core/button";
import * as KTooltip from "@kobalte/core/tooltip";

import { getRuntimeStatus, listRules, queryLogs, scanTopology } from "../rules/api";
import { appQueryClient } from "../../lib/queryClient";
import type { AuditLog, ProxyRule, RuntimeStatusItem, RuntimeState, TopologySnapshot } from "../../lib/types";
import { useI18n } from "../../i18n/context";

function toLocalTime(value: string | null) {
  if (!value) return "-";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

function renderEllipsisCell(text: string | null | undefined) {
  const content = (text ?? "").trim() || "-";
  return (
    <KTooltip.Root openDelay={180}>
      <KTooltip.Trigger as="div" class="table-cell-ellipsis">
        {content}
      </KTooltip.Trigger>
      <KTooltip.Portal>
        <KTooltip.Content class="kb-tooltip-content">
          {content}
          <KTooltip.Arrow class="kb-tooltip-arrow" />
        </KTooltip.Content>
      </KTooltip.Portal>
    </KTooltip.Root>
  );
}

export function DashboardPage() {
  const { t } = useI18n();
  const [message, setMessage] = createSignal<{ type: "info" | "error"; text: string } | null>(null);

  const rulesQuery = useQuery(
    () =>
      queryOptions<ProxyRule[]>({
        queryKey: ["dashboard", "rules"],
        queryFn: listRules,
        staleTime: 10_000,
        refetchOnWindowFocus: false
      }),
    () => appQueryClient
  );

  const runtimeQuery = useQuery(
    () =>
      queryOptions<RuntimeStatusItem[]>({
        queryKey: ["dashboard", "runtime"],
        queryFn: getRuntimeStatus,
        staleTime: 5000,
        refetchInterval: 5000,
        refetchOnWindowFocus: false
      }),
    () => appQueryClient
  );

  const topologyQuery = useQuery(
    () =>
      queryOptions<TopologySnapshot>({
        queryKey: ["dashboard", "topology"],
        queryFn: scanTopology,
        staleTime: 60_000,
        refetchOnWindowFocus: false
      }),
    () => appQueryClient
  );

  const errorLogsQuery = useQuery(
    () =>
      queryOptions<{ total: number; events: AuditLog[] }>({
        queryKey: ["dashboard", "error-logs"],
        queryFn: () =>
          queryLogs({
            level: "error",
            start_time: new Date(Date.now() - 24 * 3600_000).toISOString(),
            newest_first: true,
            limit: 8
          }),
        staleTime: 8_000,
        refetchInterval: 8_000,
        refetchOnWindowFocus: false
      }),
    () => appQueryClient
  );

  const runtimeSummary = createMemo(() => {
    const items = runtimeQuery.data ?? [];
    return {
      running: items.filter((item) => item.state === "running").length,
      error: items.filter((item) => item.state === "error").length,
      stopped: items.filter((item) => item.state === "stopped").length
    };
  });

  const enabledRules = createMemo(() => (rulesQuery.data ?? []).filter((item) => item.enabled).length);
  const natWithoutRules = createMemo(() => {
    const hasNat = (topologyQuery.data?.wsl ?? []).some((item) => item.networking_mode.toLowerCase() === "nat");
    return hasNat && enabledRules() === 0;
  });

  const appStatus = createMemo<RuntimeState | "ready">(() => {
    if ((runtimeQuery.data ?? []).length === 0) return "ready";
    if (runtimeSummary().error > 0) return "error";
    if (runtimeSummary().running > 0) return "running";
    return "stopped";
  });

  const isLoading = createMemo(
    () => rulesQuery.isPending || runtimeQuery.isPending || topologyQuery.isPending || errorLogsQuery.isPending
  );

  async function refreshDashboard() {
    try {
      await Promise.all([
        rulesQuery.refetch(),
        runtimeQuery.refetch(),
        topologyQuery.refetch(),
        errorLogsQuery.refetch()
      ]);
      setMessage({ type: "info", text: t("dashboard.refreshed") });
    } catch (error) {
      setMessage({ type: "error", text: String(error) });
    }
  }

  async function rescanTopology() {
    try {
      await topologyQuery.refetch();
      setMessage({ type: "info", text: t("dashboard.topologyScanned") });
    } catch (error) {
      setMessage({ type: "error", text: String(error) });
    }
  }

  return (
    <div class="page">
      <section class="page-shell">
        <div class="panel-title">
          <h2>{t("dashboard.title")}</h2>
          <KButton.Root class="kb-btn ghost" onClick={refreshDashboard}>
            {t("dashboard.refreshOverview")}
          </KButton.Root>
        </div>
        <Show when={!isLoading()} fallback={<div class="skeleton-grid dashboard-skeleton-grid" />}>
          <div class="dashboard-grid">
            <div class="dashboard-card">
              <div class="muted">{t("dashboard.appStatus")}</div>
              <div class={`status-chip ${appStatus()}`}>{t(`common.${appStatus()}`)}</div>
              <div class="muted">
                {t("dashboard.lastTopologyScan", { value: toLocalTime(topologyQuery.data?.timestamp ?? null) })}
              </div>
            </div>
            <div class="dashboard-card">
              <div class="muted">{t("dashboard.ruleStatus")}</div>
              <div class="dashboard-stat">{t("dashboard.totalRules", { count: rulesQuery.data?.length ?? 0 })}</div>
              <div class="dashboard-stat">{t("dashboard.enabledRules", { count: enabledRules() })}</div>
              <div class="dashboard-stat">{t("dashboard.runningRules", { count: runtimeSummary().running })}</div>
              <div class="dashboard-stat">{t("dashboard.errorRules", { count: runtimeSummary().error })}</div>
            </div>
            <div class="dashboard-card">
              <div class="muted">{t("dashboard.riskHint")}</div>
              <Show when={natWithoutRules()} fallback={<div class="hint info">{t("dashboard.noHighRisk")}</div>}>
                <div class="hint error">{t("dashboard.natRisk")}</div>
              </Show>
            </div>
          </div>
        </Show>
        <div class="actions">
          <Link to="/rules" class="kb-btn accent link-btn">
            {t("dashboard.createRule")}
          </Link>
          <KButton.Root class="kb-btn ghost" onClick={rescanTopology}>
            {t("dashboard.scanTopology")}
          </KButton.Root>
          <Link to="/logs" class="kb-btn ghost link-btn">
            {t("dashboard.viewErrorLogs")}
          </Link>
        </div>
        <Show when={message()}>
          {(msg) => <div class={`hint ${msg().type === "error" ? "error" : "info"}`}>{msg().text}</div>}
        </Show>
      </section>

      <section class="page-shell">
        <h2>{t("dashboard.recentErrorLogs")}</h2>
        <div class="table-wrap">
          <table class="rules-table">
            <thead>
              <tr>
                <th>{t("dashboard.tableTime")}</th>
                <th>{t("dashboard.tableModule")}</th>
                <th>{t("dashboard.tableEvent")}</th>
                <th>{t("dashboard.tableDetail")}</th>
              </tr>
            </thead>
            <tbody>
              <Show
                when={!errorLogsQuery.isPending}
                fallback={
                  <For each={[1, 2, 3, 4]}>
                    {() => (
                      <tr>
                        <td colspan={4}>
                          <div class="skeleton-line" />
                        </td>
                      </tr>
                    )}
                  </For>
                }
              >
                <Show
                  when={(errorLogsQuery.data?.events.length ?? 0) > 0}
                  fallback={
                    <tr>
                      <td colspan={4} class="muted">
                        {t("dashboard.noErrorLogs")}
                      </td>
                    </tr>
                  }
                >
                  <For each={errorLogsQuery.data?.events ?? []}>
                    {(item) => (
                      <tr>
                        <td>{renderEllipsisCell(toLocalTime(item.time))}</td>
                        <td>{renderEllipsisCell(item.module)}</td>
                        <td>{renderEllipsisCell(item.event)}</td>
                        <td>{renderEllipsisCell(item.detail)}</td>
                      </tr>
                    )}
                  </For>
                </Show>
              </Show>
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}
