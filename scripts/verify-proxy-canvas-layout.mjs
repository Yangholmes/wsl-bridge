import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";
import ts from "typescript";

const root = process.cwd();

function loadTsModule(relativePath) {
  const filePath = path.join(root, relativePath);
  const source = fs.readFileSync(filePath, "utf8");
  const output = ts.transpileModule(source, {
    compilerOptions: {
      target: ts.ScriptTarget.ES2021,
      module: ts.ModuleKind.CommonJS,
      esModuleInterop: true
    },
    fileName: filePath
  }).outputText;
  const module = { exports: {} };
  vm.runInNewContext(output, {
    exports: module.exports,
    module,
    require: (id) => {
      throw new Error(`Unexpected runtime import in ${relativePath}: ${id}`);
    }
  }, { filename: filePath });
  return module.exports;
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

const { computeProxyCanvasLayout } = loadTsModule("src/features/proxy/canvas/layout.ts");
const { sortRoutes, sortUpstreams } = loadTsModule("src/features/proxy/canvas/model.ts");

const listener = {
  key: "listener:l1",
  id: "l1",
  kind: "listener",
  parentKey: null,
  title: "listener",
  subtitle: "http://127.0.0.1:8080",
  enabled: true,
  hasError: false,
  runtimeError: null,
  source: {}
};
const routeA = {
  key: "route:r1",
  id: "r1",
  kind: "route",
  parentKey: "listener:l1",
  title: "a.com",
  subtitle: "/",
  enabled: true,
  hasError: false,
  runtimeError: null,
  source: {}
};
const upstreamA = {
  key: "upstream:u1",
  id: "u1",
  kind: "upstream",
  parentKey: "route:r1",
  title: "static",
  subtitle: "http 127.0.0.1:3000",
  enabled: true,
  hasError: false,
  runtimeError: null,
  source: {}
};

const layout = computeProxyCanvasLayout(
  [listener, routeA, upstreamA],
  [
    { key: "listener:l1->route:r1", from: "listener:l1", to: "route:r1", enabled: true, hasError: false },
    { key: "route:r1->upstream:u1", from: "route:r1", to: "upstream:u1", enabled: true, hasError: false }
  ]
);
const listenerNode = layout.nodes.find((node) => node.key === listener.key);
const routeNode = layout.nodes.find((node) => node.key === routeA.key);
const upstreamNode = layout.nodes.find((node) => node.key === upstreamA.key);
assert(listenerNode, "listener node should exist");
assert(routeNode, "route node should exist");
assert(upstreamNode, "upstream node should exist");
assert(routeNode.x > listenerNode.x, "route should be placed to the right of listener");
assert(upstreamNode.x > routeNode.x, "upstream should be placed to the right of route");
assert(layout.edges.length === 2, "layout should keep valid edges");
assert(layout.bounds.width > upstreamNode.x + upstreamNode.width, "bounds should include upstream node");

const sortedRoutes = sortRoutes([
  { id: "default", is_default: true, server_names: [], path_prefix: null, created_at: "2026-01-03T00:00:00Z" },
  { id: "short", is_default: false, server_names: ["a.com"], path_prefix: "/", created_at: "2026-01-02T00:00:00Z" },
  { id: "long", is_default: false, server_names: ["a.com"], path_prefix: "/api", created_at: "2026-01-01T00:00:00Z" }
]);
assert(sortedRoutes.map((route) => route.id).join(",") === "long,short,default", "routes should follow path priority then default last");

const sortedUpstreams = sortUpstreams([
  { id: "old", enabled: true, created_at: "2026-01-01T00:00:00Z" },
  { id: "disabled-new", enabled: false, created_at: "2026-01-03T00:00:00Z" },
  { id: "new", enabled: true, created_at: "2026-01-02T00:00:00Z" }
]);
assert(sortedUpstreams.map((upstream) => upstream.id).join(",") === "new,old,disabled-new", "upstreams should prefer enabled then newest");

console.log("Proxy canvas layout assertions passed.");
