import { type ParentComponent, Show, createEffect, onCleanup } from "solid-js";

import "./DropdownPanel.css";

type DropdownPanelProps = {
  actionLabel: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  class?: string;
  panelClass?: string;
  align?: "left" | "right";
};

export const DropdownPanel: ParentComponent<DropdownPanelProps> = (props) => {
  let rootRef: HTMLDivElement | undefined;

  createEffect(() => {
    if (!props.open) {
      return;
    }

    const handlePointerDown = (event: PointerEvent) => {
      if (!rootRef?.contains(event.target as Node)) {
        props.onOpenChange(false);
      }
    };

    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        props.onOpenChange(false);
      }
    };

    window.addEventListener("pointerdown", handlePointerDown);
    window.addEventListener("keydown", handleEscape);

    onCleanup(() => {
      window.removeEventListener("pointerdown", handlePointerDown);
      window.removeEventListener("keydown", handleEscape);
    });
  });

  return (
    <div ref={rootRef} class={`dropdown-panel ${props.class ?? ""}`.trim()}>
      <button
        type="button"
        class={`kb-btn ghost small dropdown-panel-trigger ${props.open ? "open" : ""}`}
        onClick={() => props.onOpenChange(!props.open)}
      >
        {props.actionLabel}
      </button>
      <Show when={props.open}>
        <div
          class={`dropdown-panel-surface ${props.align === "left" ? "align-left" : "align-right"} ${props.panelClass ?? ""}`.trim()}
        >
          {props.children}
        </div>
      </Show>
    </div>
  );
};
