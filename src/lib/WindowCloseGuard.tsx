import { createSignal, onCleanup, onMount, Show } from "solid-js";
import * as KButton from "@kobalte/core/button";
import * as KDialog from "@kobalte/core/dialog";
import { isTauri } from "@tauri-apps/api/core";

import { useI18n } from "../i18n/context";
import type { AppSettings } from "./types";
import { invokeBridge } from "./bridge";

async function getAppSettings() {
  return invokeBridge<AppSettings>("get_app_settings");
}

async function setTrayVisibility(visible: boolean) {
  return invokeBridge<void>("set_tray_visibility", { visible });
}

async function hideMainWindowToTray() {
  return invokeBridge<void>("hide_main_window_to_tray");
}

async function exitApplication() {
  return invokeBridge<void>("exit_application");
}

export function WindowCloseGuard() {
  const { t } = useI18n();
  const [dialogOpen, setDialogOpen] = createSignal(false);

  let allowClose = false;
  let unlisten: (() => void) | undefined;

  async function hideToTray() {
    await hideMainWindowToTray();
  }

  async function handleCloseIntent() {
    const settings = await getAppSettings();
    if (settings.close_behavior === "minimize") {
      await hideToTray();
      return;
    }
    if (settings.close_behavior === "exit") {
      allowClose = true;
      await exitApplication();
      return;
    }
    setDialogOpen(true);
  }

  onMount(async () => {
    if (!isTauri()) return;
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    unlisten = await getCurrentWindow().onCloseRequested(async (event) => {
      if (allowClose) return;
      event.preventDefault();
      await handleCloseIntent();
    });
  });

  onCleanup(() => {
    unlisten?.();
  });

  return (
    <Show when={dialogOpen()}>
      <KDialog.Root open={dialogOpen()} onOpenChange={setDialogOpen}>
        <KDialog.Portal>
          <KDialog.Overlay class="kb-dialog-overlay" />
          <KDialog.Content class="kb-dialog-content close-guard-dialog">
            <div class="panel-title">
              <KDialog.Title>{t("app.closeConfirmTitle")}</KDialog.Title>
            </div>
            <KDialog.Description class="muted">
              {t("app.closeConfirmBody")}
            </KDialog.Description>

            <div class="actions modal-actions close-guard-actions">
              <KButton.Root
                class="kb-btn accent"
                onClick={() => {
                  setDialogOpen(false);
                  void hideToTray();
                }}
              >
                {t("app.closeActionMinimize")}
              </KButton.Root>
              <KButton.Root
                class="kb-btn danger"
                onClick={() => {
                  setDialogOpen(false);
                  allowClose = true;
                  void exitApplication();
                }}
              >
                {t("app.closeActionExit")}
              </KButton.Root>
              <KButton.Root class="kb-btn ghost" onClick={() => setDialogOpen(false)}>
                {t("rules.formCancel")}
              </KButton.Root>
            </div>
          </KDialog.Content>
        </KDialog.Portal>
      </KDialog.Root>
    </Show>
  );
}
