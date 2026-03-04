import { createSignal } from "solid-js";

import { scanTopology } from "../rules/api";
import type { TargetKind } from "../../lib/types";

const [globalTargetKind, setGlobalTargetKind] = createSignal<TargetKind>("static");
const [globalTargetRef, setGlobalTargetRef] = createSignal("");

export function getGlobalTargetKind() {
  return globalTargetKind();
}

export function getGlobalTargetRef() {
  return globalTargetRef();
}

export function setGlobalTargetContext(kind: TargetKind, ref: string) {
  setGlobalTargetKind(kind);
  setGlobalTargetRef(kind === "static" ? "" : ref.trim());
}

export function createTopologyQueryOptions(enabled: boolean) {
  return {
    queryKey: ["topology"] as const,
    queryFn: scanTopology,
    enabled,
    staleTime: 60_000,
    gcTime: 10 * 60_000,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
    refetchOnMount: false
  };
}
