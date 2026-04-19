import { createMemo, Show } from "solid-js";
import { queryOptions, useQuery } from "@tanstack/solid-query";
import * as KButton from "@kobalte/core/button";

import "./DashboardPage.css";

import { getRuntimeStatus, listRules, scanTopology } from "../rules/api";
import { appQueryClient } from "../../lib/queryClient";
import type { ProxyRule, RuntimeStatusItem, RuntimeState, TopologySnapshot } from "../../lib/types";
import { useI18n } from "../../i18n/context";
import { toLocalTime } from "../../lib/datetime";
import { SkeletonGrid } from "../../lib/Skeleton";
import { Hint } from "../../lib/Hint";
import { useToast } from "../../lib/Toast";
import { TrafficChart } from "./TrafficChart";

export function DashboardPage() {
  const { t } = useI18n();
  const toast = useToast();

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
    () => rulesQuery.isPending || runtimeQuery.isPending || topologyQuery.isPending
  );

  async function refreshDashboard() {
    try {
      await Promise.all([rulesQuery.refetch(), runtimeQuery.refetch(), topologyQuery.refetch()]);
      toast.info(t("dashboard.refreshed"));
    } catch (error) {
      toast.error(String(error));
    }
  }

  async function rescanTopology() {
    try {
      await topologyQuery.refetch();
      toast.info(t("dashboard.topologyScanned"));
    } catch (error) {
      toast.error(String(error));
    }
  }

  return (
    <div class="page">
      <section class="page-shell">
        <div class="panel-title">
          <h2>{t("dashboard.title")}</h2>
        </div>
        <div class="dashboard-actions">
          <KButton.Root class="kb-btn primary" onClick={refreshDashboard}>
            {t("dashboard.refreshOverview")}
          </KButton.Root>
          <KButton.Root class="kb-btn ghost" onClick={rescanTopology}>
            {t("dashboard.rescanTopology")}
          </KButton.Root>
        </div>
        <Show when={!isLoading()} fallback={<SkeletonGrid dashboard />}>
          <div class="dashboard-grid">
            <div class="dashboard-card">
              <div class="dashboard-card-header">{t("dashboard.appStatus")}</div>
              <div class={`status-chip ${appStatus()}`}>
                {t(`common.${appStatus()}`)}
              </div>
              <div class="caption-text">
                {t("dashboard.lastTopologyScan", { value: toLocalTime(topologyQuery.data?.timestamp ?? null) })}
              </div>
            </div>
            <div class="dashboard-card">
              <div class="dashboard-card-header">{t("dashboard.ruleStatus")}</div>
              <div class="dashboard-stat-large">
                {t("dashboard.totalRules", { count: rulesQuery.data?.length ?? 0 })}
              </div>
              <div class="dashboard-stat">{t("dashboard.enabledRules", { count: enabledRules() })}</div>
              <div class="dashboard-stat">{t("dashboard.runningRules", { count: runtimeSummary().running })}</div>
              <Show when={runtimeSummary().error > 0}>
                <div class="dashboard-stat" style="color: var(--danger-text)">
                  {t("dashboard.errorRules", { count: runtimeSummary().error })}
                </div>
              </Show>
            </div>
            <div class="dashboard-card">
              <div class="dashboard-card-header">{t("dashboard.riskHint")}</div>
              <Show when={natWithoutRules()} fallback={<Hint variant="info">{t("dashboard.noHighRisk")}</Hint>}>
                <Hint variant="error">{t("dashboard.natRisk")}</Hint>
              </Show>
            </div>
          </div>
        </Show>
      </section>

      <TrafficChart rules={rulesQuery.data ?? []} />
    </div>
  );
}
