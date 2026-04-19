import { lazy, Suspense, createMemo, Show, For } from "solid-js";
import { createRootRoute, createRoute, createRouter, Link, Outlet, useRouterState } from "@tanstack/solid-router";
import { useI18n } from "./i18n/context";
import IconBridge from "./assets/bridge-logo.svg?url";
import { ErrorPage } from "./features/ErrorPage";
import { SkeletonTitle, SkeletonLine } from "./lib/Skeleton";
import { AppRuntimeBanner } from "./lib/AppRuntimeBanner";
import { useAppRuntimeStatusQuery } from "./lib/appRuntime";
import { useTheme } from "./lib/theme";
import { toLocalTime } from "./lib/datetime";
import * as KButton from "@kobalte/core/button";
import "./lib/Layout.css";
import "./lib/Table.css";
import "./lib/Form.css";
import "./lib/Button.css";
import "./lib/Toggle.css";
import "./lib/Modal.css";
import "./lib/Skeleton.css";
import "./lib/Status.css";

const DashboardPage = lazy(() =>
  import("./features/dashboard/DashboardPage").then((module) => ({ default: module.DashboardPage }))
);
const RulesPage = lazy(() =>
  import("./features/rules/RulesPage").then((module) => ({ default: module.RulesPage }))
);
const RuntimePage = lazy(() =>
  import("./features/runtime/RuntimePage").then((module) => ({ default: module.RuntimePage }))
);
const TopologyPage = lazy(() =>
  import("./features/topology/TopologyPage").then((module) => ({ default: module.TopologyPage }))
);
const SettingsPage = lazy(() =>
  import("./features/settings/SettingsPage").then((module) => ({ default: module.SettingsPage }))
);

function PageLoadingFallback() {
  return (
    <section class="panel">
      <SkeletonTitle />
      <SkeletonLine wide count={3} />
    </section>
  );
}

function withSuspense(component: () => any) {
  return () => <Suspense fallback={<PageLoadingFallback />}>{component()}</Suspense>;
}

function RootLayout() {
  const { t } = useI18n();
  const routerState = useRouterState();
  const runtimeStatusQuery = useAppRuntimeStatusQuery();
  const theme = useTheme();

  const currentPath = createMemo(() => routerState().location.pathname);
  const hasBanner = createMemo(() => !runtimeStatusQuery.data?.admin_features_available && runtimeStatusQuery.data);

  const navItems = createMemo(() => [
    { path: "/dashboard", label: t("nav.dashboard") },
    { path: "/rules", label: t("nav.rules") },
    { path: "/runtime", label: t("nav.runtime") },
    { path: "/topology", label: t("nav.topology") },
    { path: "/settings", label: t("nav.settings") }
  ]);

  return (
    <div class={`app-layout ${hasBanner() ? "" : "no-banner"}`}>
      <header class="top-bar">
        <div class="brand">
          <img src={IconBridge} class="brand-icon" alt="WSL Bridge" />
          <span>{t("app.name")}</span>
        </div>
        
        <div class="app-status-section">
          <Show when={runtimeStatusQuery.data?.admin_features_available}>
            <div class="status-chip running">{t("common.adminMode")}</div>
          </Show>
        </div>
      </header>
      
      <nav class="tab-nav">
        <For each={navItems()}>
          {(item) => (
            <Link 
              to={item.path} 
              class="tab-item"
              data-status={currentPath() === item.path || (item.path === "/dashboard" && currentPath() === "/") ? "active" : undefined}
            >
              {item.label}
            </Link>
          )}
        </For>
      </nav>
      
      <Show when={hasBanner()}>
        <AppRuntimeBanner />
      </Show>
      
      <main class="content">
        <Outlet />
      </main>
      
      <footer class="status-bar">
        <div class="status-bar-left">
          <Show when={runtimeStatusQuery.data}>
            <div class="status-item">
              <span>v{runtimeStatusQuery.data?.build_flavor}</span>
            </div>
          </Show>
        </div>
        <div class="status-bar-right">
          <div class="status-item">
            <span>{theme.resolvedTheme()}</span>
          </div>
        </div>
      </footer>
    </div>
  );
}

const rootRoute = createRootRoute({
  component: RootLayout,
  errorComponent: ErrorPage
});

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: withSuspense(() => <DashboardPage />)
});

const dashboardRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/dashboard",
  component: withSuspense(() => <DashboardPage />)
});

const rulesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/rules",
  component: withSuspense(() => <RulesPage />)
});

const runtimeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/runtime",
  component: withSuspense(() => <RuntimePage />)
});

const topologyRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/topology",
  component: withSuspense(() => <TopologyPage />)
});

const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: withSuspense(() => <SettingsPage />)
});

const routeTree = rootRoute.addChildren([
  indexRoute,
  dashboardRoute,
  rulesRoute,
  runtimeRoute,
  topologyRoute,
  settingsRoute
]);

export const router = createRouter({
  routeTree
});

declare module "@tanstack/solid-router" {
  interface Register {
    router: typeof router;
  }
}
