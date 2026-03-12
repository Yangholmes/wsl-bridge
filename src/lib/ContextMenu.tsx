import { Component, Show, createSignal } from "solid-js";
import { useI18n } from "../i18n/context";
import "./ContextMenu.css";

interface MenuItem {
  label?: string;
  labelKey?: string;
  action: () => void;
  disabled?: boolean;
}

interface ContextMenuState {
  show: boolean;
  x: number;
  y: number;
  items: MenuItem[];
}

const [menuState, setMenuState] = createSignal<ContextMenuState>({
  show: false,
  x: 0,
  y: 0,
  items: []
});

function isInputElement(el: Element | null): el is HTMLInputElement | HTMLTextAreaElement {
  if (!el) return false;
  const tagName = el.tagName.toLowerCase();
  return tagName === "input" || tagName === "textarea" || el.getAttribute("contenteditable") === "true";
}

function getSelectionText(): string {
  const selection = window.getSelection();
  return selection?.toString() ?? "";
}

function getActiveInput(): HTMLInputElement | HTMLTextAreaElement | null {
  const active = document.activeElement;
  if (!active) return null;
  const tagName = active.tagName.toLowerCase();
  if (tagName === "input" || tagName === "textarea") {
    return active as HTMLInputElement | HTMLTextAreaElement;
  }
  if (active.getAttribute("contenteditable") === "true") {
    return null;
  }
  return null;
}

export function showContextMenu(e: MouseEvent) {
  e.preventDefault();

  const target = e.target as Element;
  const input = getActiveInput();
  const selectedText = getSelectionText();

  let items: MenuItem[] = [];

  if (input && input.selectionStart !== undefined && input.selectionEnd !== undefined) {
    const selStart = input.selectionStart ?? 0;
    const selEnd = input.selectionEnd ?? 0;
    const hasSelection = selStart !== selEnd;
    const selectedInInput = hasSelection ? input.value.slice(selStart, selEnd) : "";

    items = [
      {
        labelKey: "common.copy",
        action: () => {
          if (hasSelection) {
            navigator.clipboard.writeText(selectedInInput);
          }
        },
        disabled: !hasSelection
      },
      {
        labelKey: "common.cut",
        action: () => {
          if (hasSelection) {
            navigator.clipboard.writeText(selectedInInput);
            const start = input.selectionStart ?? 0;
            const end = input.selectionEnd ?? 0;
            input.value = input.value.slice(0, start) + input.value.slice(end);
            input.selectionStart = input.selectionEnd = start;
            input.dispatchEvent(new Event("input", { bubbles: true }));
          }
        },
        disabled: !hasSelection
      },
      {
        labelKey: "common.paste",
        action: async () => {
          const text = await navigator.clipboard.readText();
          const start = input.selectionStart ?? input.value.length;
          const end = input.selectionEnd ?? input.value.length;
          input.value = input.value.slice(0, start) + text + input.value.slice(end);
          input.selectionStart = input.selectionEnd = start + text.length;
          input.dispatchEvent(new Event("input", { bubbles: true }));
        }
      }
    ];
  } else if (selectedText) {
    items = [
      {
        labelKey: "common.copy",
        action: () => {
          navigator.clipboard.writeText(selectedText);
        }
      }
    ];
  }

  if (items.length === 0) {
    setMenuState((prev) => ({ ...prev, show: false }));
    return;
  }

  setMenuState({
    show: true,
    x: e.clientX,
    y: e.clientY,
    items
  });
}

export function hideContextMenu() {
  setMenuState((prev) => ({ ...prev, show: false }));
}

export function handleKeyDown(e: KeyboardEvent) {
  if (!e.ctrlKey && !e.metaKey) return;

  const key = e.key.toLowerCase();
  if (key !== "c" && key !== "v" && key !== "x") return;

  const input = getActiveInput();
  if (!input) return;

  e.preventDefault();

  const selStart = input.selectionStart ?? 0;
  const selEnd = input.selectionEnd ?? 0;
  const hasSelection = selStart !== selEnd;

  if (key === "c") {
    if (hasSelection) {
      const selected = input.value.slice(selStart, selEnd);
      navigator.clipboard.writeText(selected);
    }
  } else if (key === "x") {
    if (hasSelection) {
      const selected = input.value.slice(selStart, selEnd);
      navigator.clipboard.writeText(selected);
      input.value = input.value.slice(0, selStart) + input.value.slice(selEnd);
      input.selectionStart = input.selectionEnd = selStart;
      input.dispatchEvent(new Event("input", { bubbles: true }));
    }
  } else if (key === "v") {
    navigator.clipboard.readText().then((text) => {
      const start = input.selectionStart ?? input.value.length;
      const end = input.selectionEnd ?? input.value.length;
      input.value = input.value.slice(0, start) + text + input.value.slice(end);
      input.selectionStart = input.selectionEnd = start + text.length;
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
  }
}

export const ContextMenu: Component = () => {
  const state = () => menuState();
  const { t } = useI18n();

  return (
    <Show when={state().show}>
      <div
        class="context-menu"
        style={{
          left: `${state().x}px`,
          top: `${state().y}px`
        }}
        onClick={hideContextMenu}
      >
        {state().items.map((item) => (
          <div
            class={`context-menu-item ${item.disabled ? "disabled" : ""}`}
            onClick={(e) => {
              e.stopPropagation();
              if (!item.disabled) {
                item.action();
                hideContextMenu();
              }
            }}
          >
            {item.labelKey ? t(item.labelKey) : item.label}
          </div>
        ))}
      </div>
    </Show>
  );
};
