import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import * as KButton from "@kobalte/core/button";
import * as KCheckbox from "@kobalte/core/checkbox";

import { queryLogs } from "../rules/api";
import type { AuditLog } from "../../lib/types";
import { useI18n } from "../../i18n/context";
import { EllipsisCell } from "../../lib/EllipsisCell";
import { SimpleSelect, type SelectOption } from "../../lib/SimpleSelect";
import { toLocalTime, type ReplayWindow, replayWindowToStartIso, replayWindowOptions } from "../../lib/datetime";
import { SkeletonLine } from "../../lib/Skeleton";
import { Hint } from "../../lib/Hint";
import { useToast } from "../../lib/Toast";

function toCsv(logs: AuditLog[]) {
  const esc = (text: string) => `"${text.replaceAll("\"", "\"\"")}"`;
  const rows = [
    ["id", "time", "level", "module", "event", "detail"],
    ...logs.map((item) => [
      String(item.id),
      item.time,
      item.level,
      item.module,
      item.event,
      item.detail
    ])
  ];
  return rows.map((row) => row.map(esc).join(",")).join("\n");
}

export function LogsPage() {
  const { t } = useI18n();
  const toast = useToast();
  const [logs, setLogs] = createSignal<AuditLog[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [total, setTotal] = createSignal(0);
  const [autoTail, setAutoTail] = createSignal(true);

  const [levelFilter, setLevelFilter] = createSignal("all");
  const [moduleFilter, setModuleFilter] = createSignal("all");
  const [keyword, setKeyword] = createSignal("");
  const [ruleIdFilter, setRuleIdFilter] = createSignal("");
  const [replayWindow, setReplayWindow] = createSignal<ReplayWindow>("1h");
  const [limit, setLimit] = createSignal(300);

  let timer: number | undefined;

  async function refreshLogs() {
    try {
      setLoading(true);
      const result = await queryLogs({
        level: levelFilter() === "all" ? null : levelFilter(),
        module: moduleFilter() === "all" ? null : moduleFilter(),
        rule_id: ruleIdFilter().trim() || null,
        keyword: keyword().trim() || null,
        start_time: replayWindowToStartIso(replayWindow()),
        newest_first: true,
        limit: limit()
      });
      setLogs(result.events);
      setTotal(result.total);
    } catch (err) {
      toast.error(String(err));
    } finally {
      setLoading(false);
    }
  }

  function exportFilteredCsv() {
    const data = logs();
    if (data.length === 0) {
      toast.error(t("logs.emptyExport"));
      return;
    }
    const blob = new Blob([toCsv(data)], { type: "text/csv;charset=utf-8;" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    const now = new Date().toISOString().replaceAll(":", "-");
    a.href = url;
    a.download = `wsl-bridge-logs-${now}.csv`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
    toast.success(t("logs.exported", { count: data.length }));
  }

  const moduleOptions = createMemo(() => {
    const set = new Set<string>();
    for (const log of logs()) set.add(log.module);
    return ["all", ...[...set].sort((a, b) => a.localeCompare(b))];
  });

  const levelOptions: SelectOption[] = [
    { value: "all", label: "all" },
    { value: "info", label: "info" },
    { value: "warn", label: "warn" },
    { value: "error", label: "error" }
  ];

  const limitOptions: SelectOption[] = [
    { value: "100", label: "100" },
    { value: "300", label: "300" },
    { value: "600", label: "600" },
    { value: "1000", label: "1000" }
  ];

  createEffect(() => {
    if (timer) {
      clearInterval(timer);
      timer = undefined;
    }
    if (!autoTail()) return;
    timer = window.setInterval(() => {
      void refreshLogs();
    }, 1800);
  });

  onMount(() => {
    void refreshLogs();
  });

  onCleanup(() => {
    if (timer) clearInterval(timer);
  });

  return (
    <section class="panel">
      <div class="panel-title">
        <h2>{t("logs.title")}</h2>
        <div class="runtime-tools">
          <KCheckbox.Root
            checked={autoTail()}
            onChange={setAutoTail}
            class="kb-checkbox"
          >
            <KCheckbox.Input />
            <KCheckbox.Control class="kb-checkbox-control">
              <KCheckbox.Indicator class="kb-checkbox-indicator" />
            </KCheckbox.Control>
            <KCheckbox.Label class="kb-checkbox-label">{t("logs.autoRefresh")}</KCheckbox.Label>
          </KCheckbox.Root>
          <KButton.Root class="kb-btn ghost" onClick={refreshLogs} disabled={loading()}>
            {t("common.refresh")}
          </KButton.Root>
          <KButton.Root class="kb-btn ghost" onClick={exportFilteredCsv}>
            {t("logs.exportCsv")}
          </KButton.Root>
        </div>
      </div>

      <div class="logs-toolbar logs-toolbar-extended">
        <SimpleSelect
          class="kb-input"
          value={levelFilter()}
          onChange={setLevelFilter}
          options={levelOptions}
        />
        <SimpleSelect
          class="kb-input"
          value={moduleFilter()}
          onChange={setModuleFilter}
          options={moduleOptions().map((v) => ({ value: v, label: v === "all" ? v : v }))}
        />
        <SimpleSelect
          class="kb-input"
          value={replayWindow()}
          onChange={(v) => setReplayWindow(v as ReplayWindow)}
          options={replayWindowOptions}
        />
        <SimpleSelect
          class="kb-input"
          value={String(limit())}
          onChange={(v) => setLimit(Number(v))}
          options={limitOptions}
        />
        <input
          class="kb-input"
          placeholder={t("logs.ruleIdPlaceholder")}
          value={ruleIdFilter()}
          onInput={(e) => setRuleIdFilter(e.currentTarget.value)}
        />
        <input
          class="kb-input"
          placeholder={t("logs.keywordPlaceholder")}
          value={keyword()}
          onInput={(e) => setKeyword(e.currentTarget.value)}
        />
      </div>

      <Hint>{t("logs.matchTotalHint", { total: total() })}</Hint>

      <div class="table-wrap">
        <table class="rules-table">
          <thead>
            <tr>
              <th>ID</th>
              <th>{t("logs.tableTime")}</th>
              <th>{t("logs.tableLevel")}</th>
              <th>{t("logs.tableModule")}</th>
              <th>{t("logs.tableEvent")}</th>
              <th>{t("logs.tableDetail")}</th>
            </tr>
          </thead>
          <tbody>
            <Show
              when={!loading() || logs().length > 0}
              fallback={
                <For each={[1, 2, 3, 4, 5]}>
                  {() => (
                    <tr>
                      <td colspan={6}>
                        <SkeletonLine />
                      </td>
                    </tr>
                  )}
                </For>
              }
            >
              <Show
                when={logs().length > 0}
                fallback={
                    <tr>
                      <td colspan={6} class="muted">
                        {t("logs.noLogs")}
                      </td>
                    </tr>
                  }
              >
                <For each={logs()}>
                  {(item) => (
                    <tr>
                      <td><EllipsisCell text={String(item.id)} /></td>
                      <td><EllipsisCell text={toLocalTime(item.time)} /></td>
                      <td><EllipsisCell text={item.level} /></td>
                      <td><EllipsisCell text={item.module} /></td>
                      <td><EllipsisCell text={item.event} /></td>
                      <td><EllipsisCell text={item.detail} /></td>
                    </tr>
                  )}
                </For>
              </Show>
            </Show>
          </tbody>
        </table>
      </div>
    </section>
  );
}
