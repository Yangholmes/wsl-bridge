import { createMemo, createSignal, For, Show } from "solid-js";
import { queryOptions, useQuery } from "@tanstack/solid-query";
import * as KButton from "@kobalte/core/button";

import { getRuntimeStatus, listRules } from "../rules/api";
import { appQueryClient } from "../../lib/queryClient";
import type { ProxyRule, RuntimeState, RuntimeStatusItem } from "../../lib/types";
import { useI18n } from "../../i18n/context";
import { EllipsisCell } from "../../lib/EllipsisCell";
import { SimpleSelect, type SelectOption } from "../../lib/SimpleSelect";
import { toLocalTime } from "../../lib/datetime";
import { SkeletonLine } from "../../lib/Skeleton";
import { Hint } from "../../lib/Hint";

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
  const [stateFilter, setStateFilter] = createSignal<"all" | RuntimeState>("all");

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
    await Promise.all([rulesQuery.refetch(), runtimeQuery.refetch()]);
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
              </tr>
            </thead>
            <tbody>
              <Show
                when={!isTableLoading()}
                fallback={
                  <For each={[1, 2, 3, 4]}>
                    {() => (
                      <tr>
                        <td colspan={4}>
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
                      <td colspan={4} class="muted">
                        {t("runtime.noRuntimeData")}
                      </td>
                    </tr>
                  }
                >
                  <For each={filteredRows()}>
                    {(item) => {
                      return (
                        <tr class={item.state === "error" ? "runtime-row-error" : undefined}>
                          <td><EllipsisCell text={item.name} /></td>
                          <td><EllipsisCell text={t(`common.${item.state}`)} /></td>
                          <td><EllipsisCell text={toLocalTime(item.last_apply_at)} /></td>
                          <td><EllipsisCell text={item.last_error ?? "-"} /></td>
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
    </div>
  );
}
