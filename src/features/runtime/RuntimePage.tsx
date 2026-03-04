import { createMemo, createSignal, For, Show } from "solid-js";
import { createQuery } from "@tanstack/solid-query";
import * as KButton from "@kobalte/core/button";
import * as KTooltip from "@kobalte/core/tooltip";

import { getRuntimeStatus, listRules, tailLogs } from "../rules/api";
import type { AuditLog, RuntimeState } from "../../lib/types";

type RuntimeRow = {
  rule_id: string;
  name: string;
  state: RuntimeState;
  last_apply_at: string | null;
  last_error: string | null;
};

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

export function RuntimePage() {
  const [stateFilter, setStateFilter] = createSignal<"all" | RuntimeState>("all");
  const [relatedLogs, setRelatedLogs] = createSignal<AuditLog[]>([]);
  const [selectedRuleId, setSelectedRuleId] = createSignal<string | null>(null);
  const [message, setMessage] = createSignal<string | null>(null);

  const rulesQuery = createQuery(() => ({
    queryKey: ["rules", "runtime-page"],
    queryFn: listRules,
    staleTime: 15000,
    refetchOnWindowFocus: false
  }));

  const runtimeQuery = createQuery(() => ({
    queryKey: ["runtime", "runtime-page"],
    queryFn: getRuntimeStatus,
    refetchInterval: 5000,
    refetchOnWindowFocus: false
  }));

  const rows = createMemo<RuntimeRow[]>(() => {
    const rules = new Map((rulesQuery.data ?? []).map((item) => [item.id, item]));
    const items = (runtimeQuery.data ?? []).map((item) => ({
      rule_id: item.rule_id,
      name: rules.get(item.rule_id)?.name ?? item.rule_id,
      state: item.state,
      last_apply_at: item.last_apply_at,
      last_error: item.last_error
    }));
    items.sort((a, b) => a.name.localeCompare(b.name));
    return items;
  });

  const filteredRows = createMemo(() => {
    if (stateFilter() === "all") return rows();
    return rows().filter((item) => item.state === stateFilter());
  });

  async function refreshAll() {
    await Promise.all([rulesQuery.refetch(), runtimeQuery.refetch()]);
  }

  async function loadRelatedLogs(ruleId: string) {
    try {
      setSelectedRuleId(ruleId);
      const result = await tailLogs(0);
      const events = result.events.filter(
        (event) =>
          event.detail.includes(ruleId) ||
          event.event.includes(ruleId) ||
          event.module.includes(ruleId)
      );
      setRelatedLogs(events.slice(-120));
      setMessage(`rule_id=${ruleId}，关联日志 ${events.length} 条`);
    } catch (err) {
      setMessage(String(err));
    }
  }

  return (
    <div class="page">
      <section class="panel">
        <div class="panel-title">
          <h2>Runtime</h2>
          <div class="runtime-tools">
            <select
              class="kb-input runtime-filter"
              value={stateFilter()}
              onInput={(e) => setStateFilter(e.currentTarget.value as "all" | RuntimeState)}
            >
              <option value="all">all</option>
              <option value="running">running</option>
              <option value="stopped">stopped</option>
              <option value="error">error</option>
            </select>
            <KButton.Root class="kb-btn ghost" onClick={refreshAll}>
              刷新
            </KButton.Root>
          </div>
        </div>

        <div class="table-wrap">
          <table class="rules-table">
            <thead>
              <tr>
                <th>规则</th>
                <th>状态</th>
                <th>最近应用</th>
                <th>错误</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              <Show
                when={filteredRows().length > 0}
                fallback={
                  <tr>
                    <td colspan={5} class="muted">
                      暂无运行态数据
                    </td>
                  </tr>
                }
              >
                <For each={filteredRows()}>
                  {(item) => (
                    <tr class={item.state === "error" ? "runtime-row-error" : undefined}>
                      <td>{renderEllipsisCell(item.name)}</td>
                      <td>{renderEllipsisCell(item.state)}</td>
                      <td>{renderEllipsisCell(toLocalTime(item.last_apply_at))}</td>
                      <td>{renderEllipsisCell(item.last_error ?? "-")}</td>
                      <td>
                        <KButton.Root
                          class="kb-btn ghost"
                          onClick={() => loadRelatedLogs(item.rule_id)}
                        >
                          查看关联日志
                        </KButton.Root>
                      </td>
                    </tr>
                  )}
                </For>
              </Show>
            </tbody>
          </table>
        </div>

        <Show when={message()}>
          {(text) => <div class="hint info">{text()}</div>}
        </Show>
      </section>

      <section class="panel">
        <h2>关联日志 {selectedRuleId() ? `(${selectedRuleId()})` : ""}</h2>
        <div class="table-wrap">
          <table class="rules-table">
            <thead>
              <tr>
                <th>时间</th>
                <th>级别</th>
                <th>模块</th>
                <th>事件</th>
                <th>详情</th>
              </tr>
            </thead>
            <tbody>
              <Show
                when={relatedLogs().length > 0}
                fallback={
                  <tr>
                    <td colspan={5} class="muted">
                      请在上方运行态列表中选择规则以查看关联日志
                    </td>
                  </tr>
                }
              >
                <For each={relatedLogs()}>
                  {(log) => (
                    <tr>
                      <td>{renderEllipsisCell(toLocalTime(log.time))}</td>
                      <td>{renderEllipsisCell(log.level)}</td>
                      <td>{renderEllipsisCell(log.module)}</td>
                      <td>{renderEllipsisCell(log.event)}</td>
                      <td>{renderEllipsisCell(log.detail)}</td>
                    </tr>
                  )}
                </For>
              </Show>
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}
