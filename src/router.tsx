import { lazy, Suspense } from "solid-js";
import { createRootRoute, createRoute, createRouter, Link, Outlet } from "@tanstack/solid-router";
import { useI18n } from "./i18n/context";

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
const LogsPage = lazy(() =>
  import("./features/logs/LogsPage").then((module) => ({ default: module.LogsPage }))
);
const SettingsPage = lazy(() =>
  import("./features/settings/SettingsPage").then((module) => ({ default: module.SettingsPage }))
);

function PageLoadingFallback() {
  return (
    <section class="panel">
      <div class="skeleton-title" />
      <div class="skeleton-line wide" />
      <div class="skeleton-line" />
      <div class="skeleton-line" />
    </section>
  );
}

function withSuspense(component: () => any) {
  return () => <Suspense fallback={<PageLoadingFallback />}>{component()}</Suspense>;
}

function RootLayout() {
  const { t } = useI18n();

  return (
    <div class="app-layout">
      <aside class="sidebar">
        <div class="brand">{t("app.name")}</div>
        <nav class="nav">
          <Link to="/dashboard" class="nav-item">
            {t("nav.dashboard")}
          </Link>
          <Link to="/rules" class="nav-item">
            {t("nav.rules")}
          </Link>
          <Link to="/runtime" class="nav-item">
            {t("nav.runtime")}
          </Link>
          <Link to="/topology" class="nav-item">
            {t("nav.topology")}
          </Link>
          <Link to="/logs" class="nav-item">
            {t("nav.logs")}
          </Link>
          <Link to="/settings" class="nav-item">
            {t("nav.settings")}
          </Link>
        </nav>
      </aside>
      <main class="content">
        <Outlet />
      </main>
    </div>
  );
}

const rootRoute = createRootRoute({
  component: RootLayout,
  errorComponent: (props) => {
    const { t } = useI18n();
    // 在控制台打印完整的 error 对象，查看 stack 堆栈
    console.error(props.error)
    console.log(props)
    return (
      <div>
        <button onClick={() => props.reset()}>{t("common.retry")}</button>
      </div>
    );
  },
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

const logsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/logs",
  component: withSuspense(() => <LogsPage />)
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
  logsRoute,
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
