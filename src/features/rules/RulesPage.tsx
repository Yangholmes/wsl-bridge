import {
  createEffect,
  createMemo,
  createSignal,
  For,
  Match,
  Show,
  Switch
} from "solid-js";
import { createStore } from "solid-js/store";
import { createQuery } from "@tanstack/solid-query";
import {
  createSolidTable,
  flexRender,
  getCoreRowModel,
  type ColumnDef
} from "@tanstack/solid-table";

import {
  applyRules,
  createRule,
  deleteRule,
  enableRule,
  getRuntimeStatus,
  listRules,
  scanTopology,
  stopRules,
  tailLogs,
  updateRule
} from "./api";
import type { BindMode, CreateRuleRequest, ProxyRule, RuleType, RuntimeState, RulePatch } from "../../lib/types";

type RuleRow = ProxyRule & {
  runtime_state: RuntimeState | "unknown";
  last_error: string | null;
  last_apply_at: string | null;
};

type FormState = {
  name: string;
  type: RuleType;
  listen_host: string;
  listen_port: string;
  target_kind: "static" | "wsl" | "hyperv";
  target_ref: string;
  target_host: string;
  target_port: string;
  bind_mode: BindMode;
  nic_id: string;
  enabled: boolean;
  fw_domain: boolean;
  fw_private: boolean;
  fw_public: boolean;
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

function toLocalTime(value: string | null) {
  if (!value) return "-";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

export function RulesPage() {
  const [form, setForm] = createStore<FormState>({ ...defaultForm });
  const [editingId, setEditingId] = createSignal<string | null>(null);
  const [message, setMessage] = createSignal<{ type: "info" | "error"; text: string } | null>(
    null
  );
  const [debugOutput, setDebugOutput] = createSignal("ready");

  const [filter, setFilter] = createStore({
    name: "",
    type: "all",
    enabled: "all"
  });

  const rulesQuery = createQuery(() => ({
    queryKey: ["rules"],
    queryFn: listRules
  }));
  const runtimeQuery = createQuery(() => ({
    queryKey: ["runtime"],
    queryFn: getRuntimeStatus
  }));
  const topologyQuery = createQuery(() => ({
    queryKey: ["topology"],
    queryFn: scanTopology
  }));

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
      const runtime = runtimeMap().get(rule.id);
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

  const adapterOptions = createMemo(() => topologyQuery.data?.adapters ?? []);
  const isProxyType = createMemo(() => form.type === "http_proxy" || form.type === "socks5_proxy");
  const isSingleNic = createMemo(() => form.bind_mode === "single_nic");
  const isEditing = createMemo(() => editingId() !== null);

  createEffect(() => {
    if (isProxyType() && form.target_kind !== "static") {
      setForm("target_kind", "static");
    }
  });

  createEffect(() => {
    if (!isSingleNic() && form.nic_id) {
      setForm("nic_id", "");
    }
  });

  const columns: ColumnDef<RuleRow>[] = [
    { header: "名称", accessorKey: "name" },
    { header: "类型", accessorKey: "type" },
    {
      header: "监听",
      cell: (ctx) => `${ctx.row.original.listen_host}:${ctx.row.original.listen_port}`
    },
    {
      header: "目标",
      cell: (ctx) => {
        const row = ctx.row.original;
        return `${row.target_kind}:${row.target_ref ?? row.target_host ?? "-"}:${row.target_port ?? "-"}`;
      }
    },
    {
      header: "启用",
      cell: (ctx) => (ctx.row.original.enabled ? "true" : "false")
    },
    {
      header: "运行态",
      accessorKey: "runtime_state"
    },
    {
      header: "最近应用",
      cell: (ctx) => toLocalTime(ctx.row.original.last_apply_at)
    },
    {
      header: "错误",
      cell: (ctx) => ctx.row.original.last_error ?? "-"
    },
    {
      header: "操作",
      cell: (ctx) => {
        const row = ctx.row.original;
        return (
          <div class="row-actions">
            <button onClick={() => handleEdit(row)}>编辑</button>
            <button onClick={() => handleToggle(row.id, !row.enabled)}>
              {row.enabled ? "禁用" : "启用"}
            </button>
            <button class="secondary" onClick={() => handleDelete(row.id)}>
              删除
            </button>
          </div>
        );
      }
    }
  ];

  const table = createSolidTable({
    get data() {
      return filteredRows();
    },
    columns,
    getCoreRowModel: getCoreRowModel()
  });

  function resetForm() {
    setEditingId(null);
    setForm({ ...defaultForm });
  }

  function handleEdit(rule: RuleRow) {
    setEditingId(rule.id);
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
    setMessage({
      type: "info",
      text: "编辑模式：当前后端 patch 不支持修改 type/target_kind/firewall。"
    });
  }

  async function refreshAll() {
    await Promise.all([
      rulesQuery.refetch(),
      runtimeQuery.refetch(),
      topologyQuery.refetch()
    ]);
  }

  function validateForm(excludeId: string | null) {
    if (!form.name.trim()) return "名称不能为空。";
    if (!form.listen_host.trim()) return "监听地址不能为空。";
    const listenPort = Number(form.listen_port);
    if (!Number.isInteger(listenPort) || listenPort < 1 || listenPort > 65535) {
      return "监听端口必须是 1-65535 的整数。";
    }
    if (isSingleNic() && !form.nic_id) return "single_nic 模式必须选择网卡。";
    if (!form.fw_domain && !form.fw_private && !form.fw_public) {
      return "至少启用一个防火墙 Profile。";
    }

    if (!isProxyType()) {
      const targetPort = Number(form.target_port);
      if (!Number.isInteger(targetPort) || targetPort < 1 || targetPort > 65535) {
        return "目标端口必须是 1-65535 的整数。";
      }
      if (form.target_kind === "static" && !form.target_host.trim()) {
        return "static 目标必须填写 target_host。";
      }
      if ((form.target_kind === "wsl" || form.target_kind === "hyperv") && !form.target_ref.trim()) {
        return `${form.target_kind} 目标必须填写 target_ref。`;
      }
      if (form.target_kind !== "static") {
        return "WSL/Hyper-V 动态解析在 M2 实现，M1 请先使用 static 目标。";
      }
    }

    const conflict = rows().find(
      (r) =>
        r.id !== excludeId &&
        r.listen_host === form.listen_host.trim() &&
        r.listen_port === Number(form.listen_port)
    );
    if (conflict) {
      return `监听冲突：${conflict.listen_host}:${conflict.listen_port} 已被 ${conflict.name} 占用。`;
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
        setDebugOutput(JSON.stringify({ updated_rule_id: editingId(), patch }, null, 2));
        setMessage({ type: "info", text: `规则更新成功，ID=${editingId()}` });
      } else {
        const req = toCreateRequest();
        const id = await createRule(req);
        setDebugOutput(JSON.stringify({ created_rule_id: id, request: req }, null, 2));
        setMessage({ type: "info", text: `规则创建成功，ID=${id}` });
      }

      resetForm();
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
      if (editingId() === id) resetForm();
      await refreshAll();
      setMessage({ type: "info", text: `已删除规则 ${id}` });
      setDebugOutput(JSON.stringify({ deleted_rule_id: id }, null, 2));
    } catch (err) {
      setMessage({ type: "error", text: String(err) });
    }
  }

  async function handleToggle(id: string, enabled: boolean) {
    try {
      await enableRule(id, enabled);
      await refreshAll();
      setMessage({ type: "info", text: `规则 ${id} 已${enabled ? "启用" : "禁用"}` });
      setDebugOutput(JSON.stringify({ toggled_rule_id: id, enabled }, null, 2));
    } catch (err) {
      setMessage({ type: "error", text: String(err) });
    }
  }

  async function runApply() {
    try {
      const result = await applyRules();
      await refreshAll();
      setDebugOutput(JSON.stringify(result, null, 2));
      setMessage({ type: "info", text: `已应用规则，applied=${result.applied}, failed=${result.failed.length}` });
    } catch (err) {
      setMessage({ type: "error", text: String(err) });
    }
  }

  async function runStop() {
    try {
      const result = await stopRules();
      await refreshAll();
      setDebugOutput(JSON.stringify(result, null, 2));
      setMessage({ type: "info", text: `已停止规则，stopped=${result.stopped}` });
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
      <section class="panel">
        <div class="panel-title">
          <h2>Rules</h2>
          <span class="muted">
            {filteredRows().length} / {rows().length}
          </span>
        </div>

        <div class="toolbar">
          <input
            placeholder="名称关键词"
            value={filter.name}
            onInput={(e) => setFilter("name", e.currentTarget.value)}
          />
          <select value={filter.type} onInput={(e) => setFilter("type", e.currentTarget.value)}>
            <option value="all">all</option>
            <option value="tcp_fwd">tcp_fwd</option>
            <option value="udp_fwd">udp_fwd</option>
            <option value="http_proxy">http_proxy</option>
            <option value="socks5_proxy">socks5_proxy</option>
          </select>
          <select value={filter.enabled} onInput={(e) => setFilter("enabled", e.currentTarget.value)}>
            <option value="all">all</option>
            <option value="enabled">enabled</option>
            <option value="disabled">disabled</option>
          </select>
          <button class="secondary" onClick={() => refreshAll()}>
            刷新
          </button>
        </div>

        <div class="table-wrap">
          <table>
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
                when={table.getRowModel().rows.length > 0}
                fallback={
                  <tr>
                    <td colspan={9} class="muted">
                      暂无数据
                    </td>
                  </tr>
                }
              >
                <For each={table.getRowModel().rows}>
                  {(row) => (
                    <tr>
                      <For each={row.getVisibleCells()}>
                        {(cell) => <td>{flexRender(cell.column.columnDef.cell, cell.getContext())}</td>}
                      </For>
                    </tr>
                  )}
                </For>
              </Show>
            </tbody>
          </table>
        </div>
      </section>

      <section class="panel">
        <div class="panel-title">
          <h2>{isEditing() ? "编辑规则" : "新建规则"}</h2>
          <Show when={isEditing()}>
            <button class="secondary" onClick={resetForm}>
              取消编辑
            </button>
          </Show>
        </div>

        <div class="form-grid">
          <label>
            名称
            <input value={form.name} onInput={(e) => setForm("name", e.currentTarget.value)} />
          </label>
          <label>
            类型
            <select
              value={form.type}
              disabled={isEditing()}
              onInput={(e) => setForm("type", e.currentTarget.value as RuleType)}
            >
              <option value="tcp_fwd">tcp_fwd</option>
              <option value="udp_fwd">udp_fwd</option>
              <option value="http_proxy">http_proxy</option>
              <option value="socks5_proxy">socks5_proxy</option>
            </select>
          </label>
          <label>
            监听地址
            <input
              value={form.listen_host}
              onInput={(e) => setForm("listen_host", e.currentTarget.value)}
            />
          </label>
          <label>
            监听端口
            <input
              type="number"
              min="1"
              max="65535"
              value={form.listen_port}
              onInput={(e) => setForm("listen_port", e.currentTarget.value)}
            />
          </label>
          <label>
            目标类型
            <select
              value={form.target_kind}
              disabled={isEditing() || isProxyType()}
              onInput={(e) => setForm("target_kind", e.currentTarget.value as FormState["target_kind"])}
            >
              <option value="static">static</option>
              <option value="wsl">wsl</option>
              <option value="hyperv">hyperv</option>
            </select>
          </label>
          <label>
            目标引用
            <input
              value={form.target_ref}
              disabled={isProxyType() || form.target_kind === "static"}
              onInput={(e) => setForm("target_ref", e.currentTarget.value)}
            />
          </label>
          <label>
            目标主机
            <input
              value={form.target_host}
              disabled={isProxyType() || form.target_kind !== "static"}
              onInput={(e) => setForm("target_host", e.currentTarget.value)}
            />
          </label>
          <label>
            目标端口
            <input
              type="number"
              min="1"
              max="65535"
              value={form.target_port}
              disabled={isProxyType()}
              onInput={(e) => setForm("target_port", e.currentTarget.value)}
            />
          </label>
          <label>
            绑定模式
            <select
              value={form.bind_mode}
              onInput={(e) => setForm("bind_mode", e.currentTarget.value as BindMode)}
            >
              <option value="all_nics">all_nics</option>
              <option value="single_nic">single_nic</option>
            </select>
          </label>
          <label>
            网卡
            <select
              value={form.nic_id}
              disabled={!isSingleNic()}
              onInput={(e) => setForm("nic_id", e.currentTarget.value)}
            >
              <option value="">请选择网卡</option>
              <For each={adapterOptions()}>
                {(item) => <option value={item.id}>{item.name} ({item.id})</option>}
              </For>
            </select>
          </label>
          <label>
            启用
            <select
              value={String(form.enabled)}
              onInput={(e) => setForm("enabled", e.currentTarget.value === "true")}
            >
              <option value="true">true</option>
              <option value="false">false</option>
            </select>
          </label>
        </div>

        <div class="checks">
          <label>
            <input
              type="checkbox"
              checked={form.fw_domain}
              disabled={isEditing()}
              onInput={(e) => setForm("fw_domain", e.currentTarget.checked)}
            />
            Domain
          </label>
          <label>
            <input
              type="checkbox"
              checked={form.fw_private}
              disabled={isEditing()}
              onInput={(e) => setForm("fw_private", e.currentTarget.checked)}
            />
            Private
          </label>
          <label>
            <input
              type="checkbox"
              checked={form.fw_public}
              disabled={isEditing()}
              onInput={(e) => setForm("fw_public", e.currentTarget.checked)}
            />
            Public
          </label>
        </div>

        <div class="actions">
          <button onClick={submitForm}>{isEditing() ? "保存修改" : "创建规则"}</button>
          <button class="secondary" onClick={runApply}>
            应用规则
          </button>
          <button class="secondary" onClick={runStop}>
            停止规则
          </button>
          <button class="secondary" onClick={loadLogs}>
            查看日志
          </button>
        </div>

        <Show when={message()}>
          {(msg) => (
            <div class={`hint ${msg().type === "error" ? "error" : "info"}`}>{msg().text}</div>
          )}
        </Show>
      </section>

      <section class="panel">
        <h2>Debug Output</h2>
        <pre>{debugOutput()}</pre>
      </section>

      <section class="panel">
        <h2>状态</h2>
        <div class="status-grid">
          <div>rules: {rulesQuery.data?.length ?? 0}</div>
          <div>runtime: {runtimeQuery.data?.length ?? 0}</div>
          <div>adapters: {topologyQuery.data?.adapters.length ?? 0}</div>
          <Switch>
            <Match when={rulesQuery.isPending || runtimeQuery.isPending || topologyQuery.isPending}>
              <div>loading...</div>
            </Match>
            <Match when={rulesQuery.error || runtimeQuery.error || topologyQuery.error}>
              <div class="error">
                {(rulesQuery.error ?? runtimeQuery.error ?? topologyQuery.error)?.toString()}
              </div>
            </Match>
            <Match when={true}>
              <div>ready</div>
            </Match>
          </Switch>
        </div>
      </section>
    </div>
  );
}

