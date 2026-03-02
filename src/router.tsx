import { createRootRoute, createRoute, createRouter, Link, Outlet } from "@tanstack/solid-router";

import { RulesPage } from "./features/rules/RulesPage";

function RootLayout() {
  return (
    <div class="app-layout">
      <aside class="sidebar">
        <div class="brand">WSL Bridge</div>
        <div class="sub">Solid + TanStack</div>
        <nav class="nav">
          <Link to="/" class="nav-item">
            Dashboard
          </Link>
          <Link to="/rules" class="nav-item">
            Rules
          </Link>
          <Link to="/runtime" class="nav-item">
            Runtime
          </Link>
          <Link to="/logs" class="nav-item">
            Logs
          </Link>
          <Link to="/settings" class="nav-item">
            Settings
          </Link>
        </nav>
      </aside>
      <main class="content">
        <Outlet />
      </main>
    </div>
  );
}

function Placeholder(props: { title: string; text: string }) {
  return (
    <section class="panel">
      <h2>{props.title}</h2>
      <p class="muted">{props.text}</p>
    </section>
  );
}

const rootRoute = createRootRoute({
  component: RootLayout
});

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: () => (
    <Placeholder
      title="Dashboard"
      text="M1 已完成，当前可在 Rules 页面执行完整规则管理与调试。"
    />
  )
});

const rulesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/rules",
  component: RulesPage
});

const runtimeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/runtime",
  component: () => <Placeholder title="Runtime" text="M2 将补充独立运行态监控页面。" />
});

const logsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/logs",
  component: () => <Placeholder title="Logs" text="M2 将补充日志筛选与导出。" />
});

const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: () => (
    <Placeholder
      title="Settings"
      text="当前可通过环境变量配置 DB 路径与防火墙模式，详见 README。"
    />
  )
});

const routeTree = rootRoute.addChildren([
  indexRoute,
  rulesRoute,
  runtimeRoute,
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

