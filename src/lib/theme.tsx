import { createSignal, createContext, useContext, createEffect, onMount, onCleanup, ParentComponent } from "solid-js";

export type ThemeMode = "light" | "dark" | "auto";

const THEME_STORAGE_KEY = "wsl-bridge.theme";

function getSystemTheme(): "light" | "dark" {
  if (typeof window === "undefined") return "light";
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function getStoredTheme(): ThemeMode | null {
  if (typeof window === "undefined") return null;
  const stored = localStorage.getItem(THEME_STORAGE_KEY);
  if (stored === "light" || stored === "dark" || stored === "auto") {
    return stored;
  }
  return null;
}

function applyTheme(theme: "light" | "dark") {
  document.documentElement.setAttribute("data-theme", theme);
}

type ThemeContextValue = {
  mode: () => ThemeMode;
  setMode: (mode: ThemeMode) => void;
  resolvedTheme: () => "light" | "dark";
};

const ThemeContext = createContext<ThemeContextValue>();

export const ThemeProvider: ParentComponent = (props) => {
  const [mode, setModeSignal] = createSignal<ThemeMode>(getStoredTheme() ?? "auto");
  const [resolvedTheme, setResolvedTheme] = createSignal<"light" | "dark">(
    (mode() === "auto" ? getSystemTheme() : mode()) as "light" | "dark"
  );

  function updateTheme() {
    const currentMode = mode();
    const resolved = currentMode === "auto" ? getSystemTheme() : currentMode;
    setResolvedTheme(resolved as "light" | "dark");
    applyTheme(resolved as "light" | "dark");
  }

  function setMode(newMode: ThemeMode) {
    setModeSignal(newMode);
    localStorage.setItem(THEME_STORAGE_KEY, newMode);
    updateTheme();
  }

  createEffect(() => {
    updateTheme();
  });

  onMount(() => {
    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    const handleChange = () => {
      if (mode() === "auto") {
        updateTheme();
      }
    };
    mediaQuery.addEventListener("change", handleChange);
    onCleanup(() => mediaQuery.removeEventListener("change", handleChange));
  });

  return (
    <ThemeContext.Provider value={{ mode, setMode, resolvedTheme }}>
      {props.children}
    </ThemeContext.Provider>
  );
};

export function useTheme() {
  const context = useContext(ThemeContext);
  if (!context) {
    throw new Error("useTheme must be used within ThemeProvider");
  }
  return context;
}
