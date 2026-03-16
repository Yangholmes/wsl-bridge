import { render } from "solid-js/web";
import { QueryClientProvider } from "@tanstack/solid-query";
import { RouterProvider } from "@tanstack/solid-router";

import { router } from "./router";
import { appQueryClient } from "./lib/queryClient";
import { I18nProvider } from "./i18n/context";
import { ThemeProvider } from "./lib/theme";
import { ContextMenu, showContextMenu, hideContextMenu, handleKeyDown } from "./lib/ContextMenu";
import { initClarity } from "./lib/clarity";
import "./lib/NumberInput.css";
import "./styles.css";

if (!import.meta.env.DEV) {
  document.addEventListener("contextmenu", showContextMenu);
  document.addEventListener("click", hideContextMenu);
  document.addEventListener("keydown", handleKeyDown);
}

if (!import.meta.env.DEV && import.meta.env.VITE_CLARITY_PROJECT_ID) {
  initClarity(import.meta.env.VITE_CLARITY_PROJECT_ID);
}

render(
  () => (
    <ThemeProvider>
      <I18nProvider>
        <QueryClientProvider client={appQueryClient}>
          <RouterProvider router={router} />
          {!import.meta.env.DEV && <ContextMenu />}
        </QueryClientProvider>
      </I18nProvider>
    </ThemeProvider>
  ),
  document.getElementById("app")!
);
