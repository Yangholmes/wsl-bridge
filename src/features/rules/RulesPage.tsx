import {
  createEffect,
  createMemo,
  createSignal,
  For,
  Show
} from "solid-js";
import { createStore } from "solid-js/store";
import { queryOptions, useQuery } from "@tanstack/solid-query";
import {
  createSolidTable,
  flexRender,
  getCoreRowModel,
  type ColumnDef
} from "@tanstack/solid-table";
import * as KButton from "@kobalte/core/button";
import * as KCheckbox from "@kobalte/core/checkbox";
import * as KSelect from "@kobalte/core/select";
import * as KSwitch from "@kobalte/core/switch";
import * as KTextField from "@kobalte/core/text-field";
import * as KTooltip from "@kobalte/core/tooltip";
import { useI18n } from "../../i18n/context";
import { appQueryClient } from "../../lib/queryClient";

import {
  applyRules,
  createRule,
  deleteRule,
  enableRule,
  getRuntimeStatus,
  listRules,
  stopRules,
  tailLogs,
  updateRule
} from "./api";
import {
  createTopologyQueryOptions,
  getGlobalTargetKind,
  getGlobalTargetRef,
  setGlobalTargetContext
} from "../topology/state";
import { RuleFormModal, type FormState, type SelectOption } from "./components/RuleFormModal";
import type {
  BindMode,
  CreateRuleRequest,
  ProxyRule,
  RuntimeStatusItem,
  RuleType,
  RuntimeState,
  RulePatch,
  TargetKind
} from "../../lib/types";

type RuleRow = ProxyRule & {
  runtime_state: RuntimeState | "unknown";
  last_error: string | null;
  last_apply_at: string | null;
};

type AppSelectProps = {
  value: string;
  onChange: (value: string) => void;
  options: SelectOption[];
  disabled?: boolean;
  placeholder?: string;
  triggerClass?: string;
};

const defaultForm: FormState = {
  name: "web-forward",
  type: "tcp_fwd",
  listen_host: "0.0.0.0",
  listen_port: "18081",
  target_kind: "static",
  target_ref: "",
  target_host: "127.0.0.1",
  target_port: "8080",
  bind_mode: "all_nics",
  nic_id: "",
  enabled: true,
  fw_domain: true,
  fw_private: true,
  fw_public: false
};

const ruleTypeOptions: SelectOption[] = [
  { value: "tcp_fwd", label: "tcp_fwd" },
  { value: "udp_fwd", label: "udp_fwd" },
  { value: "http_proxy", label: "http_proxy" },
  { value: "socks5_proxy", label: "socks5_proxy" }
];

const targetKindOptions: SelectOption[] = [
  { value: "static", label: "static" },
  { value: "wsl", label: "wsl" },
  { value: "hyperv", label: "hyperv" }
];

const bindModeOptions: SelectOption[] = [
  { value: "all_nics", label: "all_nics" },
  { value: "single_nic", label: "single_nic" }
];

const booleanOptions: SelectOption[] = [
  { value: "true", label: "true" },
  { value: "false", label: "false" }
];

const filterTypeOptions: SelectOption[] = [
  { value: "all", label: "all" },
  ...ruleTypeOptions
];

const filterEnabledOptions: SelectOption[] = [
  { value: "all", label: "all" },
  { value: "enabled", label: "enabled" },
  { value: "disabled", label: "disabled" }
];

const getPageSizeOptions = (t: ReturnType<typeof useI18n>["t"]): SelectOption[] => [
  { value: "10", label: t("rules.pageSize10") },
  { value: "20", label: t("rules.pageSize20") },
  { value: "50", label: t("rules.pageSize50") }
];

function toLocalTime(value: string | null) {
  if (!value) return "-";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

function AppSelect(props: AppSelectProps & { placeholderText?: string }) {
  const selectedOption = () => props.options.find((option) => option.value === props.value) ?? null;

  return (
    <KSelect.Root<SelectOption>
      options={props.options}
      optionValue="value"
      optionTextValue="label"
      value={selectedOption()}
      onChange={(option) => {
        if (option) props.onChange(option.value);
      }}
      itemComponent={(itemProps) => (
        <KSelect.Item item={itemProps.item} class="kb-select-item">
          <KSelect.ItemLabel>{itemProps.item.rawValue.label}</KSelect.ItemLabel>
          <KSelect.ItemIndicator class="kb-select-item-indicator">✓</KSelect.ItemIndicator>
        </KSelect.Item>
      )}
      disabled={props.disabled}
      placeholder={props.placeholderText ?? ""}
    >
      <KSelect.Trigger class={`kb-select-trigger ${props.triggerClass ?? ""}`}>
        <KSelect.Value<SelectOption>>{(state) => state.selectedOption()?.label}</KSelect.Value>
        <KSelect.Icon class="kb-select-icon">▾</KSelect.Icon>
      </KSelect.Trigger>
      <KSelect.Portal>
        <KSelect.Content class="kb-select-content">
          <KSelect.Listbox class="kb-select-listbox" />
        </KSelect.Content>
      </KSelect.Portal>
    </KSelect.Root>
  );
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

export function RulesPage() {
  const { t } = useI18n();
  const [form, setForm] = createStore<FormState>({ ...defaultForm });
  const [editingId, setEditingId] = createSignal<string | null>(null);
  const [isModalOpen, setIsModalOpen] = createSignal(false);
  const [selectedRuleIds, setSelectedRuleIds] = createSignal<Set<string>>(new Set<string>());
  const [pageIndex, setPageIndex] = createSignal(0);
  const [pageSize, setPageSize] = createSignal(10);
  const [message, setMessage] = createSignal<{ type: "info" | "error"; text: string } | null>(
    null
  );
  const [debugOutput, setDebugOutput] = createSignal("ready");

  const [filter, setFilter] = createStore({
    name: "",
    type: "all",
    enabled: "all"
  });
  const shouldLoadTopology = createMemo(
    () =>
      isModalOpen() &&
      (form.bind_mode === "single_nic" ||
        ((form.type === "tcp_fwd" || form.type === "udp_fwd") && form.target_kind !== "static"))
  );

  const rulesQuery = useQuery(() =>
    queryOptions<ProxyRule[]>({
    queryKey: ["rules"],
    queryFn: listRules
    }),
    () => appQueryClient
  );
  const runtimeQuery = useQuery(() =>
    queryOptions<RuntimeStatusItem[]>({
    queryKey: ["runtime"],
    queryFn: getRuntimeStatus
    }),
    () => appQueryClient
  );
  const topologyQuery = useQuery(
    () => createTopologyQueryOptions(shouldLoadTopology()),
    () => appQueryClient
  );

  const runtimeMap = createMemo(() => {
    const map = new Map<string, { state: RuntimeState; last_error: string | null; last_apply_at: string | null }>();
    for (const item of runtimeQuery.data ?? []) {
      map.set(item.rule_id, {
        state: item.state,
        last_error: item.last_error,
        last_apply_at: item.last_apply_at
      });
    }
    return map;
  });

  const rows = createMemo<RuleRow[]>(() => {
    return (rulesQuery.data ?? []).map((rule) => {
      const runtime = runtimeMap()?.get(rule.id);
      return {
        ...rule,
        runtime_state: runtime?.state ?? "unknown",
        last_error: runtime?.last_error ?? null,
        last_apply_at: runtime?.last_apply_at ?? null
      };
    });
  });

  const filteredRows = createMemo(() => {
    return rows().filter((rule) => {
      if (filter.name.trim()) {
        const keyword = filter.name.trim().toLowerCase();
        if (!rule.name.toLowerCase().includes(keyword)) return false;
      }
      if (filter.type !== "all" && rule.type !== filter.type) return false;
      if (filter.enabled === "enabled" && !rule.enabled) return false;
      if (filter.enabled === "disabled" && rule.enabled) return false;
      return true;
    });
  });
  const pageCount = createMemo(() => {
    const total = filteredRows().length;
    return total === 0 ? 1 : Math.ceil(total / pageSize());
  });
  const pagedRows = createMemo(() => {
    const start = pageIndex() * pageSize();
    return filteredRows().slice(start, start + pageSize());
  });
  const selectedCount = createMemo(() => selectedRuleIds().size);
  const isCurrentPageFullySelected = createMemo(() => {
    const rowsInPage = pagedRows();
    return rowsInPage.length > 0 && rowsInPage.every((rule) => selectedRuleIds().has(rule.id));
  });

  const adapterOptions = createMemo<SelectOption[]>(() => {
    const items = (topologyQuery.data?.adapters ?? []).map((adapter) => ({
      value: adapter.id,
      label: `${adapter.name} (${adapter.id})`
    }));
    return [{ value: "", label: t("rules.placeholderSelectNic") }, ...items];
  });
  const targetPreview = createMemo<string | null>(() => {
    const ref = form.target_ref.trim().toLowerCase();
    if (!ref || !topologyQuery.data) return null;
    if (form.target_kind === "wsl") {
      const item = topologyQuery.data.wsl.find((value) => value.distro.toLowerCase() === ref);
      return item?.ip ?? null;
    }
    if (form.target_kind === "hyperv") {
      const item = topologyQuery.data.hyperv.find((value) => value.vm_name.toLowerCase() === ref);
      return item?.ip ?? null;
    }
    return null;
  });
  const targetRefOptions = createMemo<SelectOption[]>(() => {
    let base: SelectOption[] = [];
    if (form.target_kind === "wsl") {
      base = (topologyQuery.data?.wsl ?? []).map((item) => ({
        value: item.distro,
        label: item.distro
      }));
    } else if (form.target_kind === "hyperv") {
      base = (topologyQuery.data?.hyperv ?? []).map((item) => ({
        value: item.vm_name,
        label: item.vm_name
      }));
    }
    if (!form.target_ref.trim()) return base;
    const exists = base.some((item) => item.value === form.target_ref.trim());
    if (exists) return base;
    return [{ value: form.target_ref.trim(), label: `${form.target_ref.trim()} ${t("rules.currentValue")}` }, ...base];
  });

  const isProxyType = createMemo(() => form.type === "http_proxy" || form.type === "socks5_proxy");
  const isSingleNic = createMemo(() => form.bind_mode === "single_nic");
  const isEditing = createMemo(() => editingId() !== null);
  const statusError = createMemo(() => rulesQuery.error ?? runtimeQuery.error ?? topologyQuery.error ?? null);
  const isStatusLoading = createMemo(() => rulesQuery.isPending || runtimeQuery.isPending || topologyQuery.isPending);
  const isTableLoading = createMemo(
    () => (rulesQuery.isPending || runtimeQuery.isPending) && rows().length === 0
  );

  createEffect(() => {
    if (isProxyType() && form.target_kind !== "static") {
      setForm("target_kind", "static");
    }
  });

  createEffect(() => {
    if (form.target_kind === "static") {
      if (form.target_ref) {
        setForm("target_ref", "");
      }
      setGlobalTargetContext("static", "");
      return;
    }
    const options = targetRefOptions();
    if (options.length === 0) {
      if (form.target_ref) {
        setForm("target_ref", "");
      }
      setGlobalTargetContext(form.target_kind, "");
      return;
    }
    const selected = form.target_ref.trim();
    const matched = options.some((option) => option.value === selected);
    const next = matched ? selected : options[0]!.value;
    if (next !== form.target_ref) {
      setForm("target_ref", next);
    }
    setGlobalTargetContext(form.target_kind, next);
  });

  createEffect(() => {
    if (!isSingleNic() && form.nic_id) {
      setForm("nic_id", "");
    }
  });

  createEffect(() => {
    const _name = filter.name;
    const _type = filter.type;
    const _enabled = filter.enabled;
    setPageIndex(0);
    setSelectedRuleIds(new Set<string>());
  });

  createEffect(() => {
    const maxPageIndex = pageCount() - 1;
    if (pageIndex() > maxPageIndex) {
      setPageIndex(maxPageIndex);
    }
  });

  createEffect(() => {
    const validIds = new Set(filteredRows().map((rule) => rule.id));
    setSelectedRuleIds((prev) => {
      if (prev.size === 0) return prev;
      const next = new Set<string>();
      for (const id of prev) {
        if (validIds.has(id)) next.add(id);
      }
      return next;
    });
  });

  function isRuleSelected(id: string) {
    return selectedRuleIds().has(id);
  }

  function setRuleSelected(id: string, checked: boolean) {
    setSelectedRuleIds((prev) => {
      const next = new Set(prev);
      if (checked) next.add(id);
      else next.delete(id);
      return next;
    });
  }

  function setCurrentPageSelected(checked: boolean) {
    const ids = pagedRows().map((rule) => rule.id);
    setSelectedRuleIds((prev) => {
      const next = new Set(prev);
      for (const id of ids) {
        if (checked) next.add(id);
        else next.delete(id);
      }
      return next;
    });
  }

  const columns: ColumnDef<RuleRow>[] = [
    {
      id: "select",
      header: () => (
        <KCheckbox.Root
          checked={isCurrentPageFullySelected()}
          onChange={setCurrentPageSelected}
          class="row-check"
        >
          <KCheckbox.Input />
          <KCheckbox.Control class="kb-checkbox-control">
            <KCheckbox.Indicator class="kb-checkbox-indicator" />
          </KCheckbox.Control>
        </KCheckbox.Root>
      ),
      cell: (ctx) => (
        <KCheckbox.Root
          checked={isRuleSelected(ctx.row.original.id)}
          onChange={(checked) => setRuleSelected(ctx.row.original.id, checked)}
          class="row-check"
        >
          <KCheckbox.Input />
          <KCheckbox.Control class="kb-checkbox-control">
            <KCheckbox.Indicator class="kb-checkbox-indicator" />
          </KCheckbox.Control>
        </KCheckbox.Root>
      )
    },
    { id: "name", header: () => t("rules.tableName"), cell: (ctx) => renderEllipsisCell(ctx.row.original.name) },
    { id: "type", header: () => t("rules.tableType"), cell: (ctx) => renderEllipsisCell(ctx.row.original.type) },
    {
      id: "listen",
      header: () => t("rules.tableListen"),
      cell: (ctx) => renderEllipsisCell(`${ctx.row.original.listen_host}:${ctx.row.original.listen_port}`)
    },
    {
      id: "target",
      header: () => t("rules.tableTarget"),
      cell: (ctx) => {
        const row = ctx.row.original;
        return renderEllipsisCell(`${row.target_kind}:${row.target_ref ?? row.target_host ?? "-"}:${row.target_port ?? "-"}`);
      }
    },
    {
      id: "runtime",
      header: () => t("rules.tableRuntime"),
      cell: (ctx) => renderEllipsisCell(ctx.row.original.runtime_state)
    },
    {
      id: "lastApply",
      header: () => t("rules.tableLastApply"),
      cell: (ctx) => renderEllipsisCell(toLocalTime(ctx.row.original.last_apply_at))
    },
    {
      id: "error",
      header: () => t("rules.tableError"),
      cell: (ctx) => renderEllipsisCell(ctx.row.original.last_error ?? "-")
    },
    {
      id: "action",
      header: () => t("rules.tableAction"),
      cell: (ctx) => {
        const row = ctx.row.original;
        return (
          <div class="row-actions">
            <KButton.Root class="kb-btn ghost small" onClick={() => handleEdit(row)}>
              ✏️
            </KButton.Root>
            <KButton.Root class="kb-btn danger small" onClick={() => handleDelete(row.id)}>
              ❌
            </KButton.Root>
          </div>
        );
      }
    },
    {
      id: "switch",
      header: () => t("rules.tableSwitch"),
      cell: (ctx) => {
        const row = ctx.row.original;
        return (
          <KSwitch.Root
            checked={row.enabled}
            onChange={(checked) => void handleToggle(row.id, checked)}
            class="kb-switch small row-enable-switch"
          >
            <KSwitch.Input aria-label={`${row.name} ${t("rules.tableSwitch")}`} />
            <KSwitch.Control class="kb-switch-control">
              <KSwitch.Thumb class="kb-switch-thumb" />
            </KSwitch.Control>
          </KSwitch.Root>
        );
      }
    }
  ];

  const table = createSolidTable({
    get data() {
      return pagedRows();
    },
    columns,
    getCoreRowModel: getCoreRowModel()
  });

  function resetForm() {
    setEditingId(null);
    setForm({
      ...defaultForm,
      target_kind: getGlobalTargetKind(),
      target_ref: getGlobalTargetRef()
    });
  }

  function openCreateModal() {
    resetForm();
    setMessage(null);
    setIsModalOpen(true);
  }

  function closeFormModal() {
    setIsModalOpen(false);
    resetForm();
  }

  function handleDialogOpenChange(open: boolean) {
    if (open) {
      setIsModalOpen(true);
      return;
    }
    closeFormModal();
  }

  function handleEdit(rule: RuleRow) {
    setEditingId(rule.id);
    setMessage(null);
    setForm({
      name: rule.name,
      type: rule.type,
      listen_host: rule.listen_host,
      listen_port: String(rule.listen_port),
      target_kind: rule.target_kind,
      target_ref: rule.target_ref ?? "",
      target_host: rule.target_host ?? "",
      target_port: rule.target_port == null ? "" : String(rule.target_port),
      bind_mode: rule.bind_mode,
      nic_id: rule.nic_id ?? "",
      enabled: rule.enabled,
      fw_domain: true,
      fw_private: true,
      fw_public: false
    });
    setGlobalTargetContext(rule.target_kind, rule.target_ref ?? "");
    setIsModalOpen(true);
  }

  async function refreshAll() {
    const jobs: Promise<unknown>[] = [rulesQuery.refetch(), runtimeQuery.refetch()];
    if (shouldLoadTopology()) {
      jobs.push(topologyQuery.refetch());
    }
    await Promise.all(jobs);
  }

  function validateForm(excludeId: string | null) {
    if (!form.name.trim()) return t("rules.validationNameEmpty");
    if (!form.listen_host.trim()) return t("rules.validationListenHostEmpty");
    const listenPort = Number(form.listen_port);
    if (!Number.isInteger(listenPort) || listenPort < 1 || listenPort > 65535) {
      return t("rules.validationListenPortRange");
    }
    if (isSingleNic() && !form.nic_id) return t("rules.validationSingleNicRequired");
    if (!form.fw_domain && !form.fw_private && !form.fw_public) {
      return t("rules.validationFirewallRequired");
    }

    if (!isProxyType()) {
      const targetPort = Number(form.target_port);
      if (!Number.isInteger(targetPort) || targetPort < 1 || targetPort > 65535) {
        return t("rules.validationTargetPortRange");
      }
      if (form.target_kind === "static" && !form.target_host.trim()) {
        return t("rules.validationStaticHostRequired");
      }
      if ((form.target_kind === "wsl" || form.target_kind === "hyperv") && !form.target_ref.trim()) {
        return t("rules.validationDynamicRefRequired", { kind: form.target_kind });
      }
    }

    const conflict = rows().find(
      (r) =>
        r.id !== excludeId &&
        r.listen_host === form.listen_host.trim() &&
        r.listen_port === Number(form.listen_port)
    );
    if (conflict) {
      return t("rules.validationListenConflict", { host: conflict.listen_host, port: conflict.listen_port, name: conflict.name });
    }
    return null;
  }

  function toCreateRequest(): CreateRuleRequest {
    return {
      rule: {
        name: form.name.trim(),
        type: form.type,
        listen_host: form.listen_host.trim(),
        listen_port: Number(form.listen_port),
        target_kind: form.target_kind,
        target_ref: isProxyType() || form.target_kind === "static" ? null : form.target_ref.trim(),
        target_host: isProxyType() || form.target_kind !== "static" ? null : form.target_host.trim(),
        target_port: isProxyType() ? null : Number(form.target_port),
        bind_mode: form.bind_mode,
        nic_id: form.bind_mode === "single_nic" ? form.nic_id : null,
        enabled: form.enabled
      },
      firewall: {
        allow_domain: form.fw_domain,
        allow_private: form.fw_private,
        allow_public: form.fw_public,
        direction: "inbound",
        action: "allow"
      }
    };
  }

  function toPatch(): RulePatch {
    const req = toCreateRequest().rule;
    return {
      name: req.name,
      listen_host: req.listen_host,
      listen_port: req.listen_port,
      target_ref: req.target_ref,
      target_host: req.target_host,
      target_port: req.target_port,
      bind_mode: req.bind_mode,
      nic_id: req.nic_id,
      enabled: req.enabled
    };
  }

  async function submitForm() {
    try {
      const error = validateForm(editingId());
      if (error) {
        setMessage({ type: "error", text: error });
        return;
      }

      if (editingId()) {
        const patch = toPatch();
        await updateRule(editingId()!, patch);
        if (patch.enabled) {
          const result = await applyRules();
          setDebugOutput(JSON.stringify({ updated_rule_id: editingId(), patch, auto_apply: result }, null, 2));
          setMessage({
            type: "info",
            text: t("rules.successUpdated", { id: editingId()!, applied: result.applied, failed: result.failed.length })
          });
        } else {
          setDebugOutput(JSON.stringify({ updated_rule_id: editingId(), patch }, null, 2));
          setMessage({ type: "info", text: t("rules.successUpdated", { id: editingId() ?? "", applied: 0, failed: 0 }) });
        }
      } else {
        const req = toCreateRequest();
        const id = await createRule(req);
        if (req.rule.enabled) {
          const result = await applyRules();
          setDebugOutput(JSON.stringify({ created_rule_id: id, request: req, auto_apply: result }, null, 2));
          setMessage({
            type: "info",
            text: t("rules.successCreated", { id, applied: result.applied, failed: result.failed.length })
          });
        } else {
          setDebugOutput(JSON.stringify({ created_rule_id: id, request: req }, null, 2));
          setMessage({ type: "info", text: t("rules.successCreated", { id, applied: 0, failed: 0 }) });
        }
      }

      closeFormModal();
      await refreshAll();
    } catch (err) {
      const text = String(err);
      setMessage({ type: "error", text });
      setDebugOutput(JSON.stringify({ error: text }, null, 2));
    }
  }

  async function handleDelete(id: string) {
    try {
      await deleteRule(id);
      if (editingId() === id) closeFormModal();
      await refreshAll();
      setMessage({ type: "info", text: t("rules.successDeleted", { id }) });
      setDebugOutput(JSON.stringify({ deleted_rule_id: id }, null, 2));
    } catch (err) {
      setMessage({ type: "error", text: String(err) });
    }
  }

  async function handleToggle(id: string, enabled: boolean) {
    try {
      await enableRule(id, enabled);
      const result = await applyRules();
      await refreshAll();
      setMessage({
        type: "info",
        text: t("rules.successToggled", { id, action: enabled ? t("common.enabled") : t("common.disabled"), applied: result.applied, failed: result.failed.length })
      });
      setDebugOutput(JSON.stringify({ toggled_rule_id: id, enabled, auto_apply: result }, null, 2));
    } catch (err) {
      setMessage({ type: "error", text: String(err) });
    }
  }

  async function handleBatchEnable(enabled: boolean) {
    const ids = [...selectedRuleIds()];
    if (ids.length === 0) {
      setMessage({ type: "error", text: t("rules.errorNoSelection") });
      return;
    }

    const failed: string[] = [];
    for (const id of ids) {
      try {
        await enableRule(id, enabled);
      } catch (err) {
        failed.push(`${id}: ${String(err)}`);
      }
    }

    try {
      const result = await applyRules();
      await refreshAll();
      if (failed.length > 0) {
        setMessage({
          type: "error",
          text: t("rules.errorBatchEnable", { action: enabled ? t("common.enabled") : t("common.disabled"), failed: failed.length, total: ids.length })
        });
      } else {
        setMessage({
          type: "info",
          text: t("rules.successBatchEnable", { action: enabled ? t("common.enabled") : t("common.disabled"), count: ids.length, applied: result.applied, failed: result.failed.length })
        });
      }
      setDebugOutput(
        JSON.stringify(
          {
            action: enabled ? "batch_enable" : "batch_disable",
            total: ids.length,
            failed,
            auto_apply: result
          },
          null,
          2
        )
      );
      setSelectedRuleIds(new Set<string>());
    } catch (err) {
      setMessage({ type: "error", text: String(err) });
    }
  }

  async function handleBatchDelete() {
    const ids = [...selectedRuleIds()];
    if (ids.length === 0) {
      setMessage({ type: "error", text: t("rules.errorNoSelection") });
      return;
    }
    const confirmed = window.confirm(t("rules.confirmBatchDelete", { count: ids.length }));
    if (!confirmed) return;

    const failed: string[] = [];
    for (const id of ids) {
      try {
        await deleteRule(id);
        if (editingId() === id) closeFormModal();
      } catch (err) {
        failed.push(`${id}: ${String(err)}`);
      }
    }

    await refreshAll();
    if (failed.length > 0) {
      setMessage({ type: "error", text: t("rules.errorBatchDelete", { failed: failed.length, total: ids.length }) });
    } else {
      setMessage({ type: "info", text: t("rules.successBatchDelete", { count: ids.length }) });
    }
    setDebugOutput(
      JSON.stringify(
        {
          action: "batch_delete",
          total: ids.length,
          failed
        },
        null,
        2
      )
    );
    setSelectedRuleIds(new Set<string>());
  }

  async function runApply() {
    try {
      const result = await applyRules();
      await refreshAll();
      setDebugOutput(JSON.stringify(result, null, 2));
      setMessage({ type: "info", text: t("rules.successApplied", { applied: result.applied, failed: result.failed.length }) });
    } catch (err) {
      setMessage({ type: "error", text: String(err) });
    }
  }

  async function runStop() {
    try {
      const result = await stopRules();
      await refreshAll();
      setDebugOutput(JSON.stringify(result, null, 2));
      setMessage({ type: "info", text: t("rules.successStopped", { stopped: result.stopped }) });
    } catch (err) {
      setMessage({ type: "error", text: String(err) });
    }
  }

  async function loadLogs() {
    try {
      const result = await tailLogs(0);
      setDebugOutput(JSON.stringify(result, null, 2));
    } catch (err) {
      setMessage({ type: "error", text: String(err) });
    }
  }

  return (
    <div class="page">
      <section class="panel main-panel">
        <div class="panel-title">
          <h2>{t("rules.title")}</h2>
          <span class="muted">
            {filteredRows().length} / {rows().length}，{t("rules.selected")} {selectedCount()}
          </span>
        </div>

        <div class="toolbar toolbar-kobalte">
          <KTextField.Root class="kb-text-inline" value={filter.name} onChange={(value) => setFilter("name", value)}>
            <KTextField.Input class="kb-input" placeholder={t("rules.placeholderNameKeyword")} />
          </KTextField.Root>
          <AppSelect
            value={filter.type}
            onChange={(value) => setFilter("type", value)}
            options={filterTypeOptions}
            triggerClass="kb-select-compact"
          />
          <AppSelect
            value={filter.enabled}
            onChange={(value) => setFilter("enabled", value)}
            options={filterEnabledOptions}
            triggerClass="kb-select-compact"
          />
          <KButton.Root class="kb-btn ghost" onClick={() => refreshAll()}>
            {t("rules.btnRefresh")}
          </KButton.Root>
        </div>

        <div class="actions top-actions">
          <KButton.Root class="kb-btn accent" onClick={openCreateModal}>{t("rules.btnNewRule")}</KButton.Root>
          <KButton.Root class="kb-btn ghost" onClick={runApply}>{t("rules.btnApply")}</KButton.Root>
          <KButton.Root class="kb-btn ghost" onClick={runStop}>{t("rules.btnStop")}</KButton.Root>
          <KButton.Root class="kb-btn ghost" onClick={loadLogs}>{t("rules.btnViewLogs")}</KButton.Root>
          <KButton.Root
            class="kb-btn ghost"
            disabled={selectedCount() === 0}
            onClick={() => handleBatchEnable(true)}
          >
            {t("rules.btnBatchEnable")}
          </KButton.Root>
          <KButton.Root
            class="kb-btn ghost"
            disabled={selectedCount() === 0}
            onClick={() => handleBatchEnable(false)}
          >
            {t("rules.btnBatchDisable")}
          </KButton.Root>
          <KButton.Root
            class="kb-btn danger"
            disabled={selectedCount() === 0}
            onClick={handleBatchDelete}
          >
            {t("rules.btnBatchDelete")}
          </KButton.Root>
          <KButton.Root
            class="kb-btn ghost"
            disabled={selectedCount() === 0}
            onClick={() => setSelectedRuleIds(new Set<string>())}
          >
            {t("rules.btnClearSelection")}
          </KButton.Root>
        </div>

        <div class="table-wrap">
          <table class="rules-table">
            <thead>
              <For each={table.getHeaderGroups()}>
                {(group) => (
                  <tr>
                    <For each={group.headers}>
                      {(header) => (
                        <th>
                          <Show when={!header.isPlaceholder}>
                            {flexRender(header.column.columnDef.header, header.getContext())}
                          </Show>
                        </th>
                      )}
                    </For>
                  </tr>
                )}
              </For>
            </thead>
            <tbody>
              <Show
                when={!isTableLoading()}
                fallback={
                  <For each={[1, 2, 3, 4, 5]}>
                    {() => (
                      <tr>
                        <td colspan={10}>
                          <div class="skeleton-line" />
                        </td>
                      </tr>
                    )}
                  </For>
                }
              >
                <Show
                  when={table.getRowModel().rows.length > 0}
                  fallback={
                    <tr>
                      <td colspan={10} class="muted">
                        {t("rules.noData")}
                      </td>
                    </tr>
                  }
                >
                  <For each={table.getRowModel().rows}>
                    {(row) => (
                      <tr class={isRuleSelected(row.original.id) ? "row-selected" : undefined}>
                        <For each={row.getVisibleCells()}>
                          {(cell) => <td>{flexRender(cell.column.columnDef.cell, cell.getContext())}</td>}
                        </For>
                      </tr>
                    )}
                  </For>
                </Show>
              </Show>
            </tbody>
          </table>
        </div>

        <div class="pagination-bar">
          <span class="muted">
            {t("rules.pageInfo", { current: Math.min(pageIndex() + 1, pageCount()), total: pageCount() })}
          </span>
          <AppSelect
            value={String(pageSize())}
            onChange={(value) => {
              setPageSize(Number(value));
              setPageIndex(0);
            }}
            options={getPageSizeOptions(t)}
            triggerClass="kb-select-compact page-size-select"
          />
          <KButton.Root
            class="kb-btn ghost"
            disabled={pageIndex() <= 0}
            onClick={() => setPageIndex((value) => Math.max(0, value - 1))}
          >
            {t("rules.prevPage")}
          </KButton.Root>
          <KButton.Root
            class="kb-btn ghost"
            disabled={pageIndex() >= pageCount() - 1}
            onClick={() => setPageIndex((value) => Math.min(pageCount() - 1, value + 1))}
          >
            {t("rules.nextPage")}
          </KButton.Root>
        </div>

        <Show when={message()}>
          {(msg) => (
            <div class={`hint ${msg().type === "error" ? "error" : "info"}`}>{msg().text}</div>
          )}
        </Show>
      </section>

      <section class="panel">
        <h2>{t("rules.statusTitle")}</h2>
        <div class="status-grid">
          <div>rules: {rulesQuery.data?.length ?? 0}</div>
          <div>runtime: {runtimeQuery.data?.length ?? 0}</div>
          <div>adapters: {topologyQuery.data?.adapters.length ?? 0}</div>
          <Show
            when={isStatusLoading()}
            fallback={
              <Show when={statusError()} fallback={<div>ready</div>}>
                {(err) => <div class="error">{String(err())}</div>}
              </Show>
            }
          >
            <div>loading...</div>
          </Show>
        </div>
      </section>

      <section class="panel">
        <h2>{t("rules.debugOutput")}</h2>
        <pre>{debugOutput()}</pre>
      </section>

      <RuleFormModal
        open={isModalOpen()}
        isEditing={isEditing()}
        form={form}
        setForm={setForm}
        message={message()}
        isProxyType={isProxyType()}
        isSingleNic={isSingleNic()}
        targetPreview={targetPreview()}
        topologyTimestamp={topologyQuery.data?.timestamp ?? null}
        ruleTypeOptions={ruleTypeOptions}
        targetKindOptions={targetKindOptions}
        bindModeOptions={bindModeOptions}
        adapterOptions={adapterOptions()}
        targetRefOptions={targetRefOptions()}
        onOpenChange={handleDialogOpenChange}
        onTargetKindChange={(targetKind) => {
          setForm("target_kind", targetKind);
          setGlobalTargetContext(targetKind, form.target_ref);
        }}
        onTargetRefChange={(value) => {
          setForm("target_ref", value);
          setGlobalTargetContext(form.target_kind, value);
        }}
        onSubmit={submitForm}
        onCancel={closeFormModal}
      />
    </div>
  );
}
