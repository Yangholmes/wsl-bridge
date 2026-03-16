import { createContext, useContext, type Accessor, type JSX, createSignal } from "solid-js";
import { type Component, For } from "solid-js";
import "./Toast.css";

export type ToastType = "info" | "success" | "warn" | "error";

export interface Toast {
  id: number;
  type: ToastType;
  message: string;
  duration?: number;
}

interface ToastContextValue {
  toasts: Accessor<Toast[]>;
  addToast: (type: ToastType, message: string, duration?: number) => void;
  removeToast: (id: number) => void;
  info: (message: string, duration?: number) => void;
  success: (message: string, duration?: number) => void;
  warn: (message: string, duration?: number) => void;
  error: (message: string, duration?: number) => void;
}

const ToastContext = createContext<ToastContextValue>();

let toastId = 0;

export function ToastProvider(props: { children: JSX.Element }) {
  const [toasts, setToasts] = createSignal<Toast[]>([]);

  function addToast(type: ToastType, message: string, duration: number = 5000) {
    const id = ++toastId;
    setToasts((prev) => [...prev, { id, type, message, duration }]);

    if (duration > 0) {
      setTimeout(() => {
        removeToast(id);
      }, duration);
    }
  }

  function removeToast(id: number) {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }

  const value: ToastContextValue = {
    toasts,
    addToast,
    removeToast,
    info: (message, duration) => addToast("info", message, duration),
    success: (message, duration) => addToast("success", message, duration),
    warn: (message, duration) => addToast("warn", message, duration),
    error: (message, duration) => addToast("error", message, duration)
  };

  return (
    <ToastContext.Provider value={value}>
      {props.children}
    </ToastContext.Provider>
  );
}

export function useToast() {
  const context = useContext(ToastContext);
  if (!context) {
    throw new Error("useToast must be used within ToastProvider");
  }
  return context;
}

const ICONS: Record<ToastType, string> = {
  info: "ℹ️",
  success: "✅",
  warn: "⚠️",
  error: "❌"
};

export const ToastContainer: Component = () => {
  const { toasts, removeToast } = useToast();

  return (
    <div class="toast-container">
      <For each={toasts()}>
        {(toast) => (
          <div class={`toast toast-${toast.type}`}>
            <span class="toast-icon">{ICONS[toast.type]}</span>
            <span class="toast-message">{toast.message}</span>
            <button class="toast-close" onClick={() => removeToast(toast.id)}>×</button>
          </div>
        )}
      </For>
    </div>
  );
};