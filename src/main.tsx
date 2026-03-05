import { render } from "solid-js/web";
import { QueryClientProvider } from "@tanstack/solid-query";
import { RouterProvider } from "@tanstack/solid-router";

import { router } from "./router";
import { appQueryClient } from "./lib/queryClient";
import { I18nProvider } from "./i18n/context";
import "./styles.css";

render(
  () => (
    <I18nProvider>
      <QueryClientProvider client={appQueryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </I18nProvider>
  ),
  document.getElementById("app")!
);
