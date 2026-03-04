import { createRootRoute, createRoute, createRouter, Link, Outlet } from "@tanstack/solid-router";

import { RulesPage } from "./features/rules/RulesPage";
import { RuntimePage } from "./features/runtime/RuntimePage";
import { TopologyPage } from "./features/topology/TopologyPage";

function RootLayout() {
  return (
    <div class="app-layout">
      <aside class="sidebar">
        <div class="brand">WSL Bridge</div>
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
          <Link to="/topology" class="nav-item">
            Topology
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
  component: RootLayout,
  errorComponent: (props) => {
    // 在控制台打印完整的 error 对象，查看 stack 堆栈
    console.error(props.error)
    return (
      <div>
        <button onClick={() => props.reset()}>重试</button>
      </div>
    );
  },
});

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: () => (
    <Placeholder
      title="Dashboard"
      text="M2 已完成：支持动态目标解析、拓扑探测、运行态错误定位与网卡变化自动重绑。"
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
  component: RuntimePage
});

const topologyRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/topology",
  component: TopologyPage
});

const logsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/logs",
  component: () => <Placeholder title="Logs" text="M3 将补充日志筛选与导出。" />
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
