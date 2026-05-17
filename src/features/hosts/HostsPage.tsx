import * as KButton from "@kobalte/core/button";
import * as KCheckbox from "@kobalte/core/checkbox";
import * as KDialog from "@kobalte/core/dialog";
import * as KSwitch from "@kobalte/core/switch";
import * as KTextField from "@kobalte/core/text-field";
import { queryOptions, useQuery } from "@tanstack/solid-query";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { createMemo, createSignal, For, Show } from "solid-js";
import { createStore } from "solid-js/store";

import { useI18n } from "../../i18n/context";
import { useAppRuntimeStatusQuery } from "../../lib/appRuntime";
import { EllipsisCell } from "../../lib/EllipsisCell";
import { Hint } from "../../lib/Hint";
import { useToast } from "../../lib/Toast";
import {
  ArrowDownIcon,
  ArrowUpIcon,
  CopyIcon,
  DownloadIcon,
  EditIcon,
  ListEditIcon,
  MetricCard,
  PageHeader,
  PlusIcon,
  RefreshIcon,
  SectionCard,
  TrashIcon,
  UploadIcon
} from "../../lib/ui";
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
  previewHostsEntriesFromFile,
  saveHostsEntries,
  updateHostsGroup
} from "./api";

type GroupDialogState =
  | { mode: "create"; group: null }
  | { mode: "edit"; group: HostsGroup }
  | { mode: "copy"; group: HostsGroup }
  | null;

type RecordsDialogState = {
  group: HostsGroup;
};

type HostsEntryDraft = HostsEntryInput & {
  local_id: string;
};

function toDraftEntries(entries: HostsEntry[]): HostsEntryDraft[] {
  return entries.map((entry) => ({
    local_id: entry.id,
    id: entry.id,
    ip: entry.ip,
    domain: entry.domain,
    comment: entry.comment,
    enabled: entry.enabled,
    order_index: entry.order_index
  }));
}

function toImportedDrafts(entries: HostsEntryInput[], offset: number): HostsEntryDraft[] {
  return entries.map((entry, index) => ({
    ...entry,
    id: null,
    local_id: `new-${Date.now()}-${offset}-${index}`,
    order_index: offset + index
  }));
}

function ModalShell(props: {
  open: boolean;
  title: string;
  contentClass?: string;
  onOpenChange: (open: boolean) => void;
  children: any;
  actions: any;
}) {
  return (
    <KDialog.Root open={props.open} onOpenChange={props.onOpenChange}>
      <KDialog.Portal>
        <KDialog.Overlay class="kb-dialog-overlay" />
        <KDialog.Content class={`kb-dialog-content ${props.contentClass ?? "close-guard-dialog"}`}>
          <div class="panel-title">
            <KDialog.Title>{props.title}</KDialog.Title>
          </div>
          <div class="hosts-dialog-body">
            {props.children}
            <div class="row-actions hosts-dialog-actions">
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
  const [groupDialog, setGroupDialog] = createSignal<GroupDialogState>(null);
  const [deleteGroup, setDeleteGroup] = createSignal<HostsGroup | null>(null);
  const [recordsDialog, setRecordsDialog] = createSignal<RecordsDialogState | null>(null);
  const [groupNameDraft, setGroupNameDraft] = createSignal("");
  const [groupDescriptionDraft, setGroupDescriptionDraft] = createSignal("");
  const [recordsLoading, setRecordsLoading] = createSignal(false);
  const [recordsDirty, setRecordsDirty] = createSignal(false);
  const [entryDrafts, setEntryDrafts] = createStore<HostsEntryDraft[]>([]);

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

  const hasAdminPrivileges = createMemo(
    () => runtimeStatusQuery.data?.admin_features_available ?? false
  );
  const activeGroup = createMemo(
    () => (groupsQuery.data ?? []).find((item) => item.is_active) ?? null
  );
  const invalidEntryRows = createMemo(() =>
    entryDrafts
      .map((item, index) => ({
        index,
        invalidIp: item.ip.trim().length === 0 || !isValidIp(item.ip),
        invalidDomain: item.domain.trim().length === 0
      }))
      .filter((item) => item.invalidIp || item.invalidDomain)
  );
  const hasEntryValidationError = createMemo(() => invalidEntryRows().length > 0);

  async function refreshCurrent() {
    await groupsQuery.refetch();
    if (recordsDialog()) {
      await loadEntriesForGroup(recordsDialog()!.group);
    }
  }

  function openCreateGroupDialog() {
    setGroupNameDraft("");
    setGroupDescriptionDraft("");
    setGroupDialog({ mode: "create", group: null });
  }

  function openEditGroupDialog(group: HostsGroup) {
    setGroupNameDraft(group.name);
    setGroupDescriptionDraft(group.description ?? "");
    setGroupDialog({ mode: "edit", group });
  }

  function openCopyGroupDialog(group: HostsGroup) {
    setGroupNameDraft(`${group.name}-copy`);
    setGroupDescriptionDraft(group.description ?? "");
    setGroupDialog({ mode: "copy", group });
  }

  function closeGroupDialog() {
    setGroupDialog(null);
    setGroupNameDraft("");
    setGroupDescriptionDraft("");
  }

  async function handleSaveGroupDialog() {
    const dialog = groupDialog();
    const name = groupNameDraft().trim();
    const description = groupDescriptionDraft().trim() || null;
    if (!dialog) return;
    if (!name) {
      toast.error(t("hosts.validationGroupName"));
      return;
    }

    try {
      if (dialog.mode === "create") {
        await createHostsGroup({ name, description });
        toast.info(t("hosts.groupCreated"));
      } else if (dialog.mode === "edit") {
        await updateHostsGroup(dialog.group.id, { name, description });
        toast.info(t("hosts.groupUpdated"));
      } else {
        await copyHostsGroup({
          source_group_id: dialog.group.id,
          name,
          description
        });
        toast.info(t("hosts.groupCopied"));
      }
      closeGroupDialog();
      await groupsQuery.refetch();
    } catch (err) {
      toast.error(String(err));
    }
  }

  async function handleImportGroup() {
    const selected = await openDialog({
      multiple: false,
      directory: false,
      filters: [{ name: "Hosts", extensions: ["txt", "hosts"] }]
    });
    if (typeof selected !== "string") return;

    try {
      await importHostsGroup({ path: selected });
      await groupsQuery.refetch();
      toast.info(t("hosts.groupImported"));
    } catch (err) {
      toast.error(String(err));
    }
  }

  async function handleExportGroup(group: HostsGroup) {
    const selected = await saveDialog({
      defaultPath: `${group.name}.hosts`,
      filters: [{ name: "Hosts", extensions: ["txt", "hosts"] }]
    });
    if (typeof selected !== "string") return;

    try {
      await exportHostsGroup({ group_id: group.id, path: selected });
      toast.info(t("hosts.groupExported"));
    } catch (err) {
      toast.error(String(err));
    }
  }

  async function handleActivateGroup(group: HostsGroup) {
    if (group.is_active) return;
    try {
      await activateHostsGroup(group.id);
      await groupsQuery.refetch();
      toast.info(t("hosts.groupActivated"));
    } catch (err) {
      toast.error(String(err));
    }
  }

  async function handleDeleteGroup() {
    const group = deleteGroup();
    if (!group) return;
    try {
      await deleteHostsGroup(group.id);
      setDeleteGroup(null);
      await groupsQuery.refetch();
      toast.info(t("hosts.groupDeleted"));
    } catch (err) {
      toast.error(String(err));
    }
  }

  async function loadEntriesForGroup(group: HostsGroup) {
    setRecordsLoading(true);
    try {
      const entries = await listHostsEntries(group.id);
      setEntryDrafts(toDraftEntries(entries));
      setRecordsDirty(false);
    } catch (err) {
      toast.error(String(err));
    } finally {
      setRecordsLoading(false);
    }
  }

  async function openRecordsDialog(group: HostsGroup) {
    setRecordsDialog({ group });
    await loadEntriesForGroup(group);
  }

  function closeRecordsDialog() {
    setRecordsDialog(null);
    setEntryDrafts([]);
    setRecordsDirty(false);
  }

  function updateDraft(index: number, field: keyof HostsEntryInput, value: string | boolean) {
    setEntryDrafts(index, field, value as never);
    setRecordsDirty(true);
  }

  function handleAddRow() {
    setEntryDrafts(entryDrafts.length, {
      local_id: `new-${Date.now()}-${entryDrafts.length}`,
      id: null,
      ip: "127.0.0.1",
      domain: "",
      comment: "",
      enabled: true,
      order_index: entryDrafts.length
    });
    setRecordsDirty(true);
  }

  function handleDeleteRow(index: number) {
    const next = entryDrafts
      .filter((_, itemIndex) => itemIndex !== index)
      .map((item, itemIndex) => ({ ...item, order_index: itemIndex }));
    setEntryDrafts(next);
    setRecordsDirty(true);
  }

  function moveDraftRow(index: number, direction: -1 | 1) {
    const targetIndex = index + direction;
    if (targetIndex < 0 || targetIndex >= entryDrafts.length) return;
    const next = [...entryDrafts];
    const [current] = next.splice(index, 1);
    next.splice(targetIndex, 0, current);
    setEntryDrafts(next.map((item, itemIndex) => ({ ...item, order_index: itemIndex })));
    setRecordsDirty(true);
  }

  async function handleBatchImportEntries() {
    const selected = await openDialog({
      multiple: false,
      directory: false,
      filters: [{ name: "Hosts", extensions: ["txt", "hosts"] }]
    });
    if (typeof selected !== "string") return;

    try {
      const imported = await previewHostsEntriesFromFile(selected);
      const drafts = toImportedDrafts(imported, entryDrafts.length);
      setEntryDrafts([...entryDrafts, ...drafts]);
      setRecordsDirty(true);
      toast.info(t("hosts.entriesImported", { count: imported.length }));
    } catch (err) {
      toast.error(String(err));
    }
  }

  async function handleSaveEntries() {
    const dialog = recordsDialog();
    if (!dialog) return;
    if (hasEntryValidationError()) {
      toast.error(t("hosts.validationEntriesInvalid"));
      return;
    }

    try {
      await saveHostsEntries({
        group_id: dialog.group.id,
        entries: entryDrafts.map((item, index) => ({
          id: item.id,
          ip: item.ip.trim(),
          domain: item.domain.trim(),
          comment: item.comment?.trim() || null,
          enabled: item.enabled,
          order_index: index
        }))
      });
      if (dialog.group.is_active) {
        await activateHostsGroup(dialog.group.id);
        toast.info(t("hosts.entriesSavedAndApplied"));
      } else {
        toast.info(t("hosts.entriesSaved"));
      }
      closeRecordsDialog();
      await groupsQuery.refetch();
    } catch (err) {
      toast.error(String(err));
    }
  }

  return (
    <div class="page">
      <PageHeader title={t("hosts.title")} />

      <Show when={!hasAdminPrivileges()}>
        <Hint variant="warn">{t("hosts.adminHint")}</Hint>
      </Show>

      <div class="metric-grid">
        <MetricCard
          label={t("hosts.groupsMetric")}
          value={`${groupsQuery.data?.length ?? 0}`}
          detail={t("hosts.groupsMetricDetail")}
        />
        <MetricCard
          label={t("hosts.activeMetric")}
          value={activeGroup()?.name ?? t("common.none")}
          detail={activeGroup() ? t("hosts.activeGroupDetail") : t("hosts.noActiveGroup")}
        />
        <MetricCard
          label={t("hosts.permissionMetric")}
          value={hasAdminPrivileges() ? t("common.admin") : t("common.limitedMode")}
          detail={t("hosts.permissionMetricDetail")}
        />
      </div>

      <SectionCard
        title={t("hosts.groupsTitle")}
        subtitle={t("hosts.groupsSubtitle")}
        actions={
          <div class="row-actions">
            <KButton.Root class="kb-btn ghost small" onClick={refreshCurrent}>
              <RefreshIcon size={15} />
              {t("common.refresh")}
            </KButton.Root>
            <KButton.Root
              class="kb-btn ghost small"
              onClick={handleImportGroup}
              disabled={!hasAdminPrivileges()}
            >
              <UploadIcon size={15} />
              {t("hosts.importGroup")}
            </KButton.Root>
            <KButton.Root
              class="kb-btn accent small"
              onClick={openCreateGroupDialog}
              disabled={!hasAdminPrivileges()}
            >
              <PlusIcon size={15} />
              {t("hosts.newGroup")}
            </KButton.Root>
          </div>
        }
      >
        <div class="table-wrap hosts-groups-table-wrap">
          <table class="rules-table hosts-groups-table">
            <thead>
              <tr>
                <th>{t("hosts.tableActive")}</th>
                <th>{t("hosts.groupNameLabel")}</th>
                <th>{t("hosts.groupDescriptionLabel")}</th>
                <th>{t("hosts.tableUpdatedAt")}</th>
                <th>{t("hosts.tableActions")}</th>
              </tr>
            </thead>
            <tbody>
              <Show
                when={(groupsQuery.data?.length ?? 0) > 0}
                fallback={
                  <tr>
                    <td colspan={5} class="muted">
                      {t("hosts.noGroups")}
                    </td>
                  </tr>
                }
              >
                <For each={groupsQuery.data ?? []}>
                  {(group) => (
                    <tr class={group.is_active ? "row-selected" : undefined}>
                      <td>
                        <KSwitch.Root
                          checked={group.is_active}
                          disabled={!hasAdminPrivileges() || group.is_active}
                          class="kb-switch small row-enable-switch"
                          onChange={(checked) => {
                            if (checked) {
                              handleActivateGroup(group);
                            }
                          }}
                        >
                          <KSwitch.Input aria-label={t("hosts.activateGroupLabel", { name: group.name })} />
                          <KSwitch.Control class="kb-switch-control">
                            <KSwitch.Thumb class="kb-switch-thumb" />
                          </KSwitch.Control>
                        </KSwitch.Root>
                      </td>
                      <td>
                        <EllipsisCell text={group.name} />
                      </td>
                      <td>
                        <EllipsisCell text={group.description || t("common.none")} />
                      </td>
                      <td>{formatDateTime(group.updated_at)}</td>
                      <td>
                        <div class="row-actions hosts-group-actions">
                          <KButton.Root
                            class="kb-btn ghost small icon-btn"
                            onClick={() => openEditGroupDialog(group)}
                            disabled={!hasAdminPrivileges()}
                            title={t("hosts.editGroup")}
                          >
                            <EditIcon size={16} />
                          </KButton.Root>
                          <KButton.Root
                            class="kb-btn ghost small icon-btn"
                            onClick={() => openRecordsDialog(group)}
                            disabled={!hasAdminPrivileges()}
                            title={t("hosts.editRecords")}
                          >
                            <ListEditIcon size={16} />
                          </KButton.Root>
                          <KButton.Root
                            class="kb-btn ghost small icon-btn"
                            onClick={() => openCopyGroupDialog(group)}
                            disabled={!hasAdminPrivileges()}
                            title={t("hosts.copyGroup")}
                          >
                            <CopyIcon size={16} />
                          </KButton.Root>
                          <KButton.Root
                            class="kb-btn ghost small icon-btn"
                            onClick={() => handleExportGroup(group)}
                            title={t("hosts.exportGroup")}
                          >
                            <DownloadIcon size={16} />
                          </KButton.Root>
                          <KButton.Root
                            class="kb-btn danger small icon-btn"
                            onClick={() => setDeleteGroup(group)}
                            disabled={!hasAdminPrivileges() || group.is_active}
                            title={t("hosts.deleteGroup")}
                          >
                            <TrashIcon size={16} />
                          </KButton.Root>
                        </div>
                      </td>
                    </tr>
                  )}
                </For>
              </Show>
            </tbody>
          </table>
        </div>
      </SectionCard>

      <GroupModal
        open={groupDialog() !== null}
        mode={groupDialog()?.mode ?? "create"}
        name={groupNameDraft()}
        description={groupDescriptionDraft()}
        onNameChange={setGroupNameDraft}
        onDescriptionChange={setGroupDescriptionDraft}
        onClose={closeGroupDialog}
        onSave={handleSaveGroupDialog}
      />

      <ModalShell
        open={deleteGroup() !== null}
        title={t("hosts.deleteGroup")}
        onOpenChange={(open) => !open && setDeleteGroup(null)}
        actions={
          <>
            <KButton.Root class="kb-btn ghost" onClick={() => setDeleteGroup(null)}>
              {t("hosts.cancel")}
            </KButton.Root>
            <KButton.Root class="kb-btn danger" onClick={handleDeleteGroup}>
              {t("hosts.deleteGroup")}
            </KButton.Root>
          </>
        }
      >
        <p>{t("hosts.confirmDeleteGroup", { name: deleteGroup()?.name ?? "" })}</p>
      </ModalShell>

      <ModalShell
        open={recordsDialog() !== null}
        title={t("hosts.recordsModalTitle", { name: recordsDialog()?.group.name ?? "" })}
        contentClass="hosts-records-dialog"
        onOpenChange={(open) => !open && closeRecordsDialog()}
        actions={
          <>
            <KButton.Root class="kb-btn ghost" onClick={closeRecordsDialog}>
              {t("hosts.cancel")}
            </KButton.Root>
            <KButton.Root
              class="kb-btn accent"
              onClick={handleSaveEntries}
              disabled={!hasAdminPrivileges() || !recordsDirty() || hasEntryValidationError()}
            >
              {t("hosts.save")}
            </KButton.Root>
          </>
        }
      >
        <div class="hosts-records-toolbar">
          <div>
            <p class="section-card-subtitle">
              {recordsDialog()?.group.is_active
                ? t("hosts.activeRecordsHint")
                : t("hosts.recordsEditHint")}
            </p>
          </div>
          <div class="row-actions">
            <KButton.Root
              class="kb-btn ghost small"
              onClick={handleBatchImportEntries}
              disabled={!hasAdminPrivileges() || recordsLoading()}
            >
              <UploadIcon size={15} />
              {t("hosts.batchImportEntries")}
            </KButton.Root>
            <KButton.Root
              class="kb-btn ghost small"
              onClick={handleAddRow}
              disabled={!hasAdminPrivileges() || recordsLoading()}
            >
              <PlusIcon size={15} />
              {t("hosts.addEntry")}
            </KButton.Root>
          </div>
        </div>
        <Show when={hasEntryValidationError()}>
          <Hint variant="warn">
            {t("hosts.validationEntriesInvalidDetail", {
              count: invalidEntryRows().length
            })}
          </Hint>
        </Show>
        <div class="table-wrap hosts-records-table-wrap">
          <table class="rules-table hosts-records-table">
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
                when={!recordsLoading()}
                fallback={
                  <tr>
                    <td colspan={5} class="muted">
                      {t("common.loading")}
                    </td>
                  </tr>
                }
              >
                <Show
                  when={entryDrafts.length > 0}
                  fallback={
                    <tr>
                      <td colspan={5} class="muted">
                        {t("hosts.noEntries")}
                      </td>
                    </tr>
                  }
                >
                  <For each={entryDrafts}>
                    {(entry, index) => (
                      <tr>
                        <td>
                          <KCheckbox.Root
                            class="kb-checkbox row-check"
                            checked={entry.enabled}
                            disabled={!hasAdminPrivileges()}
                            onChange={(checked) => updateDraft(index(), "enabled", Boolean(checked))}
                          >
                            <KCheckbox.Control class="kb-checkbox-control">
                              <KCheckbox.Indicator class="kb-checkbox-indicator" />
                            </KCheckbox.Control>
                          </KCheckbox.Root>
                        </td>
                        <td>
                          <KTextField.Root class="kb-field">
                            <KTextField.Input
                              class="kb-input"
                              value={entry.ip}
                              data-invalid={
                                invalidEntryRows().some(
                                  (item) => item.index === index() && item.invalidIp
                                )
                                  ? "true"
                                  : undefined
                              }
                              disabled={!hasAdminPrivileges()}
                              onInput={(event) => updateDraft(index(), "ip", event.currentTarget.value)}
                            />
                          </KTextField.Root>
                        </td>
                        <td>
                          <KTextField.Root class="kb-field">
                            <KTextField.Input
                              class="kb-input"
                              value={entry.domain}
                              data-invalid={
                                invalidEntryRows().some(
                                  (item) => item.index === index() && item.invalidDomain
                                )
                                  ? "true"
                                  : undefined
                              }
                              disabled={!hasAdminPrivileges()}
                              onInput={(event) =>
                                updateDraft(index(), "domain", event.currentTarget.value)
                              }
                            />
                          </KTextField.Root>
                        </td>
                        <td>
                          <KTextField.Root class="kb-field">
                            <KTextField.Input
                              class="kb-input"
                              value={entry.comment ?? ""}
                              disabled={!hasAdminPrivileges()}
                              onInput={(event) =>
                                updateDraft(index(), "comment", event.currentTarget.value)
                              }
                            />
                          </KTextField.Root>
                        </td>
                        <td>
                          <div class="row-actions">
                            <KButton.Root
                              class="kb-btn ghost small icon-btn"
                              disabled={!hasAdminPrivileges() || index() === 0}
                              onClick={() => moveDraftRow(index(), -1)}
                              title={t("hosts.moveUp")}
                            >
                              <ArrowUpIcon size={16} />
                            </KButton.Root>
                            <KButton.Root
                              class="kb-btn ghost small icon-btn"
                              disabled={!hasAdminPrivileges() || index() === entryDrafts.length - 1}
                              onClick={() => moveDraftRow(index(), 1)}
                              title={t("hosts.moveDown")}
                            >
                              <ArrowDownIcon size={16} />
                            </KButton.Root>
                            <KButton.Root
                              class="kb-btn danger small icon-btn"
                              disabled={!hasAdminPrivileges()}
                              onClick={() => handleDeleteRow(index())}
                              title={t("hosts.deleteEntry")}
                            >
                              <TrashIcon size={16} />
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
      </ModalShell>
    </div>
  );
}

function GroupModal(props: {
  open: boolean;
  mode: "create" | "edit" | "copy";
  name: string;
  description: string;
  onNameChange: (value: string) => void;
  onDescriptionChange: (value: string) => void;
  onClose: () => void;
  onSave: () => void;
}) {
  const { t } = useI18n();
  const title = createMemo(() => {
    if (props.mode === "edit") return t("hosts.editGroup");
    if (props.mode === "copy") return t("hosts.copyGroup");
    return t("hosts.newGroup");
  });

  return (
    <ModalShell
      open={props.open}
      title={title()}
      onOpenChange={(open) => !open && props.onClose()}
      actions={
        <>
          <KButton.Root class="kb-btn ghost" onClick={props.onClose}>
            {t("hosts.cancel")}
          </KButton.Root>
          <KButton.Root class="kb-btn accent" onClick={props.onSave}>
            {t("hosts.save")}
          </KButton.Root>
        </>
      }
    >
      <div class="form-grid">
        <KTextField.Root class="kb-field">
          <KTextField.Label>{t("hosts.groupNameLabel")}</KTextField.Label>
          <KTextField.Input
            class="kb-input"
            value={props.name}
            onInput={(event) => props.onNameChange(event.currentTarget.value)}
          />
        </KTextField.Root>
        <KTextField.Root class="kb-field">
          <KTextField.Label>{t("hosts.groupDescriptionLabel")}</KTextField.Label>
          <KTextField.Input
            class="kb-input"
            value={props.description}
            onInput={(event) => props.onDescriptionChange(event.currentTarget.value)}
          />
        </KTextField.Root>
      </div>
    </ModalShell>
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

function formatDateTime(value: string) {
  if (!value) return "-";
  return value.replace("T", " ").slice(0, 19);
}
