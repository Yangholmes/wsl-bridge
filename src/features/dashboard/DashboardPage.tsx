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
import { MetricCard, PageHeader, SectionCard, StatusBadge } from "../../lib/ui";

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
  const totalRules = createMemo(() => rulesQuery.data?.length ?? 0);
  const topologySummary = createMemo(() => ({
    wsl: topologyQuery.data?.wsl.length ?? 0,
    hyperv: topologyQuery.data?.hyperv.length ?? 0,
    adapters: topologyQuery.data?.adapters.length ?? 0
  }));
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
      <PageHeader
        title={t("dashboard.title")}
        actions={
          <>
            <KButton.Root class="kb-btn ghost" onClick={refreshDashboard}>
              {t("dashboard.refreshOverview")}
            </KButton.Root>
            <KButton.Root class="kb-btn accent" onClick={rescanTopology}>
              {t("dashboard.rescanTopology")}
            </KButton.Root>
          </>
        }
      />

      <Show when={!isLoading()} fallback={<SkeletonGrid dashboard />}>
        <div class="metric-grid">
          <MetricCard
            label={t("dashboard.appStatus")}
            value={<StatusBadge state={appStatus()} label={t(`common.${appStatus()}`)} />}
            detail={t("dashboard.lastTopologyScan", { value: toLocalTime(topologyQuery.data?.timestamp ?? null) })}
          />
          <MetricCard
            label={t("dashboard.ruleStatus")}
            value={`${totalRules()}`}
            detail={t("dashboard.enabledRules", { count: enabledRules() })}
          />
          <MetricCard
            label={t("dashboard.riskHint")}
            value={natWithoutRules() ? t("common.error") : t("common.ready")}
            detail={natWithoutRules() ? t("dashboard.natRisk") : t("dashboard.noHighRisk")}
          />
        </div>

<div class="dashboard-secondary-grid">
          <SectionCard
            title={t("dashboard.rulesSnapshotTitle")}
          >
            <div class="dashboard-summary-grid">
              <div class="line-item">
                <div class="line-item-content">
                  <span class="line-item-title">{t("dashboard.runningRules", { count: runtimeSummary().running })}</span>
                  <span class="line-item-subtitle">{t("dashboard.rulesRunningSubtitle")}</span>
                </div>
                <StatusBadge state="running" label={t("common.running")} />
              </div>
              <div class="line-item">
                <div class="line-item-content">
                  <span class="line-item-title">{t("common.stopped")}</span>
                  <span class="line-item-subtitle">{t("dashboard.rulesStoppedSubtitle", { count: runtimeSummary().stopped })}</span>
                </div>
                <StatusBadge state="stopped" label={String(runtimeSummary().stopped)} />
              </div>
              <div class="line-item">
                <div class="line-item-content">
                  <span class="line-item-title">{t("common.error")}</span>
                  <span class="line-item-subtitle">{t("dashboard.rulesErrorSubtitle", { count: runtimeSummary().error })}</span>
                </div>
                <StatusBadge state={runtimeSummary().error > 0 ? "error" : "ready"} label={String(runtimeSummary().error)} />
              </div>
            </div>
          </SectionCard>

          <SectionCard
            title={t("dashboard.topologyOverviewTitle")}
          >
            <div class="dashboard-summary-grid">
              <div class="line-item">
                <div class="line-item-content">
                  <span class="line-item-title">WSL</span>
                  <span class="line-item-subtitle">{t("dashboard.wslDistroRecognized", { count: topologySummary().wsl })}</span>
                </div>
                <strong>{topologySummary().wsl}</strong>
              </div>
              <div class="line-item">
                <div class="line-item-content">
                  <span class="line-item-title">Hyper-V</span>
                  <span class="line-item-subtitle">{t("dashboard.hypervVmAvailable", { count: topologySummary().hyperv })}</span>
                </div>
                <strong>{topologySummary().hyperv}</strong>
              </div>
              <div class="line-item">
                <div class="line-item-content">
                  <span class="line-item-title">Adapters</span>
                  <span class="line-item-subtitle">{t("dashboard.adaptersListed", { count: topologySummary().adapters })}</span>
                </div>
                <strong>{topologySummary().adapters}</strong>
              </div>
            </div>

            <Show when={natWithoutRules()}>
              <Hint variant="error">{t("dashboard.natRisk")}</Hint>
            </Show>
          </SectionCard>
        </div>
      </Show>

      <TrafficChart rules={rulesQuery.data ?? []} />
    </div>
  );
}
