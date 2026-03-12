import { render } from "solid-js/web";
import { QueryClientProvider } from "@tanstack/solid-query";
import { RouterProvider } from "@tanstack/solid-router";

import { router } from "./router";
import { appQueryClient } from "./lib/queryClient";
import { I18nProvider } from "./i18n/context";
import { ThemeProvider } from "./lib/theme";
import { ContextMenu, showContextMenu, hideContextMenu, handleKeyDown } from "./lib/ContextMenu";
import "./styles.css";

if (!import.meta.env.DEV) {
  document.addEventListener("contextmenu", showContextMenu);
  document.addEventListener("click", hideContextMenu);
  document.addEventListener("keydown", handleKeyDown);
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
