import * as KDialog from "@kobalte/core/dialog";
import type { JSX } from "solid-js";
import { Show } from "solid-js";

export function BottomDrawer(props: {
  open: boolean;
  title: string;
  subtitle?: string;
  onOpenChange: (open: boolean) => void;
  actions?: JSX.Element;
  children: JSX.Element;
}) {
  return (
    <KDialog.Root open={props.open} onOpenChange={props.onOpenChange}>
      <KDialog.Portal>
        <KDialog.Overlay class="bottom-drawer-overlay" />
        <KDialog.Content class="bottom-drawer-content">
          <div class="bottom-drawer-handle" aria-hidden="true" />
          <div class="bottom-drawer-header">
            <div class="bottom-drawer-title-group">
              <KDialog.Title class="bottom-drawer-title">{props.title}</KDialog.Title>
              <Show when={props.subtitle}>
                <KDialog.Description class="bottom-drawer-subtitle">
                  {props.subtitle}
                </KDialog.Description>
              </Show>
            </div>
            <div class="bottom-drawer-actions">
              {props.actions}
            </div>
          </div>
          <div class="bottom-drawer-body">
            {props.children}
          </div>
        </KDialog.Content>
      </KDialog.Portal>
    </KDialog.Root>
  );
}
