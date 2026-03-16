import { createEffect, createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import { queryOptions, useQuery } from "@tanstack/solid-query";
import * as KButton from "@kobalte/core/button";
import * as KCheckbox from "@kobalte/core/checkbox";
import * as KTooltip from "@kobalte/core/tooltip";

import { getRuleLogStats, getRuntimeStatus, listRules, queryLogs } from "../rules/api";
import { appQueryClient } from "../../lib/queryClient";
import type { AuditLog, ProxyRule, RuntimeState, RuleLogStatsItem, RuntimeStatusItem } from "../../lib/types";
import { useI18n } from "../../i18n/context";
import { EllipsisCell } from "../../lib/EllipsisCell";
import { SimpleSelect, type SelectOption } from "../../lib/SimpleSelect";
import { toLocalTime, type ReplayWindow, replayWindowToMinutes, replayWindowToStartIso, replayWindowOptions } from "../../lib/datetime";
import { SkeletonLine } from "../../lib/Skeleton";
import { Hint } from "../../lib/Hint";
import { useToast } from "../../lib/Toast";

type RuntimeRow = {
  rule_id: string;
  name: string;
  state: RuntimeState;
  last_apply_at: string | null;
  last_error: string | null;
};

const stateFilterOptions: SelectOption[] = [
  { value: "all", label: "all" },
  { value: "running", label: "running" },
  { value: "stopped", label: "stopped" },
  { value: "error", label: "error" }
];

export function RuntimePage() {
  const { t } = useI18n();
  const toast = useToast();
  const [stateFilter, setStateFilter] = createSignal<"all" | RuntimeState>("all");
  const [replayWindow, setReplayWindow] = createSignal<ReplayWindow>("1h");
  const [onlyErrors, setOnlyErrors] = createSignal(false);
  const [relatedLogs, setRelatedLogs] = createSignal<AuditLog[]>([]);
  const [selectedRuleId, setSelectedRuleId] = createSignal<string | null>(null);
  const [statsItems, setStatsItems] = createSignal<RuleLogStatsItem[]>([]);

  const rulesQuery = useQuery(() =>
    queryOptions<ProxyRule[]>({
      queryKey: ["rules", "runtime-page"],
      queryFn: listRules,
      staleTime: 15000,
      refetchOnWindowFocus: false
    }),
    () => appQueryClient
  );

  const runtimeQuery = useQuery(() =>
    queryOptions<RuntimeStatusItem[]>({
      queryKey: ["runtime", "runtime-page"],
      queryFn: getRuntimeStatus,
      refetchInterval: 5000,
      refetchOnWindowFocus: false
    }),
    () => appQueryClient
  );

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

  const statsRuleIds = createMemo(() => rows().map((item) => item.rule_id));

  async function refreshStats() {
    const ids = statsRuleIds();
    if (ids.length === 0) {
      setStatsItems([]);
      return;
    }
    const result = await getRuleLogStats({
      rule_ids: ids,
      since_minutes: replayWindowToMinutes(replayWindow())
    });
    setStatsItems(result);
  }

  createEffect(() => {
    const _window = replayWindow();
    const _idsKey = statsRuleIds().join(",");
    let cancelled = false;

    const run = async () => {
      try {
        const ids = statsRuleIds();
        if (ids.length === 0) {
          if (!cancelled) setStatsItems([]);
          return;
        }
        const result = await getRuleLogStats({
          rule_ids: ids,
          since_minutes: replayWindowToMinutes(replayWindow())
        });
        if (!cancelled) setStatsItems(result);
      } catch {
        if (!cancelled) setStatsItems([]);
      }
    };

    void run();
    const timer = window.setInterval(() => void run(), 5000);
    onCleanup(() => {
      cancelled = true;
      clearInterval(timer);
    });
  });

  const statsMap = createMemo(() => {
    const map = new Map<string, RuleLogStatsItem>();
    for (const item of statsItems()) {
      map.set(item.rule_id, item);
    }
    return map;
  });

  const filteredRows = createMemo(() => {
    if (stateFilter() === "all") return rows();
    return rows().filter((item) => item.state === stateFilter());
  });

  const runtimeSummary = createMemo(() => {
    const all = rows();
    return {
      total: all.length,
      running: all.filter((item) => item.state === "running").length,
      error: all.filter((item) => item.state === "error").length,
      stopped: all.filter((item) => item.state === "stopped").length
    };
  });
  const isTableLoading = createMemo(
    () => (rulesQuery.isPending || runtimeQuery.isPending) && rows().length === 0
  );

  async function refreshAll() {
    await Promise.all([rulesQuery.refetch(), runtimeQuery.refetch(), refreshStats()]);
  }

  async function loadRelatedLogs(ruleId: string) {
    try {
      setSelectedRuleId(ruleId);
      const result = await queryLogs({
        rule_id: ruleId,
        level: onlyErrors() ? "error" : null,
        start_time: replayWindowToStartIso(replayWindow()),
        newest_first: true,
        limit: 240
      });
      setRelatedLogs(result.events);
      toast.info(
        t("runtime.relatedLogMessage", {
          ruleId,
          total: result.total,
          shown: result.events.length
        })
      );
    } catch (err) {
      toast.error(String(err));
    }
  }

  return (
    <div class="page">
      <section class="panel">
        <div class="panel-title">
          <h2>{t("runtime.title")}</h2>
        </div>
        <div class="runtime-tools runtime-tools-row">
          <SimpleSelect
            class="kb-input runtime-filter"
            value={stateFilter()}
            onChange={(v) => setStateFilter(v as "all" | RuntimeState)}
            options={stateFilterOptions}
          />
          <SimpleSelect
            class="kb-input runtime-filter"
            value={replayWindow()}
            onChange={(v) => setReplayWindow(v as ReplayWindow)}
            options={replayWindowOptions}
          />
          <KCheckbox.Root
            checked={onlyErrors()}
            onChange={setOnlyErrors}
            class="kb-checkbox"
          >
            <KCheckbox.Input />
            <KCheckbox.Control class="kb-checkbox-control">
              <KCheckbox.Indicator class="kb-checkbox-indicator" />
            </KCheckbox.Control>
            <KCheckbox.Label class="kb-checkbox-label">{t("runtime.onlyErrors")}</KCheckbox.Label>
          </KCheckbox.Root>
          <KButton.Root class="kb-btn ghost" onClick={refreshAll}>
            {t("common.refresh")}
          </KButton.Root>
        </div>

        <Hint variant="info" class="runtime-summary">
          {t("runtime.summary", {
            total: runtimeSummary().total,
            running: runtimeSummary().running,
            error: runtimeSummary().error,
            stopped: runtimeSummary().stopped
          })}
        </Hint>

        <div class="table-wrap">
          <table class="rules-table">
            <thead>
              <tr>
                <th>{t("runtime.tableRule")}</th>
                <th>{t("runtime.tableState")}</th>
                <th>{t("runtime.tableLastApply")}</th>
                <th>{t("runtime.tableError")}</th>
                <th>{t("runtime.tableLogCount")}</th>
                <th>{t("runtime.tableErrorCount")}</th>
                <th>{t("runtime.tableLastErrorLog")}</th>
                <th>{t("runtime.tableAction")}</th>
              </tr>
            </thead>
            <tbody>
              <Show
                when={!isTableLoading()}
                fallback={
                  <For each={[1, 2, 3, 4]}>
                    {() => (
                      <tr>
                        <td colspan={8}>
                          <SkeletonLine />
                        </td>
                      </tr>
                    )}
                  </For>
                }
              >
                <Show
                  when={filteredRows().length > 0}
                  fallback={
                    <tr>
                      <td colspan={8} class="muted">
                        {t("runtime.noRuntimeData")}
                      </td>
                    </tr>
                  }
                >
                  <For each={filteredRows()}>
                    {(item) => {
                      const stats = () => statsMap().get(item.rule_id);
                      return (
                        <tr class={item.state === "error" ? "runtime-row-error" : undefined}>
                          <td><EllipsisCell text={item.name} /></td>
                          <td><EllipsisCell text={t(`common.${item.state}`)} /></td>
                          <td><EllipsisCell text={toLocalTime(item.last_apply_at)} /></td>
                          <td><EllipsisCell text={item.last_error ?? "-"} /></td>
                          <td><EllipsisCell text={String(stats()?.total ?? 0)} /></td>
                          <td><EllipsisCell text={String(stats()?.errors ?? 0)} /></td>
                          <td><EllipsisCell text={stats()?.last_error ?? "-"} /></td>
                          <td>
                            <KButton.Root
                              class="kb-btn ghost small"
                              onClick={() => loadRelatedLogs(item.rule_id)}
                            >
                              {t("runtime.viewRelatedLogs")}
                            </KButton.Root>
                          </td>
                        </tr>
                      );
                    }}
                  </For>
                </Show>
              </Show>
            </tbody>
          </table>
        </div>
      </section>

      <section class="panel">
        <h2>{t("runtime.relatedLogsTitle", { suffix: selectedRuleId() ? `(${selectedRuleId()})` : "" })}</h2>
        <div class="table-wrap">
          <table class="rules-table">
            <thead>
              <tr>
                <th>{t("runtime.tableTime")}</th>
                <th>{t("runtime.tableLevel")}</th>
                <th>{t("runtime.tableModule")}</th>
                <th>{t("runtime.tableEvent")}</th>
                <th>{t("runtime.tableDetail")}</th>
              </tr>
            </thead>
            <tbody>
              <Show
                when={relatedLogs().length > 0}
                fallback={
                    <tr>
                      <td colspan={5} class="muted">
                        {t("runtime.pickRuleHint")}
                      </td>
                    </tr>
                  }
              >
                <For each={relatedLogs()}>
                  {(log) => (
                    <tr>
                      <td><EllipsisCell text={toLocalTime(log.time)} /></td>
                      <td><EllipsisCell text={log.level} /></td>
                      <td><EllipsisCell text={log.module} /></td>
                      <td><EllipsisCell text={log.event} /></td>
                      <td><EllipsisCell text={log.detail} /></td>
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
