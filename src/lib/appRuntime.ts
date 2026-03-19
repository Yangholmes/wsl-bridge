import { queryOptions, useQuery } from "@tanstack/solid-query";
import type { AppRuntimeStatus } from "./types";
import { appQueryClient } from "./queryClient";
import { invokeBridge } from "./bridge";

export function getAppRuntimeStatus() {
  return invokeBridge<AppRuntimeStatus>("get_app_runtime_status");
}

export function useAppRuntimeStatusQuery() {
  return useQuery(
    () =>
      queryOptions<AppRuntimeStatus>({
        queryKey: ["app-runtime-status"],
        queryFn: getAppRuntimeStatus,
        staleTime: Infinity,
        gcTime: Infinity,
        refetchOnWindowFocus: false
      }),
    () => appQueryClient
  );
}
