import * as KButton from "@kobalte/core/button";
import * as KDialog from "@kobalte/core/dialog";
import * as KTextField from "@kobalte/core/text-field";
import { queryOptions, useQuery } from "@tanstack/solid-query";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { createEffect, createMemo, createSignal, For, Show } from "solid-js";

import { useI18n } from "../../i18n/context";
import { useAppRuntimeStatusQuery } from "../../lib/appRuntime";
import { Hint } from "../../lib/Hint";
import { useToast } from "../../lib/Toast";
import { MetricCard, PageHeader, SectionCard, StatusBadge } from "../../lib/ui";
import type { HostsEntry, HostsEntryInput, HostsGroup } from "../../lib/types";
import {
  activateHostsGroup,
  bootstrapDefaultHostsGroup,
  copyHostsGroup,
  createHostsGroup,
  deleteHostsGroup,
  exportHostsGroup,
  importHostsGroup,
  listHostsEntries,
  listHostsGroups,
  saveHostsEntries,
  updateHostsGroup
} from "./api";

type DialogMode = "create" | "copy" | "import" | "export" | "delete" | "discard" | null;

function toDraftEntries(entries: HostsEntry[]): HostsEntryInput[] {
  return entries.map((entry) => ({
    id: entry.id,
    ip: entry.ip,
    domain: entry.domain,
    comment: entry.comment,
    enabled: entry.enabled,
    order_index: entry.order_index
  }));
}

function ModalShell(props: {
  open: boolean;
  title: string;
  onOpenChange: (open: boolean) => void;
  children: any;
  actions: any;
}) {
  return (
    <KDialog.Root open={props.open} onOpenChange={props.onOpenChange}>
      <KDialog.Portal>
        <KDialog.Overlay class="kb-dialog-overlay" />
        <KDialog.Content class="kb-dialog-content close-guard-dialog">
          <div class="panel-title">
            <KDialog.Title>{props.title}</KDialog.Title>
          </div>
          <div style={{ display: "grid", gap: "16px" }}>
            {props.children}
            <div class="row-actions" style={{ "justify-content": "flex-end" }}>
              {props.actions}
            </div>
          </div>
        </KDialog.Content>
      </KDialog.Portal>
    </KDialog.Root>
  );
}

export function HostsPage() {
  const { t } = useI18n();
  const toast = useToast();
  const runtimeStatusQuery = useAppRuntimeStatusQuery();
  const [selectedGroupId, setSelectedGroupId] = createSignal("");
  const [pendingGroupId, setPendingGroupId] = createSignal<string | null>(null);
  const [entryDrafts, setEntryDrafts] = createSignal<HostsEntryInput[]>([]);
  const [entryDraftDirty, setEntryDraftDirty] = createSignal(false);
  const [groupNameDraft, setGroupNameDraft] = createSignal("");
  const [groupDescriptionDraft, setGroupDescriptionDraft] = createSignal("");
  const [groupDraftDirty, setGroupDraftDirty] = createSignal(false);
  const [dialogMode, setDialogMode] = createSignal<DialogMode>(null);
  const [createGroupName, setCreateGroupName] = createSignal("");
  const [copyGroupName, setCopyGroupName] = createSignal("");
  const [importPath, setImportPath] = createSignal("");
  const [exportPath, setExportPath] = createSignal("");

  const groupsQuery = useQuery(() =>
    queryOptions<HostsGroup[]>({
      queryKey: ["hosts", "groups"],
      queryFn: async () => {
        await bootstrapDefaultHostsGroup().catch(() => undefined);
        return listHostsGroups();
      },
      staleTime: 0
    })
  );

  createEffect(() => {
    const groups = groupsQuery.data ?? [];
    const selected = selectedGroupId();
    if (groups.length === 0) {
      if (selected) setSelectedGroupId("");
      return;
    }
    if (!selected || !groups.some((item) => item.id === selected)) {
      setSelectedGroupId(groups.find((item) => item.is_active)?.id ?? groups[0].id);
    }
  });

  const entriesQuery = useQuery(() =>
    queryOptions<HostsEntry[]>({
      queryKey: ["hosts", "entries", selectedGroupId()],
      queryFn: () => listHostsEntries(selectedGroupId()),
      enabled: selectedGroupId().length > 0,
      staleTime: 0
    })
  );

  createEffect(() => {
    const groupId = selectedGroupId();
    const entries = entriesQuery.data;
    if (!groupId || !entries) return;
    if (entryDraftDirty()) return;
    setEntryDrafts(toDraftEntries(entries));
  });

  createEffect(() => {
    const group = selectedGroup();
    if (!group || groupDraftDirty()) return;
    setGroupNameDraft(group.name);
    setGroupDescriptionDraft(group.description ?? "");
  });

  const hasAdminPrivileges = createMemo(
    () => runtimeStatusQuery.data?.admin_features_available ?? false
  );
  const selectedGroup = createMemo(
    () => (groupsQuery.data ?? []).find((item) => item.id === selectedGroupId()) ?? null
  );
  const enabledCount = createMemo(
    () => entryDrafts().filter((item) => item.enabled).length
  );
  const invalidEntryRows = createMemo(() =>
    entryDrafts()
      .map((item, index) => ({
        index,
        invalidIp: item.ip.trim().length === 0 || !isValidIp(item.ip),
        invalidDomain: item.domain.trim().length === 0
      }))
      .filter((item) => item.invalidIp || item.invalidDomain)
  );
  const hasEntryValidationError = createMemo(() => invalidEntryRows().length > 0);
  const hasUnsavedChanges = createMemo(() => entryDraftDirty() || groupDraftDirty());

  function closeDialog() {
    setDialogMode(null);
    setPendingGroupId(null);
  }

  function updateDraft(index: number, patch: Partial<HostsEntryInput>) {
    setEntryDrafts((items) =>
      items.map((item, itemIndex) =>
        itemIndex === index ? { ...item, ...patch } : item
      )
    );
    setEntryDraftDirty(true);
  }

  function resetDrafts() {
    setEntryDrafts(toDraftEntries(entriesQuery.data ?? []));
    setEntryDraftDirty(false);
  }

  function resetGroupDraft() {
    const group = selectedGroup();
    setGroupNameDraft(group?.name ?? "");
    setGroupDescriptionDraft(group?.description ?? "");
    setGroupDraftDirty(false);
  }

  function selectGroup(nextGroupId: string) {
    if (nextGroupId === selectedGroupId()) return;
    if (hasUnsavedChanges()) {
      setPendingGroupId(nextGroupId);
      setDialogMode("discard");
      return;
    }
    setEntryDraftDirty(false);
    setGroupDraftDirty(false);
    setSelectedGroupId(nextGroupId);
  }

  async function browseImportPath() {
    const selected = await openDialog({
      multiple: false,
      directory: false,
      filters: [{ name: "Hosts", extensions: ["txt", "hosts"] }]
    });
    if (typeof selected === "string") {
      setImportPath(selected);
    }
  }

  async function browseExportPath() {
    const selected = await saveDialog({
      defaultPath: exportPath() || `${selectedGroup()?.name ?? "hosts"}.hosts`,
      filters: [{ name: "Hosts", extensions: ["txt", "hosts"] }]
    });
    if (typeof selected === "string") {
      setExportPath(selected);
    }
  }

  async function refreshCurrent() {
    await Promise.all([groupsQuery.refetch(), entriesQuery.refetch()]);
  }

  function openCreateDialog() {
    setCreateGroupName("");
    setDialogMode("create");
  }

  function openCopyDialog() {
    setCopyGroupName(`${selectedGroup()?.name ?? "hosts"}-copy`);
    setDialogMode("copy");
  }

  function openImportDialog() {
    setImportPath("");
    setDialogMode("import");
  }

  function openExportDialog() {
    setExportPath(`${selectedGroup()?.name ?? "hosts"}.hosts`);
    setDialogMode("export");
  }

  async function handleCreateGroup() {
    if (!createGroupName().trim()) {
      toast.error(t("hosts.validationGroupName"));
      return;
    }
    try {
      const id = await createHostsGroup({ name: createGroupName().trim() });
      await groupsQuery.refetch();
      setSelectedGroupId(id);
      setEntryDraftDirty(false);
      setGroupDraftDirty(false);
      closeDialog();
      toast.info(t("hosts.groupCreated"));
    } catch (err) {
      toast.error(String(err));
    }
  }

  async function handleCopyGroup() {
    const group = selectedGroup();
    if (!group) return;
    if (!copyGroupName().trim()) {
      toast.error(t("hosts.validationGroupName"));
      return;
    }
    try {
      const id = await copyHostsGroup({
        source_group_id: group.id,
        name: copyGroupName().trim(),
        description: group.description
      });
      await groupsQuery.refetch();
      setSelectedGroupId(id);
      setEntryDraftDirty(false);
      setGroupDraftDirty(false);
      closeDialog();
      toast.info(t("hosts.groupCopied"));
    } catch (err) {
      toast.error(String(err));
    }
  }

  async function handleDeleteGroup() {
    const group = selectedGroup();
    if (!group) return;
    try {
      await deleteHostsGroup(group.id);
      setEntryDraftDirty(false);
      setGroupDraftDirty(false);
      await groupsQuery.refetch();
      await entriesQuery.refetch();
      closeDialog();
      toast.info(t("hosts.groupDeleted"));
    } catch (err) {
      toast.error(String(err));
    }
  }

  async function handleImportGroup() {
    if (!importPath().trim()) {
      toast.error(t("hosts.validationImportPath"));
      return;
    }
    try {
      const id = await importHostsGroup({ path: importPath().trim() });
      await groupsQuery.refetch();
      setSelectedGroupId(id);
      setEntryDraftDirty(false);
      setGroupDraftDirty(false);
      closeDialog();
      toast.info(t("hosts.groupImported"));
    } catch (err) {
      toast.error(String(err));
    }
  }

  async function handleExportGroup() {
    const group = selectedGroup();
    if (!group) return;
    if (!exportPath().trim()) {
      toast.error(t("hosts.validationExportPath"));
      return;
    }
    try {
      await exportHostsGroup({ group_id: group.id, path: exportPath().trim() });
      closeDialog();
      toast.info(t("hosts.groupExported"));
    } catch (err) {
      toast.error(String(err));
    }
  }

  async function handleActivateGroup() {
    const groupId = selectedGroupId();
    if (!groupId) return;
    try {
      if (groupDraftDirty()) {
        await handleSaveGroupMeta();
      }
      if (entryDraftDirty()) {
        await handleSaveEntries();
      }
      await activateHostsGroup(groupId);
      await groupsQuery.refetch();
      toast.info(t("hosts.groupActivated"));
    } catch (err) {
      toast.error(String(err));
    }
  }

  async function handleSaveEntries() {
    const groupId = selectedGroupId();
    if (!groupId) return;
    if (hasEntryValidationError()) {
      toast.error(t("hosts.validationEntriesInvalid"));
      throw new Error("hosts entries invalid");
    }
    await saveHostsEntries({
      group_id: groupId,
      entries: entryDrafts().map((item, index) => ({
        ...item,
        order_index: index
      }))
    });
    setEntryDraftDirty(false);
    await entriesQuery.refetch();
    await groupsQuery.refetch();
    toast.info(t("hosts.entriesSaved"));
  }

  async function handleSaveGroupMeta() {
    const group = selectedGroup();
    if (!group) return;
    if (!groupNameDraft().trim()) {
      toast.error(t("hosts.validationGroupName"));
      throw new Error("hosts group name required");
    }
    await updateHostsGroup(group.id, {
      name: groupNameDraft().trim(),
      description: groupDescriptionDraft().trim() || null
    });
    setGroupDraftDirty(false);
    await groupsQuery.refetch();
    toast.info(t("hosts.groupUpdated"));
  }

  function handleAddRow() {
    setEntryDrafts((items) => [
      ...items,
      {
        ip: "127.0.0.1",
        domain: "",
        comment: "",
        enabled: true,
        order_index: items.length
      }
    ]);
    setEntryDraftDirty(true);
  }

  function handleDeleteRow(index: number) {
    setEntryDrafts((items) =>
      items
        .filter((_, itemIndex) => itemIndex !== index)
        .map((item, itemIndex) => ({ ...item, order_index: itemIndex }))
    );
    setEntryDraftDirty(true);
  }

  function moveDraftRow(index: number, direction: -1 | 1) {
    setEntryDrafts((items) => {
      const targetIndex = index + direction;
      if (targetIndex < 0 || targetIndex >= items.length) {
        return items;
      }
      const next = [...items];
      const [current] = next.splice(index, 1);
      next.splice(targetIndex, 0, current);
      return next.map((item, itemIndex) => ({ ...item, order_index: itemIndex }));
    });
    setEntryDraftDirty(true);
  }

  function setAllEntriesEnabled(enabled: boolean) {
    setEntryDrafts((items) => items.map((item) => ({ ...item, enabled })));
    setEntryDraftDirty(true);
  }

  function discardChangesAndSwitch() {
    const next = pendingGroupId();
    if (next) {
      setEntryDraftDirty(false);
      setGroupDraftDirty(false);
      setSelectedGroupId(next);
    }
    closeDialog();
  }

  return (
    <div class="page">
      <PageHeader
        title={t("hosts.title")}
        actions={
          <>
            <KButton.Root class="kb-btn ghost" onClick={refreshCurrent}>
              {t("common.refresh")}
            </KButton.Root>
            <KButton.Root
              class="kb-btn ghost"
              onClick={openImportDialog}
              disabled={!hasAdminPrivileges()}
            >
              {t("hosts.importGroup")}
            </KButton.Root>
            <KButton.Root
              class="kb-btn accent"
              onClick={openCreateDialog}
              disabled={!hasAdminPrivileges()}
            >
              {t("hosts.newGroup")}
            </KButton.Root>
          </>
        }
      />

      <Show when={!hasAdminPrivileges()}>
        <Hint variant="warn">{t("hosts.adminHint")}</Hint>
      </Show>
      <Hint variant="info">{t("hosts.pathPromptHint")}</Hint>

      <div class="metric-grid">
        <MetricCard
          label={t("hosts.groupsMetric")}
          value={`${groupsQuery.data?.length ?? 0}`}
          detail={t("hosts.groupsMetricDetail")}
        />
        <MetricCard
          label={t("hosts.entriesMetric")}
          value={`${entryDrafts().length}`}
          detail={t("hosts.enabledMetricDetail", { count: enabledCount() })}
        />
        <MetricCard
          label={t("hosts.activeMetric")}
          value={selectedGroup()?.is_active ? t("common.enabled") : t("common.disabled")}
          detail={selectedGroup()?.name ?? t("common.none")}
        />
      </div>

      <div class="topology-grid">
        <SectionCard
          title={t("hosts.groupsTitle")}
          subtitle={t("hosts.groupsSubtitle")}
          actions={
            <div class="row-actions">
              <KButton.Root
                class="kb-btn ghost small"
                onClick={openCopyDialog}
                disabled={!hasAdminPrivileges() || !selectedGroup()}
              >
                {t("hosts.copyGroup")}
              </KButton.Root>
              <KButton.Root
                class="kb-btn ghost small"
                onClick={openExportDialog}
                disabled={!selectedGroup()}
              >
                {t("hosts.exportGroup")}
              </KButton.Root>
              <KButton.Root
                class="kb-btn danger small"
                onClick={() => setDialogMode("delete")}
                disabled={!hasAdminPrivileges() || !selectedGroup()}
              >
                {t("hosts.deleteGroup")}
              </KButton.Root>
              <KButton.Root
                class="kb-btn accent small"
                disabled={!hasAdminPrivileges() || selectedGroup()?.is_active !== false}
                onClick={handleActivateGroup}
              >
                {t("hosts.activate")}
              </KButton.Root>
            </div>
          }
        >
          <Show when={selectedGroup()}>
            <div class="form-grid" style={{ "margin-bottom": "16px" }}>
              <KTextField.Root class="kb-field">
                <KTextField.Label>{t("hosts.groupNameLabel")}</KTextField.Label>
                <KTextField.Input
                  class="kb-input"
                  value={groupNameDraft()}
                  disabled={!hasAdminPrivileges()}
                  onInput={(event) => {
                    setGroupNameDraft(event.currentTarget.value);
                    setGroupDraftDirty(true);
                  }}
                />
              </KTextField.Root>
              <KTextField.Root class="kb-field">
                <KTextField.Label>{t("hosts.groupDescriptionLabel")}</KTextField.Label>
                <KTextField.Input
                  class="kb-input"
                  value={groupDescriptionDraft()}
                  disabled={!hasAdminPrivileges()}
                  onInput={(event) => {
                    setGroupDescriptionDraft(event.currentTarget.value);
                    setGroupDraftDirty(true);
                  }}
                />
              </KTextField.Root>
            </div>
            <div class="row-actions" style={{ "margin-bottom": "16px" }}>
              <KButton.Root
                class="kb-btn ghost small"
                onClick={resetGroupDraft}
                disabled={!groupDraftDirty()}
              >
                {t("hosts.resetGroupMeta")}
              </KButton.Root>
              <KButton.Root
                class="kb-btn accent small"
                onClick={() => {
                  handleSaveGroupMeta().catch((err) => toast.error(String(err)));
                }}
                disabled={!hasAdminPrivileges() || !groupDraftDirty()}
              >
                {t("hosts.saveGroupMeta")}
              </KButton.Root>
            </div>
          </Show>
          <div class="line-list">
            <Show
              when={(groupsQuery.data?.length ?? 0) > 0}
              fallback={<p class="muted">{t("hosts.noGroups")}</p>}
            >
              <For each={groupsQuery.data ?? []}>
                {(group) => (
                  <button
                    type="button"
                    class="line-item"
                    data-active={group.id === selectedGroupId() ? "true" : undefined}
                    onClick={() => selectGroup(group.id)}
                  >
                    <span class="line-item-title">{group.name}</span>
                    <span class="line-item-subtitle">
                      {group.description ?? t("hosts.sourceLabel", { source: group.source_type })}
                    </span>
                    <Show when={group.is_active}>
                      <StatusBadge state="running" label={t("hosts.activeBadge")} />
                    </Show>
                  </button>
                )}
              </For>
            </Show>
          </div>
        </SectionCard>

        <SectionCard
          title={t("hosts.entriesTitle")}
          subtitle={t("hosts.entriesSubtitle")}
          actions={
            <div class="row-actions">
              <KButton.Root
                class="kb-btn ghost small"
                onClick={() => setAllEntriesEnabled(true)}
                disabled={!hasAdminPrivileges() || entryDrafts().length === 0}
              >
                {t("hosts.enableAll")}
              </KButton.Root>
              <KButton.Root
                class="kb-btn ghost small"
                onClick={() => setAllEntriesEnabled(false)}
                disabled={!hasAdminPrivileges() || entryDrafts().length === 0}
              >
                {t("hosts.disableAll")}
              </KButton.Root>
              <KButton.Root
                class="kb-btn ghost small"
                onClick={handleAddRow}
                disabled={!hasAdminPrivileges() || !selectedGroup()}
              >
                {t("hosts.addEntry")}
              </KButton.Root>
              <KButton.Root
                class="kb-btn ghost small"
                onClick={resetDrafts}
                disabled={!entryDraftDirty()}
              >
                {t("hosts.resetEntries")}
              </KButton.Root>
              <KButton.Root
                class="kb-btn accent small"
                onClick={() => {
                  handleSaveEntries().catch((err) => {
                    if (String(err) !== "Error: hosts entries invalid") {
                      toast.error(String(err));
                    }
                  });
                }}
                disabled={!hasAdminPrivileges() || !entryDraftDirty() || !selectedGroup()}
              >
                {t("hosts.saveEntries")}
              </KButton.Root>
            </div>
          }
        >
          <Show when={hasEntryValidationError()}>
            <Hint variant="warn">
              {t("hosts.validationEntriesInvalidDetail", {
                count: invalidEntryRows().length
              })}
            </Hint>
          </Show>
          <div class="table-wrap">
            <table class="rules-table">
              <thead>
                <tr>
                  <th>{t("hosts.tableEnabled")}</th>
                  <th>{t("hosts.tableIp")}</th>
                  <th>{t("hosts.tableDomain")}</th>
                  <th>{t("hosts.tableComment")}</th>
                  <th>{t("hosts.tableActions")}</th>
                </tr>
              </thead>
              <tbody>
                <Show
                  when={selectedGroupId().length > 0}
                  fallback={
                    <tr>
                      <td colspan={5} class="muted">
                        {t("hosts.noGroupSelected")}
                      </td>
                    </tr>
                  }
                >
                  <Show
                    when={entryDrafts().length > 0}
                    fallback={
                      <tr>
                        <td colspan={5} class="muted">
                          {t("hosts.noEntries")}
                        </td>
                      </tr>
                    }
                  >
                    <For each={entryDrafts()}>
                      {(entry, index) => (
                        <tr>
                          <td>
                            <input
                              type="checkbox"
                              checked={entry.enabled}
                              disabled={!hasAdminPrivileges()}
                              onInput={(event) =>
                                updateDraft(index(), { enabled: event.currentTarget.checked })
                              }
                            />
                          </td>
                          <td>
                            <input
                              class="kb-input"
                              data-invalid={
                                invalidEntryRows().some(
                                  (item) => item.index === index() && item.invalidIp
                                )
                                  ? "true"
                                  : undefined
                              }
                              value={entry.ip}
                              disabled={!hasAdminPrivileges()}
                              onInput={(event) =>
                                updateDraft(index(), { ip: event.currentTarget.value })
                              }
                            />
                          </td>
                          <td>
                            <input
                              class="kb-input"
                              data-invalid={
                                invalidEntryRows().some(
                                  (item) => item.index === index() && item.invalidDomain
                                )
                                  ? "true"
                                  : undefined
                              }
                              value={entry.domain}
                              disabled={!hasAdminPrivileges()}
                              onInput={(event) =>
                                updateDraft(index(), { domain: event.currentTarget.value })
                              }
                            />
                          </td>
                          <td>
                            <input
                              class="kb-input"
                              value={entry.comment ?? ""}
                              disabled={!hasAdminPrivileges()}
                              onInput={(event) =>
                                updateDraft(index(), { comment: event.currentTarget.value })
                              }
                            />
                          </td>
                          <td>
                            <div class="row-actions">
                              <KButton.Root
                                class="kb-btn ghost small"
                                disabled={!hasAdminPrivileges() || index() === 0}
                                onClick={() => moveDraftRow(index(), -1)}
                              >
                                {t("hosts.moveUp")}
                              </KButton.Root>
                              <KButton.Root
                                class="kb-btn ghost small"
                                disabled={
                                  !hasAdminPrivileges() || index() === entryDrafts().length - 1
                                }
                                onClick={() => moveDraftRow(index(), 1)}
                              >
                                {t("hosts.moveDown")}
                              </KButton.Root>
                              <KButton.Root
                                class="kb-btn danger small"
                                disabled={!hasAdminPrivileges()}
                                onClick={() => handleDeleteRow(index())}
                              >
                                {t("hosts.deleteEntry")}
                              </KButton.Root>
                            </div>
                          </td>
                        </tr>
                      )}
                    </For>
                  </Show>
                </Show>
              </tbody>
            </table>
          </div>
        </SectionCard>
      </div>

      <ModalShell
        open={dialogMode() === "create"}
        title={t("hosts.newGroup")}
        onOpenChange={(open) => !open && closeDialog()}
        actions={
          <>
            <KButton.Root class="kb-btn ghost" onClick={closeDialog}>
              {t("common.close")}
            </KButton.Root>
            <KButton.Root class="kb-btn accent" onClick={handleCreateGroup}>
              {t("hosts.newGroup")}
            </KButton.Root>
          </>
        }
      >
        <KTextField.Root class="kb-field">
          <KTextField.Label>{t("hosts.groupNameLabel")}</KTextField.Label>
          <KTextField.Input
            class="kb-input"
            value={createGroupName()}
            onInput={(event) => setCreateGroupName(event.currentTarget.value)}
          />
        </KTextField.Root>
      </ModalShell>

      <ModalShell
        open={dialogMode() === "copy"}
        title={t("hosts.copyGroup")}
        onOpenChange={(open) => !open && closeDialog()}
        actions={
          <>
            <KButton.Root class="kb-btn ghost" onClick={closeDialog}>
              {t("common.close")}
            </KButton.Root>
            <KButton.Root class="kb-btn accent" onClick={handleCopyGroup}>
              {t("hosts.copyGroup")}
            </KButton.Root>
          </>
        }
      >
        <KTextField.Root class="kb-field">
          <KTextField.Label>{t("hosts.groupNameLabel")}</KTextField.Label>
          <KTextField.Input
            class="kb-input"
            value={copyGroupName()}
            onInput={(event) => setCopyGroupName(event.currentTarget.value)}
          />
        </KTextField.Root>
      </ModalShell>

      <ModalShell
        open={dialogMode() === "import"}
        title={t("hosts.importGroup")}
        onOpenChange={(open) => !open && closeDialog()}
        actions={
          <>
            <KButton.Root class="kb-btn ghost" onClick={closeDialog}>
              {t("common.close")}
            </KButton.Root>
            <KButton.Root class="kb-btn accent" onClick={handleImportGroup}>
              {t("hosts.importGroup")}
            </KButton.Root>
          </>
        }
      >
        <KTextField.Root class="kb-field">
          <KTextField.Label>{t("hosts.promptImportPath")}</KTextField.Label>
          <div class="row-actions">
            <KTextField.Input
              class="kb-input"
              value={importPath()}
              onInput={(event) => setImportPath(event.currentTarget.value)}
            />
            <KButton.Root class="kb-btn ghost" onClick={browseImportPath}>
              {t("hosts.browse")}
            </KButton.Root>
          </div>
        </KTextField.Root>
      </ModalShell>

      <ModalShell
        open={dialogMode() === "export"}
        title={t("hosts.exportGroup")}
        onOpenChange={(open) => !open && closeDialog()}
        actions={
          <>
            <KButton.Root class="kb-btn ghost" onClick={closeDialog}>
              {t("common.close")}
            </KButton.Root>
            <KButton.Root class="kb-btn accent" onClick={handleExportGroup}>
              {t("hosts.exportGroup")}
            </KButton.Root>
          </>
        }
      >
        <KTextField.Root class="kb-field">
          <KTextField.Label>{t("hosts.promptExportPath")}</KTextField.Label>
          <div class="row-actions">
            <KTextField.Input
              class="kb-input"
              value={exportPath()}
              onInput={(event) => setExportPath(event.currentTarget.value)}
            />
            <KButton.Root class="kb-btn ghost" onClick={browseExportPath}>
              {t("hosts.browse")}
            </KButton.Root>
          </div>
        </KTextField.Root>
      </ModalShell>

      <ModalShell
        open={dialogMode() === "delete"}
        title={t("hosts.deleteGroup")}
        onOpenChange={(open) => !open && closeDialog()}
        actions={
          <>
            <KButton.Root class="kb-btn ghost" onClick={closeDialog}>
              {t("common.close")}
            </KButton.Root>
            <KButton.Root class="kb-btn danger" onClick={handleDeleteGroup}>
              {t("hosts.deleteGroup")}
            </KButton.Root>
          </>
        }
      >
        <p>{t("hosts.confirmDeleteGroup", { name: selectedGroup()?.name ?? "" })}</p>
      </ModalShell>

      <ModalShell
        open={dialogMode() === "discard"}
        title={t("hosts.discardChangesTitle")}
        onOpenChange={(open) => !open && closeDialog()}
        actions={
          <>
            <KButton.Root class="kb-btn ghost" onClick={closeDialog}>
              {t("common.close")}
            </KButton.Root>
            <KButton.Root class="kb-btn danger" onClick={discardChangesAndSwitch}>
              {t("hosts.discardChangesAction")}
            </KButton.Root>
          </>
        }
      >
        <p>{t("hosts.confirmDiscardChanges")}</p>
      </ModalShell>
    </div>
  );
}

function isValidIp(value: string) {
  const trimmed = value.trim();
  if (!trimmed) return false;
  return (
    /^(25[0-5]|2[0-4]\d|1?\d?\d)(\.(25[0-5]|2[0-4]\d|1?\d?\d)){3}$/.test(trimmed) ||
    /^[0-9a-fA-F:]+$/.test(trimmed)
  );
}
