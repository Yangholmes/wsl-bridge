import { createStore } from "solid-js/store";

import type { SelectedProxyNode } from "./model";

export type ProxyCanvasViewState = {
  scale: number;
  x: number;
  y: number;
  selectedNode: SelectedProxyNode;
  searchOpen: boolean;
  searchKeyword: string;
};

export const [proxyCanvasViewState, setProxyCanvasViewState] =
  createStore<ProxyCanvasViewState>({
    scale: 1,
    x: 0,
    y: 0,
    selectedNode: null,
    searchOpen: false,
    searchKeyword: ""
  });
