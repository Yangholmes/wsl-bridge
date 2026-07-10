import { invokeBridge } from "../../lib/bridge";
import type { AppSettings, McpServerConfig, McpServerStatus } from "../../lib/types";

export function getAppSettings() {
  return invokeBridge<AppSettings>("get_app_settings");
}

export function updateAppSettings(settings: AppSettings) {
  return invokeBridge<void>("update_app_settings", { settings });
}

export function getMcpServerStatus() {
  return invokeBridge<McpServerStatus>("get_mcp_server_status");
}

export function updateMcpServerConfig(config: McpServerConfig) {
  return invokeBridge<void>("update_mcp_server_config", { config });
}
