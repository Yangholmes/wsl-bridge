import { Dialog as KDialog } from "@kobalte/core/dialog";
import type { JSX } from "solid-js";

import "./Modal.css";

type ModalProps = {
  open: boolean;
  title: string;
  onOpenChange: (open: boolean) => void;
  description?: string;
  busy?: boolean;
  closeOnOutside?: boolean;
  closeOnEscape?: boolean;
  contentClass?: string;
  titleActions?: JSX.Element;
  footer?: JSX.Element;
  children?: JSX.Element;
};

export function Modal(props: ModalProps) {
  const contentClass = () =>
    ["kb-dialog-content", props.contentClass].filter(Boolean).join(" ");

  return (
    <KDialog open={props.open} onOpenChange={props.onOpenChange}>
      <KDialog.Portal>
        <KDialog.Overlay class="kb-dialog-overlay" />
        <KDialog.Content
          class={contentClass()}
          data-busy={props.busy ? "true" : "false"}
          onInteractOutside={(event) => {
            if (!props.closeOnOutside) {
              event.preventDefault();
            }
          }}
          onEscapeKeyDown={(event) => {
            if (!props.closeOnEscape) {
              event.preventDefault();
            }
          }}
        >
          <div class="modal-shell">
            <div class="modal-header">
              <div class="panel-title">
                <KDialog.Title>{props.title}</KDialog.Title>
                {props.titleActions ? (
                  <div class="modal-title-actions">{props.titleActions}</div>
                ) : null}
              </div>
              {props.description ? (
                <KDialog.Description class="modal-description">
                  {props.description}
                </KDialog.Description>
              ) : null}
            </div>
            <div class="modal-body">{props.children}</div>
            {props.footer ? <div class="modal-footer">{props.footer}</div> : null}
          </div>
        </KDialog.Content>
      </KDialog.Portal>
    </KDialog>
  );
}
