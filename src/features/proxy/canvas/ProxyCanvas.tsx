import * as KButton from "@kobalte/core/button";
import * as KTextField from "@kobalte/core/text-field";
import { Application, Container, Graphics, Rectangle, Text } from "pixi.js";
import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";

import { useI18n } from "../../../i18n/context";
import type {
  ProxyListener,
  ProxyRoute,
  ProxyRouteRuntimeItem,
  ProxyRuntimeStatusItem,
  ProxyUpstream,
  ProxyUpstreamRuntimeItem
} from "../../../lib/types";
import { ActionButton } from "../../../lib/ui";
import { computeProxyCanvasLayout, type LayoutNode, type ProxyCanvasLayout } from "./layout";
import {
  buildProxyCanvasGraph,
  nodeKey,
  parseNodeKey,
  type ProxyCanvasNode,
  type ProxyTopologyData,
  type SelectedProxyNode,
  upstreamTargetLabel
} from "./model";
import { proxyCanvasViewState, setProxyCanvasViewState } from "./viewState";
import "./ProxyCanvas.css";

type ContextMenuState = {
  x: number;
  y: number;
  target: SelectedProxyNode;
} | null;

type ProxyCanvasProps = {
  listeners: ProxyListener[];
  routesByListener: Map<string, ProxyRoute[]>;
  upstreamsByRoute: Map<string, ProxyUpstream[]>;
  listenerRuntime: Map<string, ProxyRuntimeStatusItem>;
  routeRuntime: Map<string, ProxyRouteRuntimeItem>;
  upstreamRuntime: Map<string, ProxyUpstreamRuntimeItem>;
  loading?: boolean;
  onRefresh: () => void;
  onOpenCertificates?: () => void;
  onOpenGuide?: () => void;
  onCreateListener: () => void;
  onCreateRoute: (listenerId: string) => void;
  onCreateUpstream: (routeId: string) => void;
  onEditListener: (listener: ProxyListener) => void;
  onEditRoute: (route: ProxyRoute) => void;
  onEditUpstream: (upstream: ProxyUpstream) => void;
  onDeleteNode: (node: ProxyCanvasNode) => void;
  onSelectedNodeChange?: (node: SelectedProxyNode) => void;
};

const MIN_SCALE = 0.1;
const MAX_SCALE = 3;
const ANIMATION_MS = 220;

export function ProxyCanvas(props: ProxyCanvasProps) {
  const { t } = useI18n();
  let hostRef: HTMLDivElement | undefined;
  let stageRef: HTMLDivElement | undefined;
  let app: Application | undefined;
  let viewport: Container | undefined;
  let edgeLayer: Container | undefined;
  let nodeLayer: Container | undefined;
  let overlayLayer: Container | undefined;
  let resizeObserver: ResizeObserver | undefined;
  let activeAnimation: (() => void) | undefined;
  let highlightTimer: number | undefined;
  let searchInputRef: HTMLInputElement | undefined;
  let lastNodePointerButton = 0;
  let lastNodeContextMenuAt = 0;
  let pointerStartedOnNode = false;
  let suppressNextCanvasClick = false;
  let didInitialFit = false;
  const lastPositions = new Map<string, { x: number; y: number }>();
  const [ready, setReady] = createSignal(false);
  const [contextMenu, setContextMenu] = createSignal<ContextMenuState>(null);
  const [showReset, setShowReset] = createSignal(false);
  const [searchResults, setSearchResults] = createSignal<string[]>([]);
  const [searchIndex, setSearchIndex] = createSignal(0);
  const [highlightedNodeKey, setHighlightedNodeKey] = createSignal<string | null>(null);

  const topology = createMemo<ProxyTopologyData>(() => ({
    listeners: props.listeners,
    routesByListener: props.routesByListener,
    upstreamsByRoute: props.upstreamsByRoute,
    listenerRuntime: props.listenerRuntime,
    routeRuntime: props.routeRuntime,
    upstreamRuntime: props.upstreamRuntime
  }));

  const graph = createMemo(() =>
    buildProxyCanvasGraph(topology(), {
      defaultRoute: t("proxy.defaultRoute")
    })
  );
  const layout = createMemo(() => computeProxyCanvasLayout(graph().nodes, graph().edges));
  const nodeMap = createMemo(() => new Map(layout().nodes.map((node) => [node.key, node] as const)));
  const selectedLayoutNode = createMemo(() => {
    const selected = proxyCanvasViewState.selectedNode;
    return selected ? nodeMap().get(nodeKey(selected.kind, selected.id)) ?? null : null;
  });

  onMount(() => {
    if (!hostRef || !stageRef) return;
    let destroyed = false;
    const pixiApp = new Application();

    const syncStageHeight = () => {
      if (!hostRef || !stageRef) return;
      const toolbar = hostRef.querySelector<HTMLElement>(".proxy-canvas-toolbar");
      const nextHeight = Math.max(0, hostRef.clientHeight - (toolbar?.offsetHeight ?? 0));
      if (nextHeight > 0) {
        stageRef.style.height = `${nextHeight}px`;
      }
    };

    syncStageHeight();
    void pixiApp
      .init({
        resizeTo: stageRef,
        backgroundAlpha: 0,
        antialias: true,
        autoDensity: true,
        resolution: window.devicePixelRatio || 1
      })
      .then(() => {
        if (destroyed || !stageRef) {
          pixiApp.destroy(true);
          return;
        }
        app = pixiApp;
        viewport = new Container();
        edgeLayer = new Container();
        nodeLayer = new Container();
        overlayLayer = new Container();
        viewport.addChild(edgeLayer, nodeLayer, overlayLayer);
        pixiApp.stage.addChild(viewport);
        stageRef.appendChild(pixiApp.canvas);
        restoreViewport();
        setupStageInteractions(pixiApp);
        resizeObserver = new ResizeObserver(() => {
          syncStageHeight();
          pixiApp.resize();
          updateResetVisibility(layout());
        });
        resizeObserver.observe(hostRef);
        resizeObserver.observe(stageRef);
        window.addEventListener("resize", syncStageHeight);
        setReady(true);
      });

    onCleanup(() => {
      destroyed = true;
      if (activeAnimation && pixiApp) pixiApp.ticker.remove(activeAnimation);
      if (highlightTimer) window.clearTimeout(highlightTimer);
      resizeObserver?.disconnect();
      window.removeEventListener("resize", syncStageHeight);
      pixiApp.destroy(true);
    });
  });

  createEffect(() => {
    if (!ready()) return;
    renderLayout(layout());
    updateSearchResults();
  });

  createEffect(() => {
    const selected = proxyCanvasViewState.selectedNode;
    props.onSelectedNodeChange?.(selected);
    if (!ready()) return;
    drawOverlay(layout());
  });

  createEffect(() => {
    highlightedNodeKey();
    if (!ready()) return;
    drawOverlay(layout());
  });

  createEffect(() => {
    if (!ready()) return;
    updateSearchResults();
  });

  function setupStageInteractions(pixiApp: Application) {
    if (!stageRef || !viewport) return;
    let dragging = false;
    let dragMoved = false;
    let last = { x: 0, y: 0 };

    const onPointerDown = (event: PointerEvent) => {
      if (isOverlayTarget(event.target)) return;
      if (event.button !== 0) return;
      if (pointerStartedOnNode) return;
      closeContextMenu();
      stageRef?.setAttribute("data-dragging", "true");
      dragging = true;
      dragMoved = false;
      last = { x: event.clientX, y: event.clientY };
    };
    const onPointerMove = (event: PointerEvent) => {
      if (!dragging || !viewport) return;
      const dx = event.clientX - last.x;
      const dy = event.clientY - last.y;
      if (Math.abs(dx) + Math.abs(dy) > 2) dragMoved = true;
      viewport.x += dx;
      viewport.y += dy;
      last = { x: event.clientX, y: event.clientY };
      persistViewport();
      updateResetVisibility(layout());
    };
    const onPointerUp = () => {
      stageRef?.removeAttribute("data-dragging");
      dragging = false;
      pointerStartedOnNode = false;
    };
    const onClick = (event: MouseEvent) => {
      if (isOverlayTarget(event.target)) return;
      if (event.button !== 0) return;
      if (suppressNextCanvasClick) {
        suppressNextCanvasClick = false;
        return;
      }
      if (!dragMoved) selectNode(null);
    };
    const onWheel = (event: WheelEvent) => {
      if (!viewport || !stageRef) return;
      event.preventDefault();
      closeContextMenu();
      const rect = stageRef.getBoundingClientRect();
      const mouse = {
        x: event.clientX - rect.left,
        y: event.clientY - rect.top
      };
      const oldScale = viewport.scale.x;
      const nextScale = clamp(oldScale * (event.deltaY > 0 ? 0.9 : 1.1), MIN_SCALE, MAX_SCALE);
      const worldX = (mouse.x - viewport.x) / oldScale;
      const worldY = (mouse.y - viewport.y) / oldScale;
      viewport.scale.set(nextScale);
      viewport.x = mouse.x - worldX * nextScale;
      viewport.y = mouse.y - worldY * nextScale;
      persistViewport();
      updateResetVisibility(layout());
    };
    const onContextMenu = (event: MouseEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (performance.now() - lastNodeContextMenuAt < 80) return;
      setContextMenu({
        x: event.offsetX,
        y: event.offsetY,
        target: null
      });
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        event.stopPropagation();
        setProxyCanvasViewState("searchOpen", !proxyCanvasViewState.searchOpen);
      }
      if (event.key === "Escape") {
        closeContextMenu();
        setProxyCanvasViewState("searchOpen", false);
      }
    };

    stageRef.addEventListener("pointerdown", onPointerDown);
    stageRef.addEventListener("click", onClick);
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
    stageRef.addEventListener("wheel", onWheel, { passive: false });
    stageRef.addEventListener("contextmenu", onContextMenu);
    window.addEventListener("keydown", onKeyDown, true);
    pixiApp.ticker.add(() => undefined);

    onCleanup(() => {
      stageRef?.removeEventListener("pointerdown", onPointerDown);
      stageRef?.removeEventListener("click", onClick);
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);
      stageRef?.removeEventListener("wheel", onWheel);
      stageRef?.removeEventListener("contextmenu", onContextMenu);
      window.removeEventListener("keydown", onKeyDown, true);
    });
  }

  createEffect(() => {
    if (!proxyCanvasViewState.searchOpen) return;
    setTimeout(() => {
      searchInputRef?.focus();
      searchInputRef?.select();
    }, 0);
  });

  function renderLayout(nextLayout: ProxyCanvasLayout) {
    if (!app || !viewport || !edgeLayer || !nodeLayer || !overlayLayer) return;
    if (activeAnimation) {
      app.ticker.remove(activeAnimation);
      activeAnimation = undefined;
    }
    const start = performance.now();
    const previous = new Map(lastPositions);
    edgeLayer.removeChildren();
    nodeLayer.removeChildren();
    overlayLayer.removeChildren();

    for (const edge of nextLayout.edges) {
      const graphic = new Graphics();
      edgeLayer.addChild(graphic);
      drawEdge(graphic, edge.fromX, edge.fromY, edge.toX, edge.toY, edge.enabled, edge.hasError);
    }

    const animatedNodes = nextLayout.nodes.map((node) => {
      const container = createNodeContainer(node);
      const previousPosition = previous.get(node.key) ?? { x: node.x - 24, y: node.y };
      container.x = previousPosition.x;
      container.y = previousPosition.y;
      nodeLayer!.addChild(container);
      return { container, node, from: previousPosition };
    });

    const tick = () => {
      const progress = clamp((performance.now() - start) / ANIMATION_MS, 0, 1);
      const eased = easeOutCubic(progress);
      for (const item of animatedNodes) {
        item.container.x = lerp(item.from.x, item.node.x, eased);
        item.container.y = lerp(item.from.y, item.node.y, eased);
      }
      if (progress >= 1) {
        app?.ticker.remove(tick);
        activeAnimation = undefined;
        lastPositions.clear();
        for (const node of nextLayout.nodes) {
          lastPositions.set(node.key, { x: node.x, y: node.y });
        }
      }
    };
    activeAnimation = tick;
    app.ticker.add(tick);
    drawOverlay(nextLayout);
    if (!didInitialFit && nextLayout.nodes.length > 0 && isDefaultViewState()) {
      didInitialFit = true;
      resetViewport();
    }
    updateResetVisibility(nextLayout);
  }

  function createNodeContainer(node: LayoutNode) {
    const container = new Container();
    container.eventMode = "static";
    container.cursor = "pointer";
    container.hitArea = new Rectangle(0, 0, node.width, node.height);
    container.on("pointerdown", (event) => {
      lastNodePointerButton = getPixiPointerButton(event);
      if (lastNodePointerButton === 0) {
        pointerStartedOnNode = true;
        suppressNextCanvasClick = true;
      }
      event.stopPropagation();
    });
    const handleSelect = (event: any) => {
      event.stopPropagation();
      if (getPixiPointerButton(event, lastNodePointerButton) !== 0) return;
      suppressNextCanvasClick = true;
      closeContextMenu();
      selectNode(parseNodeKey(node.key));
    };
    container.on("pointertap", handleSelect);
    container.on("click", handleSelect);
    const handleContextMenu = (event: any) => {
      event.stopPropagation();
      lastNodePointerButton = 2;
      pointerStartedOnNode = true;
      suppressNextCanvasClick = true;
      lastNodeContextMenuAt = performance.now();
      if (!stageRef) return;
      const point = getStagePointFromPixiEvent(event);
      setContextMenu({
        x: point.x,
        y: point.y,
        target: parseNodeKey(node.key)
      });
    };
    container.on("rightdown", handleContextMenu);
    container.on("rightclick", handleContextMenu);

    const color = getNodeColor(node);
    const card = new Graphics();
    card
      .roundRect(0, 0, node.width, node.height, 14)
      .fill({ color: color.fill, alpha: color.alpha })
      .stroke({ color: color.stroke, width: node.hasError ? 2.5 : 1.5 });
    container.addChild(card);

    const title = new Text({
      text: truncate(node.title, 22),
      style: {
        fontFamily: "Segoe UI, Microsoft YaHei UI, sans-serif",
        fontSize: 15,
        fontWeight: "600",
        fill: color.text
      }
    });
    title.resolution = Math.max(2, window.devicePixelRatio || 1);
    title.x = 16;
    title.y = 15;
    container.addChild(title);

    const subtitle = new Text({
      text: truncate(node.subtitle, 26),
      style: {
        fontFamily: "Segoe UI, Microsoft YaHei UI, sans-serif",
        fontSize: 12,
        fill: color.subtext
      }
    });
    subtitle.resolution = Math.max(2, window.devicePixelRatio || 1);
    subtitle.x = 16;
    subtitle.y = 42;
    container.addChild(subtitle);

    return container;
  }

  function drawOverlay(nextLayout: ProxyCanvasLayout) {
    if (!overlayLayer) return;
    overlayLayer.removeChildren();
    const highlighted = highlightedNodeKey();
    if (highlighted) {
      const node = nextLayout.nodes.find((item) => item.key === highlighted);
      if (node) {
        const glow = new Graphics();
        glow
          .roundRect(node.x - 10, node.y - 10, node.width + 20, node.height + 20, 22)
          .fill({ color: 0xffd76d, alpha: 0.18 })
          .stroke({ color: 0xffb900, width: 3, alpha: 0.9 });
        overlayLayer.addChild(glow);
      }
    }
    const selected = proxyCanvasViewState.selectedNode;
    if (!selected) return;
    const key = nodeKey(selected.kind, selected.id);
    const node = nextLayout.nodes.find((item) => item.key === key);
    if (!node) return;
    const selection = new Graphics();
    selection
      .roundRect(node.x - 5, node.y - 5, node.width + 10, node.height + 10, 18)
      .stroke({ color: 0x0a64ff, width: 2.5, alpha: 0.85 });
    overlayLayer.addChild(selection);
  }

  function drawEdge(
    graphic: Graphics,
    fromX: number,
    fromY: number,
    toX: number,
    toY: number,
    enabled: boolean,
    hasError: boolean
  ) {
    const color = hasError ? 0xc42b1c : enabled ? 0x0a64ff : 0x8791a1;
    const alpha = enabled || hasError ? 0.9 : 0.4;
    const midX = fromX + (toX - fromX) * 0.52;
    graphic
      .moveTo(fromX, fromY)
      .bezierCurveTo(midX, fromY, midX, toY, toX, toY)
      .stroke({ color, width: hasError ? 3 : 2.4, alpha });
  }

  function selectNode(node: SelectedProxyNode) {
    setProxyCanvasViewState("selectedNode", node);
  }

  function restoreViewport() {
    if (!viewport) return;
    viewport.scale.set(proxyCanvasViewState.scale);
    viewport.x = proxyCanvasViewState.x;
    viewport.y = proxyCanvasViewState.y;
  }

  function persistViewport() {
    if (!viewport) return;
    setProxyCanvasViewState("scale", viewport.scale.x);
    setProxyCanvasViewState("x", viewport.x);
    setProxyCanvasViewState("y", viewport.y);
  }

  function resetViewport() {
    if (!viewport || !stageRef) return;
    const bounds = layout().bounds;
    const padding = 56;
    const scale = clamp(
      Math.min(
        (stageRef.clientWidth - padding * 2) / bounds.width,
        (stageRef.clientHeight - padding * 2) / bounds.height,
        1
      ),
      MIN_SCALE,
      1
    );
    viewport.scale.set(scale);
    viewport.x = (stageRef.clientWidth - bounds.width * scale) / 2 - bounds.x * scale;
    viewport.y = (stageRef.clientHeight - bounds.height * scale) / 2 - bounds.y * scale;
    persistViewport();
    updateResetVisibility(layout());
  }

  function isDefaultViewState() {
    return (
      proxyCanvasViewState.scale === 1 &&
      proxyCanvasViewState.x === 0 &&
      proxyCanvasViewState.y === 0
    );
  }

  function updateResetVisibility(nextLayout: ProxyCanvasLayout) {
    if (!stageRef || !viewport) return;
    const scale = viewport.scale.x;
    const view = {
      x: viewport.x + nextLayout.bounds.x * scale,
      y: viewport.y + nextLayout.bounds.y * scale,
      width: nextLayout.bounds.width * scale,
      height: nextLayout.bounds.height * scale
    };
    const stageWidth = stageRef.clientWidth;
    const stageHeight = stageRef.clientHeight;
    const visible =
      view.x < stageWidth - 120 &&
      view.x + view.width > 120 &&
      view.y < stageHeight - 100 &&
      view.y + view.height > 100;
    setShowReset(!visible || scale < 0.65 || scale > 2.2);
  }

  function updateSearchResults() {
    const keyword = proxyCanvasViewState.searchKeyword.trim().toLowerCase();
    if (!keyword) {
      setSearchResults([]);
      setSearchIndex(0);
      return;
    }
    const matches = layout()
      .nodes
      .filter((node) => searchableText(node).includes(keyword))
      .map((node) => node.key);
    setSearchResults(matches);
    setSearchIndex((current) => (matches.length === 0 ? 0 : Math.min(current, matches.length - 1)));
  }

  function locateSearchResult(direction: -1 | 1) {
    const results = searchResults();
    if (results.length === 0) return;
    const next = (searchIndex() + direction + results.length) % results.length;
    setSearchIndex(next);
    locateNode(results[next]);
  }

  function locateCurrentSearchResult() {
    const results = searchResults();
    if (results.length === 0) return;
    locateNode(results[searchIndex()] ?? results[0]);
  }

  function locateNode(key: string) {
    const node = nodeMap().get(key);
    if (!node || !viewport || !stageRef) return;
    selectNode(parseNodeKey(key));
    flashNode(key);
    const nextScale = clamp(viewport.scale.x < 0.9 ? 1 : viewport.scale.x, 0.9, 1.2);
    viewport.scale.set(nextScale);
    viewport.x = stageRef.clientWidth * 0.5 - (node.x + node.width / 2) * nextScale;
    viewport.y = stageRef.clientHeight * 0.5 - (node.y + node.height / 2) * nextScale;
    persistViewport();
    updateResetVisibility(layout());
  }

  function flashNode(key: string) {
    setHighlightedNodeKey(key);
    if (highlightTimer) window.clearTimeout(highlightTimer);
    highlightTimer = window.setTimeout(() => setHighlightedNodeKey(null), 1200);
  }

  function closeContextMenu() {
    setContextMenu(null);
  }

  function getStagePointFromPixiEvent(event: any) {
    const nativeEvent = event.nativeEvent ?? event.originalEvent ?? event;
    const clientX = getNumericValue(nativeEvent.clientX, event.clientX, event.client?.x);
    const clientY = getNumericValue(nativeEvent.clientY, event.clientY, event.client?.y);
    if (clientX !== null && clientY !== null && stageRef) {
      const rect = stageRef.getBoundingClientRect();
      return {
        x: clientX - rect.left,
        y: clientY - rect.top
      };
    }
    return {
      x: event.global?.x ?? 0,
      y: event.global?.y ?? 0
    };
  }

  function getPixiPointerButton(event: any, fallback = 0) {
    const nativeEvent = event.nativeEvent ?? event.originalEvent ?? event;
    const button = getNumericValue(nativeEvent.button, event.button);
    return button ?? fallback;
  }

  function getNumericValue(...values: unknown[]) {
    for (const value of values) {
      if (typeof value === "number" && Number.isFinite(value)) return value;
    }
    return null;
  }

  function runMenuAction(event: Event, action: () => void) {
    event.preventDefault();
    event.stopPropagation();
    action();
  }

  function isOverlayTarget(target: EventTarget | null) {
    if (!(target instanceof Element)) return false;
    return Boolean(
      target.closest(".proxy-canvas-context-menu") ||
      target.closest(".proxy-canvas-detail-panel") ||
      target.closest(".proxy-canvas-search-float") ||
      target.closest(".proxy-canvas-viewport-tools") ||
      target.closest(".proxy-canvas-reset")
    );
  }

  function menuNode() {
    const target = contextMenu()?.target;
    return target ? nodeMap().get(nodeKey(target.kind, target.id)) ?? null : null;
  }

  function openCreateRouteFromNode(node: LayoutNode) {
    if (node.kind !== "listener") return;
    closeContextMenu();
    props.onCreateRoute(node.id);
  }

  function openCreateUpstreamFromNode(node: LayoutNode) {
    if (node.kind !== "route") return;
    closeContextMenu();
    props.onCreateUpstream(node.id);
  }

  function editNode(node: LayoutNode) {
    closeContextMenu();
    if (node.kind === "listener") props.onEditListener(node.source as ProxyListener);
    if (node.kind === "route") props.onEditRoute(node.source as ProxyRoute);
    if (node.kind === "upstream") props.onEditUpstream(node.source as ProxyUpstream);
  }

  function deleteNode(node: LayoutNode) {
    closeContextMenu();
    props.onDeleteNode(node);
  }

  return (
    <div class="proxy-canvas-shell" ref={hostRef}>
      <div class="proxy-canvas-toolbar">
        <div class="proxy-canvas-toolbar-main">
          <ActionButton onClick={props.onRefresh}>{t("common.refresh")}</ActionButton>
          <ActionButton variant="primary" onClick={props.onCreateListener}>
            {t("proxy.newListener")}
          </ActionButton>
          <ActionButton onClick={() => setProxyCanvasViewState("searchOpen", !proxyCanvasViewState.searchOpen)}>
            {t("proxy.canvasSearch")}
          </ActionButton>
          <Show when={props.onOpenCertificates}>
            {(openCertificates) => (
              <ActionButton onClick={openCertificates()}>
                {t("proxy.certificatesTitle")}
              </ActionButton>
            )}
          </Show>
          <Show when={props.onOpenGuide}>
            {(openGuide) => (
              <ActionButton size="small" onClick={openGuide()}>
                ?
              </ActionButton>
            )}
          </Show>
        </div>
      </div>

      <div class="proxy-canvas-stage" ref={stageRef}>
        <Show when={proxyCanvasViewState.searchOpen}>
          <div
            class="proxy-canvas-search-float"
            onPointerDown={(event) => event.stopPropagation()}
            onClick={(event) => event.stopPropagation()}
          >
            <KTextField.Root
              class="kb-field"
              value={proxyCanvasViewState.searchKeyword}
              onChange={(value) => {
                setProxyCanvasViewState("searchKeyword", value);
                queueMicrotask(locateCurrentSearchResult);
              }}
            >
              <KTextField.Input
                ref={searchInputRef}
                class="kb-input"
                placeholder={t("proxy.canvasSearchPlaceholder")}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    locateSearchResult(event.shiftKey ? -1 : 1);
                  }
                  if (event.key === "Escape") {
                    setProxyCanvasViewState("searchOpen", false);
                  }
                }}
              />
            </KTextField.Root>
            {/* <span class="muted proxy-canvas-search-scope">
              {t("proxy.canvasSearchScope")}
            </span> */}
            <span class="muted">
              {searchResults().length === 0 ? "0 / 0" : `${searchIndex() + 1} / ${searchResults().length}`}
            </span>
            <KButton.Root class="kb-btn ghost small" onClick={() => locateSearchResult(-1)}>
              {t("proxy.prevResult")}
            </KButton.Root>
            <KButton.Root class="kb-btn ghost small" onClick={() => locateSearchResult(1)}>
              {t("proxy.nextResult")}
            </KButton.Root>
          </div>
        </Show>
        <Show when={props.loading}>
          <div class="proxy-canvas-loading" aria-live="polite">
            <span class="proxy-canvas-loading-spinner" />
            <span>{t("common.loading")}</span>
          </div>
        </Show>
        <Show when={!props.loading && layout().nodes.length === 0}>
          <div class="proxy-canvas-empty">
            <div class="proxy-canvas-empty-card">
              <p class="heading-3">{t("proxy.emptyListeners")}</p>
              <p class="muted">{t("proxy.canvasEmptyHint")}</p>
            </div>
          </div>
        </Show>
        <div class="proxy-canvas-viewport-tools">
          <Show when={showReset()}>
            <KButton.Root class="kb-btn accent small" onClick={resetViewport}>
              {t("proxy.backToContent")}
            </KButton.Root>
          </Show>
          <span class="proxy-canvas-zoom-badge">
            {Math.round(proxyCanvasViewState.scale * 100)}%
          </span>
        </div>
        <Show when={contextMenu()}>
          {(menu) => (
            <div
              class="proxy-canvas-context-menu"
              onPointerDown={(event) => event.stopPropagation()}
              onClick={(event) => event.stopPropagation()}
              onContextMenu={(event) => {
                event.preventDefault();
                event.stopPropagation();
              }}
              style={{
                left: `${clamp(menu().x, 8, Math.max(8, (stageRef?.clientWidth ?? 220) - 220))}px`,
                top: `${clamp(menu().y, 8, Math.max(8, (stageRef?.clientHeight ?? 160) - 160))}px`
              }}
            >
              <Show
                when={menuNode()}
                fallback={
                  <KButton.Root
                    class="kb-btn ghost small"
                    onClick={() => {
                      closeContextMenu();
                      props.onCreateListener();
                    }}
                    onPointerDown={(event) =>
                      runMenuAction(event, () => {
                        closeContextMenu();
                        props.onCreateListener();
                      })
                    }
                  >
                    {t("proxy.newListener")}
                  </KButton.Root>
                }
              >
                {(node) => (
                  <>
                    <KButton.Root
                      class="kb-btn ghost small"
                      onClick={() => editNode(node())}
                      onPointerDown={(event) => runMenuAction(event, () => editNode(node()))}
                    >
                      {t("proxy.edit")}
                    </KButton.Root>
                    <Show when={node().kind === "listener"}>
                      <KButton.Root
                        class="kb-btn ghost small"
                        onClick={() => openCreateRouteFromNode(node())}
                        onPointerDown={(event) => runMenuAction(event, () => openCreateRouteFromNode(node()))}
                      >
                        {t("proxy.newRoute")}
                      </KButton.Root>
                    </Show>
                    <Show when={node().kind === "route"}>
                      <KButton.Root
                        class="kb-btn ghost small"
                        onClick={() => openCreateUpstreamFromNode(node())}
                        onPointerDown={(event) => runMenuAction(event, () => openCreateUpstreamFromNode(node()))}
                      >
                        {t("proxy.newUpstream")}
                      </KButton.Root>
                    </Show>
                    <KButton.Root
                      class="kb-btn danger small"
                      onClick={() => deleteNode(node())}
                      onPointerDown={(event) => runMenuAction(event, () => deleteNode(node()))}
                    >
                      {t("proxy.delete")}
                    </KButton.Root>
                  </>
                )}
              </Show>
            </div>
          )}
        </Show>
        <Show when={selectedLayoutNode()}>
          {(node) => (
            <ProxyCanvasDetail
              node={node()}
              listenerRuntime={props.listenerRuntime}
              routeRuntime={props.routeRuntime}
              upstreamRuntime={props.upstreamRuntime}
              onEdit={() => editNode(node())}
              onDelete={() => deleteNode(node())}
              onCreateRoute={() => openCreateRouteFromNode(node())}
              onCreateUpstream={() => openCreateUpstreamFromNode(node())}
            />
          )}
        </Show>
      </div>
    </div>
  );
}

function ProxyCanvasDetail(props: {
  node: LayoutNode;
  listenerRuntime: Map<string, ProxyRuntimeStatusItem>;
  routeRuntime: Map<string, ProxyRouteRuntimeItem>;
  upstreamRuntime: Map<string, ProxyUpstreamRuntimeItem>;
  onEdit: () => void;
  onDelete: () => void;
  onCreateRoute: () => void;
  onCreateUpstream: () => void;
}) {
  const { t } = useI18n();
  const rows = createMemo(() => detailRows(props.node, t, props));
  return (
    <div
      class="proxy-canvas-detail-panel"
      onPointerDown={(event) => event.stopPropagation()}
      onClick={(event) => event.stopPropagation()}
      onContextMenu={(event) => {
        event.preventDefault();
        event.stopPropagation();
      }}
    >
      <div class="proxy-canvas-detail-header">
        <div class="proxy-canvas-detail-title">
          <strong>{props.node.title}</strong>
          <span class="muted">{props.node.subtitle}</span>
        </div>
        <div class="row-actions">
          <ActionButton onClick={props.onEdit}>{t("proxy.edit")}</ActionButton>
          <Show when={props.node.kind === "listener"}>
            <ActionButton onClick={props.onCreateRoute}>{t("proxy.newRoute")}</ActionButton>
          </Show>
          <Show when={props.node.kind === "route"}>
            <ActionButton onClick={props.onCreateUpstream}>{t("proxy.newUpstream")}</ActionButton>
          </Show>
          <ActionButton variant="danger" onClick={props.onDelete}>{t("proxy.delete")}</ActionButton>
        </div>
      </div>
      <div class="proxy-canvas-detail-grid">
        <For each={rows()}>
          {(row) => (
            <div class="proxy-canvas-detail-item">
              <span>{row.label}</span>
              <strong>{row.value}</strong>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}

function detailRows(
  node: LayoutNode,
  t: ReturnType<typeof useI18n>["t"],
  context: {
    listenerRuntime: Map<string, ProxyRuntimeStatusItem>;
    routeRuntime: Map<string, ProxyRouteRuntimeItem>;
    upstreamRuntime: Map<string, ProxyUpstreamRuntimeItem>;
  }
) {
  if (node.kind === "listener") {
    const listener = node.source as ProxyListener;
    const runtime = context.listenerRuntime.get(listener.id);
    return [
      { label: t("proxy.listenHost"), value: `${listener.listen_host}:${listener.listen_port}` },
      { label: t("proxy.protocol"), value: listener.protocol },
      { label: t("proxy.tlsMode"), value: listener.tls_mode },
      { label: t("proxy.bindMode"), value: listener.bind_mode },
      { label: t("common.enabled"), value: listener.enabled ? t("common.enabled") : t("common.disabled") },
      { label: t("proxy.lastError"), value: runtime?.last_error ?? t("common.none") }
    ];
  }
  if (node.kind === "route") {
    const route = node.source as ProxyRoute;
    const runtime = context.routeRuntime.get(route.id);
    return [
      { label: t("proxy.serverNames"), value: route.is_default ? t("proxy.defaultRoute") : route.server_names.join(", ") },
      { label: t("proxy.pathPrefix"), value: route.path_prefix ?? "/" },
      { label: t("proxy.defaultRoute"), value: route.is_default ? t("common.enabled") : t("common.disabled") },
      { label: t("proxy.hitCount"), value: String(runtime?.hit_count ?? 0) },
      { label: t("proxy.errorCount"), value: String(runtime?.error_count ?? 0) },
      { label: t("proxy.lastError"), value: runtime?.last_error ?? t("common.none") }
    ];
  }
  const upstream = node.source as ProxyUpstream;
  const runtime = context.upstreamRuntime.get(upstream.id);
  return [
    { label: t("proxy.targetKind"), value: upstream.target_kind },
    { label: t("proxy.targetHost"), value: upstreamTargetLabel(upstream) },
    { label: t("proxy.upstreamScheme"), value: upstream.upstream_scheme },
    { label: t("proxy.rewriteFrom"), value: upstream.path_rewrite_from ?? t("common.none") },
    { label: t("proxy.rewriteTo"), value: upstream.path_rewrite_to ?? t("common.none") },
    { label: t("proxy.lastError"), value: runtime?.last_error ?? t("common.none") }
  ];
}

function getNodeColor(node: LayoutNode) {
  if (node.hasError) {
    return {
      fill: 0xfff0f0,
      stroke: 0xc42b1c,
      text: 0x7f1d1d,
      subtext: 0x9f2f25,
      alpha: 1
    };
  }
  if (!node.enabled) {
    return {
      fill: 0xf2f4f7,
      stroke: 0xb7c0cb,
      text: 0x5f6977,
      subtext: 0x8791a1,
      alpha: 0.88
    };
  }
  if (node.kind === "listener") {
    return { fill: 0xe8f1ff, stroke: 0x0a64ff, text: 0x174ea6, subtext: 0x3574f0, alpha: 1 };
  }
  if (node.kind === "route") {
    return { fill: 0xecf8f0, stroke: 0x16a34a, text: 0x14532d, subtext: 0x15803d, alpha: 1 };
  }
  return { fill: 0xf8f0ff, stroke: 0x9333ea, text: 0x581c87, subtext: 0x7e22ce, alpha: 1 };
}

function searchableText(node: LayoutNode) {
  if (node.kind === "listener") {
    const listener = node.source as ProxyListener;
    return [
      listener.name,
      listener.listen_host,
      listener.listen_port
    ].join(" ").toLowerCase();
  }
  if (node.kind === "route") {
    const route = node.source as ProxyRoute;
    return [
      route.server_names.join(" "),
      route.path_prefix
    ].join(" ").toLowerCase();
  }
  const upstream = node.source as ProxyUpstream;
  return [
    upstream.target_ref,
    upstream.target_host,
    upstream.target_port,
    upstream.path_rewrite_from,
    upstream.path_rewrite_to
  ].join(" ").toLowerCase();
}

function truncate(value: string, max: number) {
  return value.length > max ? `${value.slice(0, max - 1)}...` : value;
}

function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(max, value));
}

function lerp(from: number, to: number, progress: number) {
  return from + (to - from) * progress;
}

function easeOutCubic(value: number) {
  return 1 - Math.pow(1 - value, 3);
}
