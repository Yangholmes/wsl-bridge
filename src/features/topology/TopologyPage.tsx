import { createSignal, For, Show } from "solid-js";
import { createQuery } from "@tanstack/solid-query";
import * as KButton from "@kobalte/core/button";
import * as KTooltip from "@kobalte/core/tooltip";

import { debugHyperVProbe } from "../rules/api";
import { createTopologyQueryOptions, getGlobalTargetKind, getGlobalTargetRef } from "./state";

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

export function TopologyPage() {
  const [debugOpen, setDebugOpen] = createSignal(false);

  const topologyQuery = createQuery(() => createTopologyQueryOptions(true));

  const hypervProbeQuery = createQuery(() => ({
    queryKey: ["topology", "hyperv-probe"],
    queryFn: debugHyperVProbe,
    enabled: debugOpen(),
    staleTime: 10000,
    refetchOnWindowFocus: false
  }));

  return (
    <div class="page">
      <section class="panel">
        <div class="panel-title">
          <h2>Topology</h2>
          <div class="runtime-tools">
            <span class="muted">目标上下文：{getGlobalTargetKind()} / {getGlobalTargetRef() || "-"}</span>
            <span class="muted">最近扫描：{toLocalTime(topologyQuery.data?.timestamp ?? null)}</span>
            <KButton.Root class="kb-btn ghost" onClick={() => topologyQuery.refetch()}>
              重新扫描
            </KButton.Root>
          </div>
        </div>

        <div class="topology-grid">
          <section class="panel topology-subpanel">
            <h2>WSL</h2>
            <div class="table-wrap">
              <table class="rules-table">
                <thead>
                  <tr>
                    <th>Distro</th>
                    <th>Mode</th>
                    <th>IP</th>
                  </tr>
                </thead>
                <tbody>
                  <Show
                    when={(topologyQuery.data?.wsl.length ?? 0) > 0}
                    fallback={
                      <tr>
                        <td colspan={3} class="muted">
                          无 WSL 拓扑数据
                        </td>
                      </tr>
                    }
                  >
                    <For each={topologyQuery.data?.wsl ?? []}>
                      {(item) => (
                        <tr>
                          <td>{renderEllipsisCell(item.distro)}</td>
                          <td>{renderEllipsisCell(item.networking_mode)}</td>
                          <td>{renderEllipsisCell(item.ip ?? "-")}</td>
                        </tr>
                      )}
                    </For>
                  </Show>
                </tbody>
              </table>
            </div>
          </section>

          <section class="panel topology-subpanel">
            <h2>Hyper-V</h2>
            <Show when={topologyQuery.data?.hyperv_error}>
              {(err) => <div class="hint error">Hyper-V：{String(err())} 请以管理员身份启动应用后重试。</div>}
            </Show>
            <div class="table-wrap">
              <table class="rules-table">
                <thead>
                  <tr>
                    <th>VM</th>
                    <th>vSwitch</th>
                    <th>IP</th>
                  </tr>
                </thead>
                <tbody>
                  <Show
                    when={(topologyQuery.data?.hyperv.length ?? 0) > 0}
                    fallback={
                      <tr>
                        <td colspan={3} class="muted">
                          无 Hyper-V 拓扑数据
                        </td>
                      </tr>
                    }
                  >
                    <For each={topologyQuery.data?.hyperv ?? []}>
                      {(item) => (
                        <tr>
                          <td>{renderEllipsisCell(item.vm_name)}</td>
                          <td>{renderEllipsisCell(item.v_switch ?? "-")}</td>
                          <td>{renderEllipsisCell(item.ip ?? "-")}</td>
                        </tr>
                      )}
                    </For>
                  </Show>
                </tbody>
              </table>
            </div>
            <details
              class="topology-debug-details"
              onToggle={(e) => setDebugOpen((e.currentTarget as HTMLDetailsElement).open)}
            >
              <summary>Hyper-V 原始探测结果（调试）</summary>
              <Show when={hypervProbeQuery.isPending}>
                <div class="muted topology-debug-block">加载调试信息中...</div>
              </Show>
              <Show when={hypervProbeQuery.error}>
                {(err) => <div class="hint error topology-debug-block">{String(err())}</div>}
              </Show>
              <Show when={hypervProbeQuery.data}>
                {(debug) => (
                  <div class="topology-debug-block">
                    <div class="muted">最近调试：{toLocalTime(debug().timestamp)}</div>
                    <div>最终识别 VM：{debug().selected_vm_names.join(", ") || "-"}</div>
                    <For each={debug().steps}>
                      {(step) => (
                        <section class="panel topology-debug-step">
                          <h2>{step.source}</h2>
                          <div class="muted">executable: {step.executable || "-"}</div>
                          <div>
                            status: {step.ok ? "ok" : "failed"} (code={step.status_code})
                          </div>
                          <div>parsed vm names: {step.parsed_vm_names.join(", ") || "-"}</div>
                          <div class="muted">stdout</div>
                          <pre>{step.raw_stdout || "-"}</pre>
                          <div class="muted">stderr</div>
                          <pre>{step.raw_stderr || "-"}</pre>
                        </section>
                      )}
                    </For>
                  </div>
                )}
              </Show>
            </details>
          </section>

          <section class="panel topology-subpanel">
            <h2>Adapters</h2>
            <div class="table-wrap">
              <table class="rules-table">
                <thead>
                  <tr>
                    <th>名称</th>
                    <th>ID</th>
                    <th>IPv4</th>
                    <th>IPv6</th>
                  </tr>
                </thead>
                <tbody>
                  <Show
                    when={(topologyQuery.data?.adapters.length ?? 0) > 0}
                    fallback={
                      <tr>
                        <td colspan={4} class="muted">
                          无网卡数据
                        </td>
                      </tr>
                    }
                  >
                    <For each={topologyQuery.data?.adapters ?? []}>
                      {(item) => (
                        <tr>
                          <td>{renderEllipsisCell(item.name)}</td>
                          <td>{renderEllipsisCell(item.id)}</td>
                          <td>{renderEllipsisCell(item.ipv4.join(", ") || "-")}</td>
                          <td>{renderEllipsisCell(item.ipv6.join(", ") || "-")}</td>
                        </tr>
                      )}
                    </For>
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
