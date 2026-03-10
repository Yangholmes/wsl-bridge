import { createSignal, For, Show } from "solid-js";
import { queryOptions, useQuery } from "@tanstack/solid-query";
import * as KButton from "@kobalte/core/button";
import * as KDialog from "@kobalte/core/dialog";

import { debugHyperVProbe } from "../rules/api";
import { appQueryClient } from "../../lib/queryClient";
import { createTopologyQueryOptions, getGlobalTargetKind, getGlobalTargetRef } from "./state";
import type { HyperVProbeDebug } from "../../lib/types";
import { useI18n } from "../../i18n/context";
import { EllipsisCell } from "../../lib/EllipsisCell";

function toLocalTime(value: string | null) {
  if (!value) return "-";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

export function TopologyPage() {
  const { t } = useI18n();
  const [debugOpen, setDebugOpen] = createSignal(false);

  const topologyQuery = useQuery(() => createTopologyQueryOptions(true), () => appQueryClient);

  const hypervProbeQuery = useQuery(() =>
    queryOptions<HyperVProbeDebug>({
      queryKey: ["topology", "hyperv-probe"],
      queryFn: debugHyperVProbe,
      enabled: debugOpen(),
      staleTime: 10000,
      refetchOnWindowFocus: false
    }),
    () => appQueryClient
  );
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
            <Show when={topologyQuery.data?.hyperv_error}>
              {(err) => <div class="hint error">{t("topology.adminRequired", { error: String(err()) })}</div>}
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
            <div class="topology-debug-actions">
              <KButton.Root class="kb-btn ghost small" onClick={() => setDebugOpen(true)}>
                {t("topology.debugButton")}
              </KButton.Root>
            </div>
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
      <KDialog.Root open={debugOpen()} onOpenChange={setDebugOpen}>
        <KDialog.Portal>
          <KDialog.Overlay class="kb-dialog-overlay" />
          <KDialog.Content class="kb-dialog-content topology-debug-modal">
            <div class="panel-title">
              <KDialog.Title as="h2">{t("topology.debugTitle")}</KDialog.Title>
            </div>
            <Show when={hypervProbeQuery.isPending}>
              <div class="topology-debug-block">
                <div class="skeleton-line wide" />
                <For each={[1, 2, 3, 4, 5, 6]}>
                  {() => <div class="skeleton-line" />}
                </For>
              </div>
            </Show>
            <Show when={hypervProbeQuery.error}>
              {(err) => <div class="hint error topology-debug-block">{String(err())}</div>}
            </Show>
            <Show when={hypervProbeQuery.data}>
              {(debug) => (
                <div class="topology-debug-block">
                  <div class="muted">{t("topology.latestDebug", { value: toLocalTime(debug().timestamp) })}</div>
                  <div>{t("topology.selectedVm", { value: debug().selected_vm_names.join(", ") || t("common.none") })}</div>
                  <For each={debug().steps}>
                    {(step) => (
                      <section class="panel topology-debug-step">
                        <h2>{step.source}</h2>
                        <div class="muted">{t("topology.executable", { value: step.executable || t("common.none") })}</div>
                        <div>
                          {t("topology.status", { value: step.ok ? "ok" : "failed", code: step.status_code })}
                        </div>
                        <div>{t("topology.parsedVmNames", { value: step.parsed_vm_names.join(", ") || t("common.none") })}</div>
                        <div class="muted">{t("topology.stdout")}</div>
                        <pre>{step.raw_stdout || t("common.none")}</pre>
                        <div class="muted">{t("topology.stderr")}</div>
                        <pre>{step.raw_stderr || t("common.none")}</pre>
                      </section>
                    )}
                  </For>
                </div>
              )}
            </Show>
            <div class="actions modal-actions">
              <KButton.Root class="kb-btn ghost" onClick={() => setDebugOpen(false)}>
                {t("common.close")}
              </KButton.Root>
            </div>
          </KDialog.Content>
        </KDialog.Portal>
      </KDialog.Root>
    </div>
  );
}

function TopologySkeletonRows(props: { colspan: number; rows: number }) {
  return (
    <For each={Array.from({ length: props.rows })}>
      {() => (
        <tr>
          <td colspan={props.colspan}>
            <div class="skeleton-line" />
          </td>
        </tr>
      )}
    </For>
  );
}
