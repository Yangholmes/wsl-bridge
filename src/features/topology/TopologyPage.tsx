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

export function TopologyPage() {
  const { t } = useI18n();
  const runtimeStatusQuery = useAppRuntimeStatusQuery();

  const topologyQuery = useQuery(() => createTopologyQueryOptions(true), () => appQueryClient);
  const hasAdminPrivileges = () => runtimeStatusQuery.data?.admin_features_available ?? false;

  const isScanning = () => topologyQuery.isFetching;

  return (
    <div class="page">
      <section class="page-shell">
        <div class="panel-title">
          <h2>{t("topology.title")}</h2>
          <div class="runtime-tools">
            <span class="muted">
              {t("topology.targetContext", {
                kind: getGlobalTargetKind(),
                ref: getGlobalTargetRef() || t("common.none")
              })}
            </span>
            <span class="muted">{t("topology.lastScanned", { value: toLocalTime(topologyQuery.data?.timestamp ?? null) })}</span>
            <KButton.Root
              class="kb-btn ghost"
              onClick={() => topologyQuery.refetch()}
              disabled={isScanning()}
            >
              {isScanning() ? t("common.scanning") : t("common.rescan")}
            </KButton.Root>
          </div>
        </div>

        <div class="topology-grid">
          <section class="panel topology-subpanel">
            <h2>{t("topology.wslTitle")}</h2>
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
          </section>

          <section class="panel topology-subpanel">
            <h2>{t("topology.hypervTitle")}</h2>
            <Show
              when={hasAdminPrivileges()}
              fallback={<Hint variant="info">{t("topology.hiddenWithoutAdmin")}</Hint>}
            >
              <Show when={topologyQuery.data?.hyperv_error}>
                {(err) => {
                  const errorStr = String(err());
                  const isNotEnabled = errorStr.includes("未启用") || errorStr.includes("not enabled") || errorStr.includes("有効");
                  return <Hint variant="error">{isNotEnabled ? t("topology.hypervNotEnabled") : t("topology.adminRequired", { error: errorStr })}</Hint>;
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
          </section>

          <section class="panel topology-subpanel">
            <h2>{t("topology.adaptersTitle")}</h2>
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
          </section>
        </div>
      </section>

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
