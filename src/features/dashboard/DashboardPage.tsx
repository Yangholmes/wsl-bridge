import { createMemo, Show } from "solid-js";
import { queryOptions, useQuery } from "@tanstack/solid-query";
import * as KButton from "@kobalte/core/button";

import "./DashboardPage.css";

import {
  getRuntimeStatus,
  listRules,
  listTrafficMonitorEntities,
  scanTopology
} from "../rules/api";
import {
  getProxyRuntimeStatus,
  listProxyListeners,
  listProxyRoutes,
  listProxyUpstreams
} from "../proxy/api";
import { appQueryClient } from "../../lib/queryClient";
import type {
  ProxyListener,
  ProxyRule,
  ProxyRoute,
  ProxyRuntimeStatusItem,
  ProxyUpstream,
  RuntimeStatusItem,
  RuntimeState,
  TopologySnapshot,
  TrafficMonitorEntity
} from "../../lib/types";
import { useI18n } from "../../i18n/context";
import { toLocalTime } from "../../lib/datetime";
import { SkeletonGrid } from "../../lib/Skeleton";
import { useToast } from "../../lib/Toast";
import { TrafficChart } from "./TrafficChart";

import { MetricCard, PageHeader, StatusBadge } from "../../lib/ui";

type ProxyDashboardSnapshot = {
  listeners: ProxyListener[];
  routes: ProxyRoute[];
  upstreams: ProxyUpstream[];
  runtime: ProxyRuntimeStatusItem[];
};

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

  const trafficEntitiesQuery = useQuery(
    () =>
      queryOptions<TrafficMonitorEntity[]>({
        queryKey: ["dashboard", "traffic-entities"],
        queryFn: listTrafficMonitorEntities,
        staleTime: 10_000,
        refetchOnWindowFocus: false
      }),
    () => appQueryClient
  );

  const proxyOverviewQuery = useQuery(
    () =>
      queryOptions<ProxyDashboardSnapshot>({
        queryKey: ["dashboard", "proxy-overview"],
        queryFn: async () => {
          const listeners = await listProxyListeners();
          const routesByListener = await Promise.all(listeners.map((item) => listProxyRoutes(item.id)));
          const routes = routesByListener.flat();
          const upstreamsByRoute = await Promise.all(routes.map((item) => listProxyUpstreams(item.id)));
          const upstreams = upstreamsByRoute.flat();
          const runtime = await getProxyRuntimeStatus();
          return {
            listeners,
            routes,
            upstreams,
            runtime
          };
        },
        staleTime: 10_000,
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

  const proxyRuntimeSummary = createMemo(() => {
    const items = proxyOverviewQuery.data?.runtime ?? [];
    return {
      running: items.filter((item) => item.state === "running").length,
      error: items.filter((item) => item.state === "error").length,
      stopped: items.filter((item) => item.state === "stopped").length
    };
  });

  const enabledRules = createMemo(() => (rulesQuery.data ?? []).filter((item) => item.enabled).length);
  const totalRules = createMemo(() => rulesQuery.data?.length ?? 0);
  const totalProxyListeners = createMemo(() => proxyOverviewQuery.data?.listeners.length ?? 0);
  const enabledProxyListeners = createMemo(
    () => (proxyOverviewQuery.data?.listeners ?? []).filter((item) => item.enabled).length
  );
  const totalProxyRoutes = createMemo(() => proxyOverviewQuery.data?.routes.length ?? 0);
  const enabledProxyRoutes = createMemo(
    () => (proxyOverviewQuery.data?.routes ?? []).filter((item) => item.enabled).length
  );
  const totalProxyUpstreams = createMemo(() => proxyOverviewQuery.data?.upstreams.length ?? 0);
  const enabledProxyUpstreams = createMemo(
    () => (proxyOverviewQuery.data?.upstreams ?? []).filter((item) => item.enabled).length
  );
  const natWithoutRules = createMemo(() => {
    const hasNat = (topologyQuery.data?.wsl ?? []).some((item) => item.networking_mode.toLowerCase() === "nat");
    return hasNat && enabledRules() === 0 && enabledProxyListeners() === 0;
  });

  const appStatus = createMemo<RuntimeState | "ready">(() => {
    const totalRuntimeEntities = (runtimeQuery.data?.length ?? 0) + (proxyOverviewQuery.data?.runtime.length ?? 0);
    if (totalRuntimeEntities === 0) return "ready";
    if (runtimeSummary().error > 0 || proxyRuntimeSummary().error > 0) return "error";
    if (runtimeSummary().running > 0 || proxyRuntimeSummary().running > 0) return "running";
    return "stopped";
  });

  const isLoading = createMemo(
    () =>
      rulesQuery.isPending ||
      runtimeQuery.isPending ||
      topologyQuery.isPending ||
      trafficEntitiesQuery.isPending ||
      proxyOverviewQuery.isPending
  );

  async function refreshDashboard() {
    try {
      await Promise.all([
        rulesQuery.refetch(),
        runtimeQuery.refetch(),
        topologyQuery.refetch(),
        trafficEntitiesQuery.refetch(),
        proxyOverviewQuery.refetch()
      ]);
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
        <div class="metric-grid dashboard-metric-grid">
          <MetricCard
            label={t("dashboard.appStatus")}
            value={<StatusBadge state={appStatus()} label={t(`common.${appStatus()}`)} />}
            detail={
              <span class="dashboard-metric-detail-stack">
                <span class="dashboard-metric-detail-primary">
                  {t("dashboard.lastTopologyScan", { value: toLocalTime(topologyQuery.data?.timestamp ?? null) })}
                </span>
                <span class="dashboard-metric-detail-secondary">
                  {t("dashboard.dashboardRuntimeMix", {
                    rulesRunning: runtimeSummary().running,
                    rulesError: runtimeSummary().error,
                    proxyRunning: proxyRuntimeSummary().running,
                    proxyError: proxyRuntimeSummary().error
                  })}
                </span>
              </span>
            }
          />
          <MetricCard
            label={t("dashboard.ruleStatus")}
            value={
              <span class="dashboard-metric-dual-value">
                <span>{t("dashboard.dashboardRulesShort", { count: totalRules() })}</span>
                <span>{t("dashboard.dashboardProxyShort", { count: totalProxyListeners() })}</span>
              </span>
            }
            detail={
              <span class="dashboard-metric-detail-stack">
                <span class="dashboard-metric-detail-primary">
                  {t("dashboard.dashboardConfigEnabledMix", {
                    rules: enabledRules(),
                    listeners: enabledProxyListeners(),
                    routes: enabledProxyRoutes(),
                    upstreams: enabledProxyUpstreams()
                  })}
                </span>
                <span class="dashboard-metric-detail-secondary">
                  {t("dashboard.dashboardConfigTotalMix", {
                    routes: totalProxyRoutes(),
                    upstreams: totalProxyUpstreams()
                  })}
                </span>
              </span>
            }
          />
          <MetricCard
            label={t("dashboard.riskHint")}
            value={natWithoutRules() ? t("common.error") : t("common.ready")}
            detail={
              <span class="dashboard-metric-detail-stack">
                <span class="dashboard-metric-detail-primary">
                  {natWithoutRules() ? t("dashboard.natRisk") : t("dashboard.noHighRisk")}
                </span>
                <span class="dashboard-metric-detail-secondary">
                  {t("dashboard.dashboardExposureMix", {
                    rules: enabledRules(),
                    listeners: enabledProxyListeners(),
                    routes: enabledProxyRoutes(),
                    upstreams: enabledProxyUpstreams()
                  })}
                </span>
              </span>
            }
          />
        </div>
      </Show>

      <TrafficChart entities={trafficEntitiesQuery.data ?? []} />
    </div>
  );
}
