import { invoke, isTauri } from "@tauri-apps/api/core";

export async function invokeBridge<T>(
  command: string,
  payload: Record<string, unknown> = {}
): Promise<T> {
  if (!isTauri()) {
    throw new Error("Tauri runtime is required. Use `pnpm tauri dev` or packaged .exe.");
  }
  return invoke<T>(command, payload);
}

