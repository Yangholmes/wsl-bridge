import { lazy, Suspense, createEffect, createMemo, Show, For } from "solid-js";
import { createRootRoute, createRoute, createRouter, Link, Outlet, useRouterState } from "@tanstack/solid-router";
import { useI18n } from "./i18n/context";
import IconBridge from "./assets/bridge-logo.svg?url";
import { ErrorPage } from "./features/ErrorPage";
import { SkeletonTitle } from "./lib/Skeleton";
import { AppRuntimeBanner } from "./lib/AppRuntimeBanner";
import { useAppRuntimeStatusQuery } from "./lib/appRuntime";
import { useTheme } from "./lib/theme";
import {
  DashboardIcon,
  RulesIcon,
  RuntimeIcon,
  SettingsIcon,
  StatusBadge,
  TopologyIcon
} from "./lib/ui";
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
    <section class="panel page-loading-card">
      <SkeletonTitle />
      <div class="page-loading-metrics">
        <div class="skeleton-grid dashboard-skeleton-grid" />
        <div class="skeleton-grid dashboard-skeleton-grid" />
        <div class="skeleton-grid dashboard-skeleton-grid" />
      </div>
      <div class="skeleton-grid page-loading-body" />
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
  const runtimeStateLabel = createMemo(() =>
    runtimeStatusQuery.data?.admin_features_available ? t("common.running") : t("common.ready")
  );

  const navItems = createMemo(() => [
    { path: "/dashboard", label: t("nav.dashboard"), icon: DashboardIcon },
    { path: "/rules", label: t("nav.rules"), icon: RulesIcon },
    { path: "/runtime", label: t("nav.runtime"), icon: RuntimeIcon },
    { path: "/topology", label: t("nav.topology"), icon: TopologyIcon },
    { path: "/settings", label: t("nav.settings"), icon: SettingsIcon }
  ]);
  let contentRef: HTMLDivElement | undefined;

  createEffect(() => {
    currentPath();
    queueMicrotask(() => contentRef?.scrollTo({ top: 0, left: 0, behavior: "auto" }));
  });

  return (
    <div class="app-layout">
      <aside class="app-sidebar">
        <div class="sidebar-brand">
          <div class="brand">
            <img src={IconBridge} class="brand-icon" alt="WSL Bridge" />
            <span>{t("app.name")}</span>
          </div>
        </div>

        <nav class="sidebar-nav">
          <For each={navItems()}>
            {(item) => {
              const Icon = item.icon;
              const isActive = () => currentPath() === item.path || (item.path === "/dashboard" && currentPath() === "/");
              return (
                <Link to={item.path} class="sidebar-nav-item" data-active={isActive() ? "true" : undefined}>
                  <Icon size={18} />
                  <span>{item.label}</span>
                </Link>
              );
            }}
          </For>
        </nav>
      </aside>

      <section class="app-main">
        <header class="main-header">
          <div class="main-header-meta">
            <StatusBadge
              state={runtimeStatusQuery.data?.admin_features_available ? "running" : "stopped"}
              label={runtimeStatusQuery.data?.admin_features_available ? t("common.engineAvailable") : t("common.limitedMode")}
            />
            <Show when={runtimeStatusQuery.data?.is_admin}>
              <StatusBadge state="ready" label={t("common.admin")} />
            </Show>
          </div>
        </header>

        <Show when={hasBanner()}>
          <AppRuntimeBanner />
        </Show>

        <main class="content" ref={contentRef}>
          <div class="content-inner">
            <Outlet />
          </div>
        </main>
      </section>
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
