import type { ProxyCanvasEdge, ProxyCanvasNode } from "./model";

export type CanvasBounds = {
  x: number;
  y: number;
  width: number;
  height: number;
};

export type LayoutNode = ProxyCanvasNode & {
  x: number;
  y: number;
  width: number;
  height: number;
  subtreeHeight: number;
};

export type LayoutEdge = ProxyCanvasEdge & {
  fromX: number;
  fromY: number;
  toX: number;
  toY: number;
};

export type ProxyCanvasLayout = {
  nodes: LayoutNode[];
  edges: LayoutEdge[];
  bounds: CanvasBounds;
};

const NODE_WIDTH = 190;
const NODE_HEIGHT = 72;
const LAYER_GAP = 230;
const SIBLING_GAP = 30;
const TREE_GAP = 86;
const CANVAS_PADDING = 80;

export function computeProxyCanvasLayout(
  nodes: ProxyCanvasNode[],
  edges: ProxyCanvasEdge[]
): ProxyCanvasLayout {
  if (nodes.length === 0) {
    return {
      nodes: [],
      edges: [],
      bounds: { x: 0, y: 0, width: 640, height: 320 }
    };
  }

  const nodeMap = new Map(nodes.map((node) => [node.key, node] as const));
  const children = new Map<string, ProxyCanvasNode[]>();
  const roots = nodes.filter((node) => !node.parentKey);
  for (const node of nodes) {
    if (!node.parentKey) continue;
    const bucket = children.get(node.parentKey) ?? [];
    bucket.push(node);
    children.set(node.parentKey, bucket);
  }

  const subtreeHeight = new Map<string, number>();
  const measure = (node: ProxyCanvasNode): number => {
    const childNodes = children.get(node.key) ?? [];
    if (childNodes.length === 0) {
      subtreeHeight.set(node.key, NODE_HEIGHT);
      return NODE_HEIGHT;
    }
    const childrenHeight =
      childNodes.reduce((total, child) => total + measure(child), 0) +
      SIBLING_GAP * (childNodes.length - 1);
    const height = Math.max(NODE_HEIGHT, childrenHeight);
    subtreeHeight.set(node.key, height);
    return height;
  };

  for (const root of roots) measure(root);

  const layoutNodes: LayoutNode[] = [];
  const place = (node: ProxyCanvasNode, layer: number, top: number) => {
    const height = subtreeHeight.get(node.key) ?? NODE_HEIGHT;
    const x = CANVAS_PADDING + layer * LAYER_GAP;
    const y = top + (height - NODE_HEIGHT) / 2;
    layoutNodes.push({
      ...node,
      x,
      y,
      width: NODE_WIDTH,
      height: NODE_HEIGHT,
      subtreeHeight: height
    });

    let childTop = top;
    for (const child of children.get(node.key) ?? []) {
      place(child, layer + 1, childTop);
      childTop += (subtreeHeight.get(child.key) ?? NODE_HEIGHT) + SIBLING_GAP;
    }
  };

  let top = CANVAS_PADDING;
  for (const root of roots) {
    place(root, 0, top);
    top += (subtreeHeight.get(root.key) ?? NODE_HEIGHT) + TREE_GAP;
  }

  const layoutMap = new Map(layoutNodes.map((node) => [node.key, node] as const));
  const layoutEdges = edges
    .map((edge) => {
      const from = layoutMap.get(edge.from);
      const to = layoutMap.get(edge.to);
      if (!from || !to || !nodeMap.has(edge.from) || !nodeMap.has(edge.to)) return null;
      return {
        ...edge,
        fromX: from.x + from.width,
        fromY: from.y + from.height / 2,
        toX: to.x,
        toY: to.y + to.height / 2
      };
    })
    .filter((edge): edge is LayoutEdge => Boolean(edge));

  const maxRight = Math.max(...layoutNodes.map((node) => node.x + node.width));
  const maxBottom = Math.max(...layoutNodes.map((node) => node.y + node.height));

  return {
    nodes: layoutNodes,
    edges: layoutEdges,
    bounds: {
      x: 0,
      y: 0,
      width: maxRight + CANVAS_PADDING,
      height: maxBottom + CANVAS_PADDING
    }
  };
}
