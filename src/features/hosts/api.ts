import { invokeBridge } from "../../lib/bridge";
import type {
  CopyHostsGroupRequest,
  CreateHostsGroupRequest,
  ExportHostsGroupRequest,
  HostsEntry,
  HostsEntryInput,
  HostsGroup,
  ImportHostsGroupRequest,
  SaveHostsEntriesRequest,
  UpdateHostsGroupRequest
} from "../../lib/types";

export function bootstrapDefaultHostsGroup() {
  return invokeBridge<HostsGroup>("bootstrap_default_hosts_group");
}

export function listHostsGroups() {
  return invokeBridge<HostsGroup[]>("list_hosts_groups");
}

export function createHostsGroup(req: CreateHostsGroupRequest) {
  return invokeBridge<string>("create_hosts_group", { req });
}

export function updateHostsGroup(id: string, req: UpdateHostsGroupRequest) {
  return invokeBridge<void>("update_hosts_group", { id, req });
}

export function deleteHostsGroup(id: string) {
  return invokeBridge<void>("delete_hosts_group", { id });
}

export function copyHostsGroup(req: CopyHostsGroupRequest) {
  return invokeBridge<string>("copy_hosts_group", { req });
}

export function listHostsEntries(groupId: string) {
  return invokeBridge<HostsEntry[]>("list_hosts_entries", { groupId });
}

export function saveHostsEntries(req: SaveHostsEntriesRequest) {
  return invokeBridge<void>("save_hosts_entries", { req });
}

export function importHostsGroup(req: ImportHostsGroupRequest) {
  return invokeBridge<string>("import_hosts_group", { req });
}

export function previewHostsEntriesFromFile(path: string) {
  return invokeBridge<HostsEntryInput[]>("preview_hosts_entries_from_file", { path });
}

export function exportHostsGroup(req: ExportHostsGroupRequest) {
  return invokeBridge<void>("export_hosts_group", { req });
}

export function activateHostsGroup(groupId: string) {
  return invokeBridge<void>("activate_hosts_group", { groupId });
}
