import { invokeBridge } from "../../lib/bridge";
import type { McpServerConfig, McpServerStatus } from "../../lib/types";

export function getMcpServerStatus() {
  return invokeBridge<McpServerStatus>("get_mcp_server_status");
}

export function updateMcpServerConfig(config: McpServerConfig) {
  return invokeBridge<void>("update_mcp_server_config", { config });
}
