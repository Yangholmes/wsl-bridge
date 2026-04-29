import Clarity from "@microsoft/clarity";
import { invokeBridge } from "./bridge";

declare const __APP_VERSION__: string;

export function initClarity(projectId: string) {
  Clarity.init(projectId);
}

export async function setupClarityTracking(projectId: string) {
  Clarity.init(projectId);

  try {
    const settings = await invokeBridge<{
      close_behavior: string;
      show_tray_on_start: boolean;
      user_uid: string;
    }>("get_app_settings");

    Clarity.identify(settings.user_uid);

    Clarity.setTag("version", __APP_VERSION__);
    Clarity.setTag("channel", import.meta.env.VITE_APP_CHANNEL || "default");
  } catch (err) {
    console.warn("Failed to setup Clarity tracking:", err);
  }
}