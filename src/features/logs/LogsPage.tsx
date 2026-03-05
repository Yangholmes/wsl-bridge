import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import * as KButton from "@kobalte/core/button";
import * as KTooltip from "@kobalte/core/tooltip";

import { queryLogs } from "../rules/api";
import type { AuditLog } from "../../lib/types";
import { useI18n } from "../../i18n/context";

type ReplayWindow = "15m" | "1h" | "6h" | "24h" | "all";

function toLocalTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

function replayWindowToStartIso(value: ReplayWindow): string | null {
  if (value === "all") return null;
  const minutes = value === "15m" ? 15 : value === "1h" ? 60 : value === "6h" ? 360 : 1440;
  return new Date(Date.now() - minutes * 60_000).toISOString();
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
  const [logs, setLogs] = createSignal<AuditLog[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [total, setTotal] = createSignal(0);
  const [message, setMessage] = createSignal<{ type: "info" | "error"; text: string } | null>(null);
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
      setMessage({
        type: "info",
        text: t("logs.refreshed", { total: result.total, shown: result.events.length })
      });
    } catch (err) {
      setMessage({ type: "error", text: String(err) });
    } finally {
      setLoading(false);
    }
  }

  function exportFilteredCsv() {
    const data = logs();
    if (data.length === 0) {
      setMessage({ type: "error", text: t("logs.emptyExport") });
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
    setMessage({ type: "info", text: t("logs.exported", { count: data.length }) });
  }

  const moduleOptions = createMemo(() => {
    const set = new Set<string>();
    for (const log of logs()) set.add(log.module);
    return ["all", ...[...set].sort((a, b) => a.localeCompare(b))];
  });

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
          <label class="kb-checkbox">
            <input
              type="checkbox"
              checked={autoTail()}
              onChange={(e) => setAutoTail(e.currentTarget.checked)}
            />
            <span class="kb-checkbox-label">{t("logs.autoRefresh")}</span>
          </label>
          <KButton.Root class="kb-btn ghost" onClick={refreshLogs} disabled={loading()}>
            {t("common.refresh")}
          </KButton.Root>
          <KButton.Root class="kb-btn ghost" onClick={exportFilteredCsv}>
            {t("logs.exportCsv")}
          </KButton.Root>
        </div>
      </div>

      <div class="logs-toolbar logs-toolbar-extended">
        <select class="kb-input" value={levelFilter()} onInput={(e) => setLevelFilter(e.currentTarget.value)}>
          <option value="all">{t("logs.levelAll")}</option>
          <option value="info">info</option>
          <option value="warn">warn</option>
          <option value="error">error</option>
        </select>
        <select class="kb-input" value={moduleFilter()} onInput={(e) => setModuleFilter(e.currentTarget.value)}>
          <For each={moduleOptions()}>
            {(item) => <option value={item}>{item === "all" ? t("common.all") : item}</option>}
          </For>
        </select>
        <select
          class="kb-input"
          value={replayWindow()}
          onInput={(e) => setReplayWindow(e.currentTarget.value as ReplayWindow)}
        >
          <option value="15m">{t("logs.replay15m")}</option>
          <option value="1h">{t("logs.replay1h")}</option>
          <option value="6h">{t("logs.replay6h")}</option>
          <option value="24h">{t("logs.replay24h")}</option>
          <option value="all">{t("logs.replayAll")}</option>
        </select>
        <select class="kb-input" value={String(limit())} onInput={(e) => setLimit(Number(e.currentTarget.value))}>
          <option value="100">{t("logs.show100")}</option>
          <option value="300">{t("logs.show300")}</option>
          <option value="600">{t("logs.show600")}</option>
          <option value="1000">{t("logs.show1000")}</option>
        </select>
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

      <div class="hint info">{t("logs.matchTotalHint", { total: total() })}</div>

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
                        <div class="skeleton-line" />
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
                      <td>{renderEllipsisCell(String(item.id))}</td>
                      <td>{renderEllipsisCell(toLocalTime(item.time))}</td>
                      <td>{renderEllipsisCell(item.level)}</td>
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

      <Show when={message()}>
        {(msg) => <div class={`hint ${msg().type === "error" ? "error" : "info"}`}>{msg().text}</div>}
      </Show>
    </section>
  );
}
