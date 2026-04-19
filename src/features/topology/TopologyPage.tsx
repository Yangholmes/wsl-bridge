import { For, Show } from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import * as KButton from "@kobalte/core/button";

import "./TopologyPage.css";

import { appQueryClient } from "../../lib/queryClient";
import { createTopologyQueryOptions, getGlobalTargetKind, getGlobalTargetRef } from "./state";
import { useI18n } from "../../i18n/context";
import { EllipsisCell } from "../../lib/EllipsisCell";
import { toLocalTime } from "../../lib/datetime";
import { SkeletonLine } from "../../lib/Skeleton";
import { Hint } from "../../lib/Hint";
import { useAppRuntimeStatusQuery } from "../../lib/appRuntime";
import { MetricCard, PageHeader, SectionCard } from "../../lib/ui";

export function TopologyPage() {
  const { t } = useI18n();
  const runtimeStatusQuery = useAppRuntimeStatusQuery();

  const topologyQuery = useQuery(() => createTopologyQueryOptions(true), () => appQueryClient);
  const hasAdminPrivileges = () => runtimeStatusQuery.data?.admin_features_available ?? false;

  const isScanning = () => topologyQuery.isFetching;

  return (
    <div class="page">
      <PageHeader
        title={t("topology.title")}
        actions={
          <KButton.Root class="kb-btn accent" onClick={() => topologyQuery.refetch()} disabled={isScanning()}>
            {isScanning() ? t("common.scanning") : t("common.rescan")}
          </KButton.Root>
        }
      />

      <div class="metric-grid">
        <MetricCard label={t("topology.wslTitle")} value={`${topologyQuery.data?.wsl.length ?? 0}`} detail={t("topology.lastScanned", { value: toLocalTime(topologyQuery.data?.timestamp ?? null) })} />
        <MetricCard label={t("topology.hypervTitle")} value={`${topologyQuery.data?.hyperv.length ?? 0}`} detail={hasAdminPrivileges() ? t("common.adminMode") : t("common.limitedMode")} />
        <MetricCard label={t("topology.adaptersTitle")} value={`${topologyQuery.data?.adapters.length ?? 0}`} detail={t("topology.networkInterfaces")} />
      </div>

      <div class="topology-grid">
        <SectionCard title={t("topology.wslTitle")} subtitle={t("topology.wslSubtitle")}>
            <div class="table-wrap">
              <table class="rules-table">
                <thead>
                  <tr>
                    <th>{t("topology.tableDistro")}</th>
                    <th>{t("topology.tableMode")}</th>
                    <th>{t("topology.tableIp")}</th>
                  </tr>
                </thead>
                <tbody>
                  <Show when={!isScanning()} fallback={<TopologySkeletonRows colspan={3} rows={4} />}>
                    <Show
                      when={(topologyQuery.data?.wsl.length ?? 0) > 0}
                      fallback={
                        <tr>
                          <td colspan={3} class="muted">
                            {t("topology.noWslData")}
                          </td>
                        </tr>
                      }
                    >
                    <For each={topologyQuery.data?.wsl ?? []}>
                      {(item) => (
                        <tr>
                          <td><EllipsisCell text={item.distro} /></td>
                          <td><EllipsisCell text={item.networking_mode} /></td>
                          <td><EllipsisCell text={item.ip ?? "-"} /></td>
                        </tr>
                      )}
                    </For>
                    </Show>
                  </Show>
                </tbody>
              </table>
            </div>
        </SectionCard>

        <SectionCard title={t("topology.hypervTitle")} subtitle={t("topology.hypervSubtitle")}>
            <Show
              when={hasAdminPrivileges()}
              fallback={<Hint variant="info">{t("topology.hiddenWithoutAdmin")}</Hint>}
            >
              <Show when={topologyQuery.data?.hyperv_error}>
                {(err) => {
                  const errorStr = String(err());
                  const translateError = (error: string) => {
                    if (error === "hyperv_not_enabled") return t("topology.hypervNotEnabled");
                    if (error === "hyperv_admin_required") return t("topology.hypervAdminRequired");
                    if (error === "hyperv_powershell_failed") return t("topology.hypervPowershellFailed");
                    if (error.startsWith("hyperv_query_failed:")) {
                      const code = error.split(":")[1];
                      return t("topology.hypervQueryFailed", { code });
                    }
                    if (error.startsWith("hyperv_error:")) {
                      const detail = error.split(":").slice(1).join(":");
                      return t("topology.hypervError", { error: detail });
                    }
                    if (error.startsWith("hyperv_json_parse_error:")) {
                      const detail = error.split(":").slice(1).join(":");
                      return t("topology.hypervJsonParseError", { error: detail });
                    }
                    return t("topology.adminRequired", { error });
                  };
                  return <Hint variant="error">{translateError(errorStr)}</Hint>;
                }}
              </Show>
              <div class="table-wrap">
                <table class="rules-table">
                  <thead>
                    <tr>
                      <th>{t("topology.tableVm")}</th>
                      <th>{t("topology.tableSwitch")}</th>
                      <th>{t("topology.tableIp")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    <Show when={!isScanning()} fallback={<TopologySkeletonRows colspan={3} rows={4} />}>
                      <Show
                        when={(topologyQuery.data?.hyperv.length ?? 0) > 0}
                        fallback={
                          <tr>
                            <td colspan={3} class="muted">
                              {t("topology.noHypervData")}
                            </td>
                          </tr>
                        }
                      >
                        <For each={topologyQuery.data?.hyperv ?? []}>
                          {(item) => (
                            <tr>
                              <td><EllipsisCell text={item.vm_name} /></td>
                              <td><EllipsisCell text={item.v_switch ?? "-"} /></td>
                              <td><EllipsisCell text={item.ip ?? "-"} /></td>
                            </tr>
                          )}
                        </For>
                      </Show>
                    </Show>
                  </tbody>
                </table>
              </div>
            </Show>
        </SectionCard>

        <SectionCard title={t("topology.adaptersTitle")} subtitle={t("topology.adaptersSubtitle")}>
            <div class="table-wrap">
              <table class="rules-table">
                <thead>
                  <tr>
                    <th>{t("topology.tableName")}</th>
                    <th>{t("topology.tableId")}</th>
                    <th>{t("topology.tableIpv4")}</th>
                    <th>{t("topology.tableIpv6")}</th>
                  </tr>
                </thead>
                <tbody>
                  <Show when={!isScanning()} fallback={<TopologySkeletonRows colspan={4} rows={4} />}>
                    <Show
                      when={(topologyQuery.data?.adapters.length ?? 0) > 0}
                      fallback={
                        <tr>
                          <td colspan={4} class="muted">
                            {t("topology.noAdaptersData")}
                          </td>
                        </tr>
                      }
                    >
                    <For each={topologyQuery.data?.adapters ?? []}>
                      {(item) => (
                        <tr>
                          <td><EllipsisCell text={item.name} /></td>
                          <td><EllipsisCell text={item.id} /></td>
                          <td><EllipsisCell text={item.ipv4.join(", ") || "-"} /></td>
                          <td><EllipsisCell text={item.ipv6.join(", ") || "-"} /></td>
                        </tr>
                      )}
                    </For>
                    </Show>
                  </Show>
                </tbody>
              </table>
            </div>
        </SectionCard>
      </div>
    </div>
  );
}

function TopologySkeletonRows(props: { colspan: number; rows: number }) {
  return (
    <For each={Array.from({ length: props.rows })}>
      {() => (
        <tr>
          <td colspan={props.colspan}>
            <SkeletonLine />
          </td>
        </tr>
      )}
    </For>
  );
}
