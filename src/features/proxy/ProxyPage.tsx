import { open as openDialog } from "@tauri-apps/plugin-dialog";
import * as KButton from "@kobalte/core/button";
import * as KDialog from "@kobalte/core/dialog";
import * as KTextField from "@kobalte/core/text-field";
import { queryOptions, useQuery } from "@tanstack/solid-query";
import { useNavigate } from "@tanstack/solid-router";
import { createEffect, createMemo, createSignal, For, Show } from "solid-js";

import { useI18n } from "../../i18n/context";
import { BottomDrawer } from "../../lib/Drawer";
import { Hint } from "../../lib/Hint";
import { appQueryClient } from "../../lib/queryClient";
import { useToast } from "../../lib/Toast";
import {
  ActionButton,
  CheckboxField,
  MetricCard,
  PageHeader,
  SectionCard,
  SelectableCard,
  StatusBadge,
  TextFieldControl
} from "../../lib/ui";
import { SimpleSelect, type SelectOption } from "../../lib/SimpleSelect";
import type {
  BindMode,
  CreateProxyCertificateRequest,
  ProxyCertificate,
  ProxyCertificateSourceType,
  ProxyListener,
  ProxyProtocol,
  ProxyRule,
  ProxyRoute,
  ProxyRouteRuntimeItem,
  ProxyRuntimeStatusItem,
  ProxyTlsMode,
  ProxyUpstream,
  ProxyUpstreamRuntimeItem,
  RuntimeState,
  RuleMigrationRecord,
  TargetKind,
  UpdateProxyListenerRequest,
  UpdateProxyRouteRequest,
  UpdateProxyUpstreamRequest,
  UpstreamScheme
} from "../../lib/types";
import { listRuleMigrations, listRules } from "../rules/api";
import { createTopologyQueryOptions } from "../topology/state";
import { ProxyCanvas } from "./canvas/ProxyCanvas";
import type { ProxyTopologyData, SelectedProxyNode } from "./canvas/model";
import {
  createProxyCertificate,
  createProxyListener,
  createProxyRoute,
  createProxyUpstream,
  deleteProxyCertificate,
  deleteProxyListener,
  deleteProxyRoute,
  deleteProxyUpstream,
  getProxyRuntimeStatus,
  listProxyCertificates,
  listProxyListeners,
  listProxyRouteRuntime,
  listProxyRoutes,
  listProxyUpstreamRuntime,
  listProxyUpstreams,
  updateProxyCertificate,
  updateProxyListener,
  updateProxyRoute,
  updateProxyUpstream
} from "./api";
import "./ProxyPage.css";

type ProxyGuideSection = {
  title: string;
  paragraphs?: string[];
  ordered?: string[];
  unordered?: string[];
};

const proxyGuideContent: Record<string, { title: string; sections: ProxyGuideSection[] }> = {
  "zh-CN": {
    title: "Proxy 使用指南",
    sections: [
      {
        title: "模块用途",
        paragraphs: [
          "Proxy 用于把本机监听端口上的 HTTP/HTTPS 流量按 server name、路径和上游规则分发到不同目标。它适合替代旧 Rules 中的 tcp_fwd 与 http_proxy，并支持 WSL、Hyper-V 和静态主机三类上游。"
        ]
      },
      {
        title: "一条可用链路的组成",
        ordered: [
          "Listener：定义本机监听地址、端口、协议和 TLS 方式。",
          "Route：定义 server name、是否默认路由、路径前缀等匹配条件。",
          "Upstream：定义最终转发目标、协议、端口和可选路径改写。"
        ],
        paragraphs: ["流量匹配顺序是 Listener -> Route -> Upstream。每次请求只会命中一条优先级最高的 Route。"]
      },
      {
        title: "常见配置流程",
        ordered: [
          "点击新建 Listener，选择 HTTP 或 HTTPS，并设置监听端口。",
          "在 Listener 节点上右键创建 Route，填写 server name，例如 a.com 或 *.example.com。",
          "在 Route 节点上右键创建 Upstream，选择 static、wsl 或 hyperv 目标。",
          "如果是 HTTPS Listener，请先在工具栏的证书抽屉中创建或导入证书。",
          "保存后查看画布节点状态，红色节点代表存在运行错误，灰色节点代表已禁用。"
        ]
      },
      {
        title: "server name 与 Hosts",
        paragraphs: [
          "Proxy 只负责按请求中的 Host / SNI 分流，不负责域名解析。如果需要让 a.com 或 b.com 指向本机，请在 Hosts 模块中启用对应 hosts 分组。"
        ]
      },
      {
        title: "上游类型说明",
        unordered: [
          "static：手动填写目标主机和端口，例如 127.0.0.1:3000。",
          "wsl：选择已扫描到的 WSL 发行版，运行时会解析该发行版当前 IP。",
          "hyperv：选择已扫描到的 Hyper-V 虚拟机，运行时会解析虚拟机当前 IP。"
        ]
      },
      {
        title: "画布操作",
        unordered: [
          "左键点击节点：选中节点并打开右侧详情。",
          "右键点击节点：打开上下文菜单，可编辑、新增下级节点或删除。",
          "鼠标滚轮：缩放画布。",
          "拖动画布空白处：平移视图。",
          "Ctrl + F：打开画布搜索，Enter 跳转下一个结果。"
        ]
      },
      {
        title: "注意事项",
        unordered: [
          "HTTPS 需要证书；可以手动上传证书，也可以使用本地 CA 生成。",
          "gRPC / gRPCS 当前有协议约束，请优先使用默认 Route，并避免路径改写。",
          "WSL / Hyper-V 目标依赖拓扑扫描结果；如果列表为空，请先刷新拓扑或确认权限。",
          "默认 Route 用于兜底匹配；如果默认 Route 也不可用，请求会被拒绝并写入错误日志。"
        ]
      }
    ]
  },
  "zh-HK": {
    title: "Proxy 使用指南",
    sections: [
      {
        title: "模組用途",
        paragraphs: [
          "Proxy 用於把本機監聽連接埠上的 HTTP/HTTPS 流量按 server name、路徑和上游規則分發到不同目標。它適合替代舊 Rules 中的 tcp_fwd 與 http_proxy，並支援 WSL、Hyper-V 和靜態主機三類上游。"
        ]
      },
      {
        title: "一條可用鏈路的組成",
        ordered: [
          "Listener：定義本機監聽地址、連接埠、協議和 TLS 方式。",
          "Route：定義 server name、是否預設路由、路徑前綴等匹配條件。",
          "Upstream：定義最終轉發目標、協議、連接埠和可選路徑改寫。"
        ],
        paragraphs: ["流量匹配順序是 Listener -> Route -> Upstream。每次請求只會命中一條優先級最高的 Route。"]
      },
      {
        title: "常見配置流程",
        ordered: [
          "點擊新建 Listener，選擇 HTTP 或 HTTPS，並設定監聽連接埠。",
          "在 Listener 節點上右鍵建立 Route，填寫 server name，例如 a.com 或 *.example.com。",
          "在 Route 節點上右鍵建立 Upstream，選擇 static、wsl 或 hyperv 目標。",
          "如果是 HTTPS Listener，請先在工具欄的證書抽屜中建立或匯入證書。",
          "儲存後查看畫布節點狀態，紅色節點代表存在執行錯誤，灰色節點代表已停用。"
        ]
      },
      {
        title: "server name 與 Hosts",
        paragraphs: [
          "Proxy 只負責按請求中的 Host / SNI 分流，不負責域名解析。如果需要讓 a.com 或 b.com 指向本機，請在 Hosts 模組中啟用對應 hosts 分組。"
        ]
      },
      {
        title: "上游類型說明",
        unordered: [
          "static：手動填寫目標主機和連接埠，例如 127.0.0.1:3000。",
          "wsl：選擇已掃描到的 WSL 發行版，執行時會解析該發行版目前 IP。",
          "hyperv：選擇已掃描到的 Hyper-V 虛擬機，執行時會解析虛擬機目前 IP。"
        ]
      },
      {
        title: "畫布操作",
        unordered: [
          "左鍵點擊節點：選中節點並打開右側詳情。",
          "右鍵點擊節點：打開上下文選單，可編輯、新增下級節點或刪除。",
          "滑鼠滾輪：縮放畫布。",
          "拖動畫布空白處：平移視圖。",
          "Ctrl + F：打開畫布搜尋，Enter 跳轉下一個結果。"
        ]
      },
      {
        title: "注意事項",
        unordered: [
          "HTTPS 需要證書；可以手動上傳證書，也可以使用本地 CA 生成。",
          "gRPC / gRPCS 目前有協議限制，請優先使用預設 Route，並避免路徑改寫。",
          "WSL / Hyper-V 目標依賴拓撲掃描結果；如果列表為空，請先刷新拓撲或確認權限。",
          "預設 Route 用於兜底匹配；如果預設 Route 也不可用，請求會被拒絕並寫入錯誤日誌。"
        ]
      }
    ]
  },
  "en-US": {
    title: "Proxy Guide",
    sections: [
      {
        title: "What Proxy Does",
        paragraphs: [
          "Proxy routes HTTP/HTTPS traffic received by local listeners to different targets by server name, path, and upstream rules. It is intended to replace legacy tcp_fwd and http_proxy rules and supports static, WSL, and Hyper-V upstream targets."
        ]
      },
      {
        title: "A Working Chain",
        ordered: [
          "Listener: defines the local bind address, port, protocol, and TLS mode.",
          "Route: defines server names, default-route behavior, path prefix, and other matching conditions.",
          "Upstream: defines the final target, upstream protocol, port, and optional path rewrite."
        ],
        paragraphs: ["Traffic is matched in the order Listener -> Route -> Upstream. Each request matches only the highest-priority route."]
      },
      {
        title: "Common Setup Flow",
        ordered: [
          "Click New Listener, select HTTP or HTTPS, and set the listen port.",
          "Right-click the Listener node to create a Route, then enter server names such as a.com or *.example.com.",
          "Right-click the Route node to create an Upstream, then select static, wsl, or hyperv.",
          "For HTTPS listeners, create or import a certificate from the Certificates drawer first.",
          "After saving, check node states on the canvas. Red means runtime error; gray means disabled."
        ]
      },
      {
        title: "Server Names and Hosts",
        paragraphs: [
          "Proxy routes by request Host / SNI only. It does not resolve domains. If a.com or b.com should point to this machine, enable the corresponding hosts group in the Hosts module."
        ]
      },
      {
        title: "Upstream Target Types",
        unordered: [
          "static: manually enter a target host and port, for example 127.0.0.1:3000.",
          "wsl: select a scanned WSL distribution. Runtime resolves its current IP.",
          "hyperv: select a scanned Hyper-V VM. Runtime resolves its current IP."
        ]
      },
      {
        title: "Canvas Controls",
        unordered: [
          "Left-click a node: select it and open details on the right.",
          "Right-click a node: open the context menu to edit, create child nodes, or delete.",
          "Mouse wheel: zoom the canvas.",
          "Drag empty canvas space: pan the view.",
          "Ctrl + F: open canvas search. Press Enter to jump to the next result."
        ]
      },
      {
        title: "Notes",
        unordered: [
          "HTTPS requires a certificate. You can upload one manually or generate one with the local CA.",
          "gRPC / gRPCS still has protocol constraints. Prefer default routes and avoid path rewrite.",
          "WSL / Hyper-V targets depend on topology scan results. If the list is empty, refresh topology or check privileges.",
          "Default routes are used as fallback. If the default route is also invalid, the request is rejected and an error is logged."
        ]
      }
    ]
  },
  "ja-JP": {
    title: "Proxy ガイド",
    sections: [
      {
        title: "Proxy の用途",
        paragraphs: [
          "Proxy はローカル Listener が受けた HTTP/HTTPS トラフィックを、server name、パス、Upstream ルールに基づいて別々のターゲットへ転送します。旧 Rules の tcp_fwd と http_proxy を置き換えるための機能で、static、WSL、Hyper-V の Upstream をサポートします。"
        ]
      },
      {
        title: "有効なチェーンの構成",
        ordered: [
          "Listener：ローカルのバインドアドレス、ポート、プロトコル、TLS モードを定義します。",
          "Route：server name、デフォルト Route、パスプレフィックスなどのマッチ条件を定義します。",
          "Upstream：最終ターゲット、上流プロトコル、ポート、任意のパス書き換えを定義します。"
        ],
        paragraphs: ["トラフィックは Listener -> Route -> Upstream の順にマッチします。各リクエストは優先度が最も高い Route 1 件だけにマッチします。"]
      },
      {
        title: "基本的な設定手順",
        ordered: [
          "New Listener をクリックし、HTTP または HTTPS を選択して listen port を設定します。",
          "Listener ノードを右クリックして Route を作成し、a.com や *.example.com などの server name を入力します。",
          "Route ノードを右クリックして Upstream を作成し、static、wsl、hyperv のいずれかを選択します。",
          "HTTPS Listener の場合は、先に Certificates ドロワーで証明書を作成またはインポートします。",
          "保存後、キャンバス上のノード状態を確認します。赤は実行時エラー、灰色は無効状態です。"
        ]
      },
      {
        title: "server name と Hosts",
        paragraphs: [
          "Proxy はリクエストの Host / SNI による分岐だけを行い、ドメイン解決は行いません。a.com や b.com をこのマシンへ向ける必要がある場合は、Hosts モジュールで対応する hosts グループを有効にしてください。"
        ]
      },
      {
        title: "Upstream ターゲット種別",
        unordered: [
          "static：ターゲットホストとポートを手動入力します。例：127.0.0.1:3000。",
          "wsl：スキャン済みの WSL ディストリビューションを選択します。実行時に現在の IP を解決します。",
          "hyperv：スキャン済みの Hyper-V VM を選択します。実行時に現在の IP を解決します。"
        ]
      },
      {
        title: "キャンバス操作",
        unordered: [
          "ノードを左クリック：選択して右側の詳細を開きます。",
          "ノードを右クリック：コンテキストメニューを開き、編集、子ノード作成、削除を行います。",
          "マウスホイール：キャンバスをズームします。",
          "空白部分をドラッグ：ビューをパンします。",
          "Ctrl + F：キャンバス検索を開きます。Enter で次の結果へ移動します。"
        ]
      },
      {
        title: "注意事項",
        unordered: [
          "HTTPS には証明書が必要です。手動アップロードまたはローカル CA 生成を利用できます。",
          "gRPC / gRPCS には現在プロトコル制約があります。デフォルト Route を優先し、パス書き換えは避けてください。",
          "WSL / Hyper-V ターゲットはトポロジースキャン結果に依存します。リストが空の場合は、トポロジーを更新するか権限を確認してください。",
          "デフォルト Route はフォールバックに使われます。デフォルト Route も無効な場合、リクエストは拒否され、エラーログに記録されます。"
        ]
      }
    ]
  }
};

type DialogMode = "listener" | "route" | "upstream" | "certificate" | "delete" | null;
type DeleteTarget = {
  kind: "listener" | "route" | "upstream" | "certificate";
  id: string;
  name: string;
  cascadeDetail?: string;
} | null;
type EditingTarget = { kind: "listener" | "route" | "upstream" | "certificate"; id: string } | null;

function ModalShell(props: {
  open: boolean;
  title: string;
  onOpenChange: (open: boolean) => void;
  busy?: boolean;
  children: any;
  actions: any;
}) {
  return (
    <KDialog.Root
      open={props.open}
      onOpenChange={(open) => {
        if (props.busy && !open) return;
        props.onOpenChange(open);
      }}
    >
      <KDialog.Portal>
        <KDialog.Overlay class="kb-dialog-overlay" />
        <KDialog.Content
          class="kb-dialog-content close-guard-dialog"
          aria-busy={props.busy ? "true" : undefined}
        >
          <div class="panel-title">
            <KDialog.Title>{props.title}</KDialog.Title>
          </div>
          <div style={{ display: "grid", gap: "16px" }}>
            {props.children}
            <div class="row-actions" style={{ "justify-content": "flex-end" }}>
              {props.actions}
            </div>
          </div>
        </KDialog.Content>
      </KDialog.Portal>
    </KDialog.Root>
  );
}

function ProxyGuide(props: { locale: string }) {
  const content = () => proxyGuideContent[props.locale] ?? proxyGuideContent["zh-CN"];
  return (
    <div class="proxy-guide-content">
      <article class="proxy-guide-doc">
        <header class="proxy-guide-doc-header">
          <h2>{content().title}</h2>
        </header>
        <For each={content().sections}>
          {(section) => (
            <section>
              <h3>{section.title}</h3>
              <For each={section.paragraphs ?? []}>
                {(paragraph) => <p>{paragraph}</p>}
              </For>
              <Show when={section.ordered}>
                {(items) => (
                  <ol>
                    <For each={items()}>
                      {(item) => <li>{item}</li>}
                    </For>
                  </ol>
                )}
              </Show>
              <Show when={section.unordered}>
                {(items) => (
                  <ul>
                    <For each={items()}>
                      {(item) => <li>{item}</li>}
                    </For>
                  </ul>
                )}
              </Show>
            </section>
          )}
        </For>
      </article>
    </div>
  );
}

const protocolOptions: SelectOption[] = [
  { value: "http", label: "HTTP" },
  { value: "https", label: "HTTPS" }
];

const tlsModeOptions: SelectOption[] = [
  { value: "disabled", label: "disabled" },
  { value: "manual_cert", label: "manual_cert" },
  { value: "local_ca", label: "local_ca" }
];

const bindModeOptions: SelectOption[] = [
  { value: "all_nics", label: "all_nics" },
  { value: "single_nic", label: "single_nic" }
];

const LISTENER_HOST_CUSTOM = "__custom__";

const targetKindOptions: SelectOption[] = [
  { value: "static", label: "static" },
  { value: "wsl", label: "wsl" },
  { value: "hyperv", label: "hyperv" }
];

const upstreamSchemeOptions: SelectOption[] = [
  { value: "http", label: "http" },
  { value: "https", label: "https" },
  { value: "ws", label: "ws" },
  { value: "wss", label: "wss" },
  { value: "grpc", label: "grpc" },
  { value: "grpcs", label: "grpcs" }
];

const certificateSourceTypeOptions: SelectOption[] = [
  { value: "manual_upload", label: "manual_upload" },
  { value: "local_ca", label: "local_ca" }
];

function isGrpcScheme(value: UpstreamScheme) {
  return value === "grpc" || value === "grpcs";
}

function isWebSocketScheme(value: UpstreamScheme) {
  return value === "ws" || value === "wss";
}

function getUpstreamProtocolFamilyLabel(
  t: ReturnType<typeof useI18n>["t"],
  scheme: UpstreamScheme
) {
  if (isGrpcScheme(scheme)) {
    return t("proxy.protocolFamilyGrpc");
  }
  if (isWebSocketScheme(scheme)) {
    return t("proxy.protocolFamilyWebSocket");
  }
  return t("proxy.protocolFamilyHttp");
}

export function ProxyPage() {
  const { t, locale } = useI18n();
  const toast = useToast();
  const navigate = useNavigate();
  const [selectedListenerId, setSelectedListenerId] = createSignal("");
  const [selectedRouteId, setSelectedRouteId] = createSignal("");
  const [dialogMode, setDialogMode] = createSignal<DialogMode>(null);
  const [deleteTarget, setDeleteTarget] = createSignal<DeleteTarget>(null);
  const [editingTarget, setEditingTarget] = createSignal<EditingTarget>(null);
  const [migrationGuideDismissed, setMigrationGuideDismissed] = createSignal(false);
  const [dialogSubmitting, setDialogSubmitting] = createSignal(false);
  const [certificateDrawerOpen, setCertificateDrawerOpen] = createSignal(false);
  const [guideOpen, setGuideOpen] = createSignal(false);

  const [listenerName, setListenerName] = createSignal("http-listener");
  const [listenerHost, setListenerHost] = createSignal("127.0.0.1");
  const [listenerPort, setListenerPort] = createSignal("8080");
  const [listenerProtocol, setListenerProtocol] = createSignal<ProxyProtocol>("http");
  const [listenerTlsMode, setListenerTlsMode] = createSignal<ProxyTlsMode>("disabled");
  const [listenerCertId, setListenerCertId] = createSignal("");
  const [listenerBindMode, setListenerBindMode] = createSignal<BindMode>("all_nics");
  const [listenerHostSelection, setListenerHostSelection] = createSignal("127.0.0.1");
  const [listenerAllNicsHostBackup, setListenerAllNicsHostBackup] = createSignal("127.0.0.1");
  const [listenerNicId, setListenerNicId] = createSignal("");
  const [listenerEnabled, setListenerEnabled] = createSignal(true);

  const [routeServerNames, setRouteServerNames] = createSignal("");
  const [routePathPrefix, setRoutePathPrefix] = createSignal("");
  const [routeIsDefault, setRouteIsDefault] = createSignal(false);
  const [routeEnabled, setRouteEnabled] = createSignal(true);

  const [upstreamTargetKind, setUpstreamTargetKind] = createSignal<TargetKind>("static");
  const [upstreamHost, setUpstreamHost] = createSignal("127.0.0.1");
  const [upstreamTargetRef, setUpstreamTargetRef] = createSignal("");
  const [upstreamPort, setUpstreamPort] = createSignal("3000");
  const [upstreamScheme, setUpstreamScheme] = createSignal<UpstreamScheme>("http");
  const [upstreamRewriteFrom, setUpstreamRewriteFrom] = createSignal("");
  const [upstreamRewriteTo, setUpstreamRewriteTo] = createSignal("");
  const [upstreamEnabled, setUpstreamEnabled] = createSignal(true);

  const [certificateName, setCertificateName] = createSignal("dev-cert");
  const [certificateSourceType, setCertificateSourceType] =
    createSignal<ProxyCertificateSourceType>("manual_upload");
  const [certificateCertPath, setCertificateCertPath] = createSignal("");
  const [certificateKeyPath, setCertificateKeyPath] = createSignal("");
  const [certificateDomains, setCertificateDomains] = createSignal("");

  const listenersQuery = useQuery(() =>
    queryOptions<ProxyListener[]>({
      queryKey: ["proxy", "listeners"],
      queryFn: listProxyListeners,
      staleTime: 0
    })
  );

  const certificatesQuery = useQuery(() =>
    queryOptions<ProxyCertificate[]>({
      queryKey: ["proxy", "certificates"],
      queryFn: listProxyCertificates,
      staleTime: 0
    })
  );

  const routesQuery = useQuery(() =>
    queryOptions<ProxyRoute[]>({
      queryKey: ["proxy", "routes", selectedListenerId()],
      queryFn: () => listProxyRoutes(selectedListenerId()),
      enabled: selectedListenerId().length > 0,
      staleTime: 0
    })
  );

  const upstreamsQuery = useQuery(() =>
    queryOptions<ProxyUpstream[]>({
      queryKey: ["proxy", "upstreams", selectedRouteId()],
      queryFn: () => listProxyUpstreams(selectedRouteId()),
      enabled: selectedRouteId().length > 0,
      staleTime: 0
    })
  );

  const proxyRuntimeQuery = useQuery(() =>
    queryOptions<ProxyRuntimeStatusItem[]>({
      queryKey: ["proxy", "runtime"],
      queryFn: getProxyRuntimeStatus,
      refetchInterval: 5000,
      staleTime: 0
    })
  );

  const routeRuntimeQuery = useQuery(() =>
    queryOptions<ProxyRouteRuntimeItem[]>({
      queryKey: ["proxy", "route-runtime", selectedListenerId()],
      queryFn: () => listProxyRouteRuntime(selectedListenerId()),
      enabled: selectedListenerId().length > 0,
      refetchInterval: 5000,
      staleTime: 0
    })
  );

  const upstreamRuntimeQuery = useQuery(() =>
    queryOptions<ProxyUpstreamRuntimeItem[]>({
      queryKey: ["proxy", "upstream-runtime", selectedRouteId()],
      queryFn: () => listProxyUpstreamRuntime(selectedRouteId()),
      enabled: selectedRouteId().length > 0,
      refetchInterval: 5000,
      staleTime: 0
    })
  );

  const legacyRulesQuery = useQuery(() =>
    queryOptions<ProxyRule[]>({
      queryKey: ["rules", "legacy-migration-summary"],
      queryFn: listRules,
      staleTime: 0
    })
  );

  const migrationRecordsQuery = useQuery(() =>
    queryOptions<RuleMigrationRecord[]>({
      queryKey: ["rules", "migration-records", "proxy-guide"],
      queryFn: listRuleMigrations,
      staleTime: 0
    })
  );

  const topologyQuery = useQuery(() =>
    queryOptions<ProxyTopologyData>({
      queryKey: ["proxy", "topology"],
      queryFn: async () => {
        const listeners = await listProxyListeners();
        const routePairs = await Promise.all(
          listeners.map(async (listener) => [listener.id, await listProxyRoutes(listener.id)] as const)
        );
        const routesByListener = new Map(routePairs);
        const routes = routePairs.flatMap(([, items]) => items);
        const upstreamPairs = await Promise.all(
          routes.map(async (route) => [route.id, await listProxyUpstreams(route.id)] as const)
        );
        const routeRuntimePairs = await Promise.all(
          listeners.map(async (listener) => [listener.id, await listProxyRouteRuntime(listener.id)] as const)
        );
        const upstreamRuntimePairs = await Promise.all(
          routes.map(async (route) => [route.id, await listProxyUpstreamRuntime(route.id)] as const)
        );
        return {
          listeners,
          routesByListener,
          upstreamsByRoute: new Map(upstreamPairs),
          listenerRuntime: new Map(
            (await getProxyRuntimeStatus()).map((item) => [item.listener_id, item] as const)
          ),
          routeRuntime: new Map(
            routeRuntimePairs.flatMap(([, items]) => items).map((item) => [item.route_id, item] as const)
          ),
          upstreamRuntime: new Map(
            upstreamRuntimePairs
              .flatMap(([, items]) => items)
              .map((item) => [item.upstream_id, item] as const)
          )
        };
      },
      refetchInterval: 5000,
      staleTime: 0
    })
  );

  const targetTopologyQuery = useQuery(
    () => createTopologyQueryOptions(dialogMode() === "upstream" || dialogMode() === "listener"),
    () => appQueryClient
  );

  createEffect(() => {
    const listeners = listenersQuery.data ?? [];
    if (listeners.length === 0) {
      setSelectedListenerId("");
      return;
    }
    if (!listeners.some((item) => item.id === selectedListenerId())) {
      setSelectedListenerId(listeners[0].id);
    }
  });

  createEffect(() => {
    const routes = routesQuery.data ?? [];
    if (routes.length === 0) {
      setSelectedRouteId("");
      return;
    }
    if (!routes.some((item) => item.id === selectedRouteId())) {
      setSelectedRouteId(routes[0].id);
    }
  });

  createEffect(() => {
    if (listenerProtocol() === "http") {
      if (listenerTlsMode() !== "disabled") {
        setListenerTlsMode("disabled");
      }
      if (listenerCertId()) {
        setListenerCertId("");
      }
      return;
    }
    if (listenerTlsMode() === "disabled") {
      setListenerTlsMode("manual_cert");
    }
    if (listenerTlsMode() === "disabled" && listenerCertId()) {
      setListenerCertId("");
    }
  });

  const selectedListener = createMemo(
    () => (listenersQuery.data ?? []).find((item) => item.id === selectedListenerId()) ?? null
  );
  const selectedRoute = createMemo(
    () => (routesQuery.data ?? []).find((item) => item.id === selectedRouteId()) ?? null
  );
  const runtimeMap = createMemo(
    () =>
      new Map(
        (proxyRuntimeQuery.data ?? []).map((item) => [item.listener_id, item] as const)
      )
  );
  const runtimeSummary = createMemo(() => {
    const items = proxyRuntimeQuery.data ?? [];
    return {
      running: items.filter((item) => item.state === "running").length,
      error: items.filter((item) => item.state === "error").length,
      stopped: items.filter((item) => item.state === "stopped").length
    };
  });
  const topologyRouteCount = createMemo(() =>
    [...(topologyQuery.data?.routesByListener.values() ?? [])].reduce(
      (total, routes) => total + routes.length,
      0
    )
  );
  const topologyUpstreamCount = createMemo(() =>
    [...(topologyQuery.data?.upstreamsByRoute.values() ?? [])].reduce(
      (total, upstreams) => total + upstreams.length,
      0
    )
  );
  const routeRuntimeMap = createMemo(
    () => new Map((routeRuntimeQuery.data ?? []).map((item) => [item.route_id, item] as const))
  );
  const upstreamRuntimeMap = createMemo(
    () =>
      new Map((upstreamRuntimeQuery.data ?? []).map((item) => [item.upstream_id, item] as const))
  );
  const migrationSummary = createMemo(() => {
    const rules = legacyRulesQuery.data ?? [];
    const migrations = migrationRecordsQuery.data ?? [];
    const migrationMap = new Map(migrations.map((item) => [item.rule_id, item] as const));
    const legacyRules = rules.filter(
      (item) => item.type === "tcp_fwd" || item.type === "http_proxy"
    );
    const pending = legacyRules.filter(
      (item) => migrationMap.get(item.id)?.status !== "migrated"
    ).length;
    const migrated = migrations.filter((item) => item.status === "migrated").length;
    const rollbacked = migrations.filter((item) => item.status === "rollbacked").length;
    const drafts = migrations.filter(
      (item) => item.status === "migrated" && Boolean(item.detail)
    ).length;
    return {
      pending,
      migrated,
      rollbacked,
      drafts
    };
  });
  const showMigrationGuide = createMemo(() => {
    const summary = migrationSummary();
    return !migrationGuideDismissed() && (summary.pending > 0 || summary.drafts > 0);
  });
  const filteredCertificates = createMemo(() => {
    const sourceType = listenerTlsMode() === "local_ca" ? "local_ca" : "manual_upload";
    return (certificatesQuery.data ?? []).filter((item) => item.source_type === sourceType);
  });
  const certificateOptions = createMemo<SelectOption[]>(() => [
    { value: "", label: t("proxy.selectCertificate") },
    ...filteredCertificates().map((item) => ({
      value: item.id,
      label: `${item.name} (${item.domains.join(", ")})`
    }))
  ]);
  const upstreamTargetRefOptions = createMemo<SelectOption[]>(() => {
    let base: SelectOption[] = [];
    if (upstreamTargetKind() === "wsl") {
      base = (targetTopologyQuery.data?.wsl ?? []).map((item) => ({
        value: item.distro,
        label: item.ip ? `${item.distro} (${item.ip})` : item.distro
      }));
    } else if (upstreamTargetKind() === "hyperv") {
      base = (targetTopologyQuery.data?.hyperv ?? []).map((item) => ({
        value: item.vm_name,
        label: item.ip ? `${item.vm_name} (${item.ip})` : item.vm_name
      }));
    }
    return base;
  });
  const upstreamTargetPreview = createMemo(() => {
    const ref = upstreamTargetRef().trim().toLowerCase();
    if (!ref) return null;
    if (upstreamTargetKind() === "wsl") {
      return targetTopologyQuery.data?.wsl.find((item) => item.distro.toLowerCase() === ref)?.ip ?? null;
    }
    if (upstreamTargetKind() === "hyperv") {
      return targetTopologyQuery.data?.hyperv.find((item) => item.vm_name.toLowerCase() === ref)?.ip ?? null;
    }
    return null;
  });
  const listenerAdapterOptions = createMemo<SelectOption[]>(() =>
    (targetTopologyQuery.data?.adapters ?? []).map((item) => {
      const ips = [...item.ipv4, ...item.ipv6].filter(Boolean).join(", ");
      return {
        value: item.id,
        label: ips ? `${item.name} (${ips})` : item.name
      };
    })
  );
  const listenerHostOptions = createMemo<SelectOption[]>(() => {
    const values = new Set<string>(["127.0.0.1", "0.0.0.0", "::1", "::"]);
    for (const adapter of targetTopologyQuery.data?.adapters ?? []) {
      for (const ip of [...adapter.ipv4, ...adapter.ipv6]) {
        if (ip.trim()) values.add(ip.trim());
      }
    }
    const options = [...values].map((value) => ({ value, label: value }));
    options.push({ value: LISTENER_HOST_CUSTOM, label: t("proxy.listenHostCustom") });
    return options;
  });
  const selectedListenerAdapter = createMemo(() =>
    (targetTopologyQuery.data?.adapters ?? []).find((item) => item.id === listenerNicId()) ?? null
  );
  const resolvedListenerNicIp = createMemo(() => {
    const adapter = selectedListenerAdapter();
    if (!adapter) return "";
    return adapter.ipv4[0] ?? adapter.ipv6[0] ?? "";
  });

  createEffect(() => {
    if (dialogMode() !== "upstream") return;
    if (upstreamTargetKind() === "static") {
      if (upstreamTargetRef()) setUpstreamTargetRef("");
    }
  });

  createEffect(() => {
    if (dialogMode() !== "listener" || listenerBindMode() !== "single_nic") return;
    const resolvedIp = resolvedListenerNicIp();
    if (resolvedIp && listenerHost() !== resolvedIp) {
      setListenerHost(resolvedIp);
    }
  });

  createEffect(() => {
    if (dialogMode() !== "listener" || listenerBindMode() !== "all_nics") return;
    if (listenerHost().trim()) {
      setListenerAllNicsHostBackup(listenerHost().trim());
    }
  });

  function closeDialog(force = false) {
    if (dialogSubmitting() && !force) return;
    setDialogMode(null);
    setDeleteTarget(null);
    setEditingTarget(null);
  }

  async function refreshAll() {
    await listenersQuery.refetch();
    await certificatesQuery.refetch();
    await routesQuery.refetch();
    await upstreamsQuery.refetch();
    await proxyRuntimeQuery.refetch();
    await routeRuntimeQuery.refetch();
    await upstreamRuntimeQuery.refetch();
    await legacyRulesQuery.refetch();
    await migrationRecordsQuery.refetch();
    await topologyQuery.refetch();
  }

  function openCreateListenerDialog() {
    setEditingTarget(null);
    setListenerName("http-listener");
    setListenerHost("127.0.0.1");
    setListenerPort("8080");
    setListenerProtocol("http");
    setListenerTlsMode("disabled");
    setListenerCertId("");
    setListenerBindMode("all_nics");
    setListenerHostSelection("127.0.0.1");
    setListenerAllNicsHostBackup("127.0.0.1");
    setListenerNicId("");
    setListenerEnabled(true);
    setDialogMode("listener");
  }

  function openEditListenerDialog(listener: ProxyListener) {
    setEditingTarget({ kind: "listener", id: listener.id });
    setListenerName(listener.name);
    setListenerHost(listener.listen_host);
    setListenerPort(String(listener.listen_port));
    setListenerProtocol(listener.protocol);
    setListenerTlsMode(listener.tls_mode);
    setListenerCertId(listener.cert_id ?? "");
    setListenerBindMode(listener.bind_mode);
    setListenerHostSelection(
      listener.bind_mode === "all_nics" &&
      listenerHostOptions().some((option) => option.value === listener.listen_host)
        ? listener.listen_host
        : LISTENER_HOST_CUSTOM
    );
    setListenerAllNicsHostBackup(listener.listen_host);
    setListenerNicId(listener.nic_id ?? "");
    setListenerEnabled(listener.enabled);
    setDialogMode("listener");
  }

  function openCreateRouteDialog(listenerId?: string) {
    if (listenerId) {
      setSelectedListenerId(listenerId);
    }
    setEditingTarget(null);
    setRouteServerNames("");
    setRoutePathPrefix("");
    setRouteIsDefault(false);
    setRouteEnabled(true);
    setDialogMode("route");
  }

  function openEditRouteDialog(route: ProxyRoute) {
    setEditingTarget({ kind: "route", id: route.id });
    setRouteServerNames(route.server_names.join(", "));
    setRoutePathPrefix(route.path_prefix ?? "");
    setRouteIsDefault(route.is_default);
    setRouteEnabled(route.enabled);
    setDialogMode("route");
  }

  function openCreateUpstreamDialog(routeId?: string) {
    if (routeId) {
      setSelectedRouteId(routeId);
      const parentListenerId = findListenerIdForRoute(routeId);
      if (parentListenerId) setSelectedListenerId(parentListenerId);
    }
    setEditingTarget(null);
    setUpstreamTargetKind("static");
    setUpstreamHost("127.0.0.1");
    setUpstreamTargetRef("");
    setUpstreamPort("3000");
    setUpstreamScheme("http");
    setUpstreamRewriteFrom("");
    setUpstreamRewriteTo("");
    setUpstreamEnabled(true);
    setDialogMode("upstream");
  }

  function openEditUpstreamDialog(upstream: ProxyUpstream) {
    setEditingTarget({ kind: "upstream", id: upstream.id });
    setUpstreamTargetKind(upstream.target_kind);
    setUpstreamHost(upstream.target_host ?? "");
    setUpstreamTargetRef(upstream.target_ref ?? "");
    setUpstreamPort(String(upstream.target_port));
    setUpstreamScheme(upstream.upstream_scheme);
    setUpstreamRewriteFrom(upstream.path_rewrite_from ?? "");
    setUpstreamRewriteTo(upstream.path_rewrite_to ?? "");
    setUpstreamEnabled(upstream.enabled);
    setDialogMode("upstream");
  }

  function openCreateCertificateDialog() {
    setEditingTarget(null);
    setCertificateName("dev-cert");
    setCertificateSourceType("manual_upload");
    setCertificateCertPath("");
    setCertificateKeyPath("");
    setCertificateDomains("");
    setDialogMode("certificate");
  }

  function openEditCertificateDialog(certificate: ProxyCertificate) {
    setEditingTarget({ kind: "certificate", id: certificate.id });
    setCertificateName(certificate.name);
    setCertificateSourceType(certificate.source_type);
    setCertificateCertPath(certificate.cert_path);
    setCertificateKeyPath(certificate.key_path);
    setCertificateDomains(certificate.domains.join(", "));
    setDialogMode("certificate");
  }

  function handleUpstreamTargetKindChange(value: string) {
    setUpstreamTargetKind(value as TargetKind);
    setUpstreamTargetRef("");
  }

  function handleListenerBindModeChange(value: string) {
    const next = value as BindMode;
    const previous = listenerBindMode();
    setListenerBindMode(next);
    if (next === "single_nic") {
      if (previous === "all_nics" && listenerHost().trim()) {
        setListenerAllNicsHostBackup(listenerHost().trim());
      }
      if (listenerHostSelection() !== LISTENER_HOST_CUSTOM) {
        setListenerHostSelection(LISTENER_HOST_CUSTOM);
      }
      const resolvedIp = resolvedListenerNicIp();
      if (resolvedIp) {
        setListenerHost(resolvedIp);
      }
      return;
    }
    const restoredHost = listenerAllNicsHostBackup().trim() || "127.0.0.1";
    setListenerHost(restoredHost);
    if (!restoredHost) {
      setListenerHostSelection("127.0.0.1");
      return;
    }
    const matchedPreset = listenerHostOptions().some((option) => option.value === restoredHost);
    setListenerHostSelection(matchedPreset ? restoredHost : LISTENER_HOST_CUSTOM);
  }

  function handleListenerHostSelectionChange(value: string) {
    setListenerHostSelection(value);
    if (value === LISTENER_HOST_CUSTOM) {
      if (listenerHostOptions().some((option) => option.value === listenerHost())) {
        setListenerHost("");
      }
      return;
    }
    setListenerHost(value);
  }

  function handleListenerNicIdChange(value: string) {
    setListenerNicId(value);
    if (listenerBindMode() !== "single_nic") return;
    const adapter = (targetTopologyQuery.data?.adapters ?? []).find((item) => item.id === value);
    const resolvedIp = adapter?.ipv4[0] ?? adapter?.ipv6[0] ?? "";
    setListenerHost(resolvedIp);
  }

  function validateListenerForm() {
    if (!listenerName().trim()) return t("proxy.validationListenerName");
    if (listenerBindMode() === "all_nics" && !listenerHost().trim()) {
      return t("proxy.validationListenHost");
    }
    const port = Number(listenerPort());
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      return t("proxy.validationListenPort");
    }
    if (listenerBindMode() === "single_nic" && !listenerNicId().trim()) {
      return t("proxy.validationNicId");
    }
    if (listenerBindMode() === "single_nic" && !resolvedListenerNicIp()) {
      return t("proxy.validationNicIpUnavailable");
    }
    if (listenerProtocol() === "https" && listenerTlsMode() !== "disabled" && !listenerCertId()) {
      return t("proxy.validationCertificateRequired");
    }
    return null;
  }

  function validateCertificateForm() {
    if (!certificateName().trim()) return t("proxy.validationCertificateName");
    if (certificateSourceType() === "manual_upload") {
      if (!certificateCertPath().trim()) return t("proxy.validationCertificatePath");
      if (!certificateKeyPath().trim()) return t("proxy.validationCertificateKeyPath");
    }
    const domains = certificateDomains()
      .split(/[,\n]/)
      .map((item) => item.trim())
      .filter(Boolean);
    if (domains.length === 0) return t("proxy.validationCertificateDomains");
    if (domains.some((item) => !isValidServerName(item))) {
      return t("proxy.validationServerNameInvalid");
    }
    return null;
  }

  function isValidServerName(value: string) {
    if (!value || value.includes("/") || value.includes("://") || /\s/.test(value)) {
      return false;
    }
    if (value === "*") {
      return false;
    }
    return /^(\*\.)?[A-Za-z0-9.-]+$/.test(value) || /^\.[A-Za-z0-9.-]+$/.test(value);
  }

  function validateRouteForm() {
    const serverNames = routeServerNames()
      .split(/[,\n]/)
      .map((item) => item.trim())
      .filter(Boolean);
    const pathPrefix = routePathPrefix().trim();
    if (!routeIsDefault() && serverNames.length === 0) {
      return t("proxy.validationServerNamesRequired");
    }
    if (serverNames.some((item) => !isValidServerName(item))) {
      return t("proxy.validationServerNameInvalid");
    }
    if (pathPrefix && !pathPrefix.startsWith("/")) {
      return t("proxy.validationPathPrefix");
    }
    const duplicatedDefault = routeIsDefault() && (routesQuery.data ?? []).some(
      (item) =>
        item.is_default &&
        item.id !== (editingTarget()?.kind === "route" ? editingTarget()?.id : undefined)
    );
    if (duplicatedDefault) {
      return t("proxy.validationDefaultRouteUnique");
    }
    return null;
  }

  function validateUpstreamForm() {
    const port = Number(upstreamPort());
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      return t("proxy.validationTargetPort");
    }
    if (upstreamTargetKind() === "static" && !upstreamHost().trim()) {
      return t("proxy.validationTargetHost");
    }
    if (upstreamTargetKind() !== "static" && !upstreamTargetRef().trim()) {
      return t("proxy.validationTargetRef");
    }
    if (upstreamRewriteFrom().trim() && !upstreamRewriteFrom().trim().startsWith("/")) {
      return t("proxy.validationRewriteFrom");
    }
    if (upstreamRewriteTo().trim() && !upstreamRewriteTo().trim().startsWith("/")) {
      return t("proxy.validationRewriteTo");
    }
    if (upstreamRewriteTo().trim() && !upstreamRewriteFrom().trim()) {
      return t("proxy.validationRewritePair");
    }
    if (upstreamScheme() === "grpc" && selectedListener()?.protocol !== "http") {
      return t("proxy.validationGrpcNeedsHttpListener");
    }
    if (upstreamScheme() === "grpcs" && selectedListener()?.protocol !== "https") {
      return t("proxy.validationGrpcsNeedsHttpsListener");
    }
    if (upstreamScheme() === "grpc" && !selectedRoute()?.is_default) {
      return t("proxy.validationGrpcNeedsDefaultRoute");
    }
    if (upstreamScheme() === "grpc" && (upstreamRewriteFrom().trim() || upstreamRewriteTo().trim())) {
      return t("proxy.validationGrpcRewriteUnsupported");
    }
    if (upstreamScheme() === "grpcs" && !selectedRoute()?.is_default) {
      return t("proxy.validationGrpcsNeedsDefaultRoute");
    }
    if (upstreamScheme() === "grpcs" && (upstreamRewriteFrom().trim() || upstreamRewriteTo().trim())) {
      return t("proxy.validationGrpcsRewriteUnsupported");
    }
    return null;
  }

  function getListenerDialogTitle() {
    return editingTarget()?.kind === "listener"
      ? t("proxy.editListener")
      : t("proxy.newListener");
  }

  function getRouteDialogTitle() {
    return editingTarget()?.kind === "route"
      ? t("proxy.editRoute")
      : t("proxy.newRoute");
  }

  function getUpstreamDialogTitle() {
    return editingTarget()?.kind === "upstream"
      ? t("proxy.editUpstream")
      : t("proxy.newUpstream");
  }

  function getCertificateDialogTitle() {
    return editingTarget()?.kind === "certificate"
      ? t("proxy.editCertificate")
      : t("proxy.newCertificate");
  }

  async function handleSubmitListener() {
    if (dialogSubmitting()) return;
    try {
      const error = validateListenerForm();
      if (error) {
        toast.error(error);
        return;
      }
      setDialogSubmitting(true);
      const isEditing = editingTarget()?.kind === "listener";
      const listenHost =
        listenerBindMode() === "single_nic"
          ? resolvedListenerNicIp() || listenerHost()
          : listenerHost().trim();
      const req: UpdateProxyListenerRequest = {
        name: listenerName(),
        listen_host: listenHost,
        listen_port: Number(listenerPort()),
        protocol: listenerProtocol(),
        tls_mode: listenerProtocol() === "http" ? "disabled" : listenerTlsMode(),
        cert_id:
          listenerProtocol() === "https" && listenerTlsMode() !== "disabled"
            ? listenerCertId() || null
            : null,
        bind_mode: listenerBindMode(),
        nic_id: listenerNicId().trim() || null,
        enabled: listenerEnabled()
      };
      if (isEditing) {
        await updateProxyListener(editingTarget()!.id, req);
      } else {
        const id = await createProxyListener(req);
        setSelectedListenerId(id);
      }
      await refreshAll();
      closeDialog(true);
      toast.success(
        isEditing
          ? t("proxy.listenerUpdated")
          : t("proxy.listenerCreated")
      );
    } catch (error) {
      toast.error(String(error));
    } finally {
      setDialogSubmitting(false);
    }
  }

  async function handleSubmitCertificate() {
    if (dialogSubmitting()) return;
    try {
      const error = validateCertificateForm();
      if (error) {
        toast.error(error);
        return;
      }
      setDialogSubmitting(true);
      const isEditing = editingTarget()?.kind === "certificate";
      const req: CreateProxyCertificateRequest = {
        name: certificateName().trim(),
        source_type: certificateSourceType(),
        cert_path: certificateSourceType() === "manual_upload" ? certificateCertPath().trim() : "",
        key_path: certificateSourceType() === "manual_upload" ? certificateKeyPath().trim() : "",
        domains: certificateDomains()
          .split(/[,\n]/)
          .map((item) => item.trim())
          .filter(Boolean)
      };
      if (isEditing) {
        await updateProxyCertificate(editingTarget()!.id, req);
      } else {
        await createProxyCertificate(req);
      }
      await refreshAll();
      closeDialog(true);
      toast.success(
        isEditing
          ? t("proxy.certificateUpdated")
          : t("proxy.certificateCreated")
      );
    } catch (error) {
      toast.error(String(error));
    } finally {
      setDialogSubmitting(false);
    }
  }

  async function handleSubmitRoute() {
    if (!selectedListenerId()) return;
    if (dialogSubmitting()) return;
    try {
      const error = validateRouteForm();
      if (error) {
        toast.error(error);
        return;
      }
      setDialogSubmitting(true);
      const isEditing = editingTarget()?.kind === "route";
      const req: UpdateProxyRouteRequest = {
        server_names: routeServerNames()
          .split(/[,\n]/)
          .map((item) => item.trim())
          .filter(Boolean),
        path_prefix: routePathPrefix().trim() || null,
        is_default: routeIsDefault(),
        enabled: routeEnabled()
      };
      if (isEditing) {
        await updateProxyRoute(editingTarget()!.id, req);
      } else {
        const id = await createProxyRoute({
          listener_id: selectedListenerId(),
          ...req
        });
        setSelectedRouteId(id);
      }
      await refreshAll();
      closeDialog(true);
      toast.success(
        isEditing
          ? t("proxy.routeUpdated")
          : t("proxy.routeCreated")
      );
    } catch (error) {
      toast.error(String(error));
    } finally {
      setDialogSubmitting(false);
    }
  }

  async function handleSubmitUpstream() {
    if (!selectedRouteId()) return;
    if (dialogSubmitting()) return;
    try {
      const error = validateUpstreamForm();
      if (error) {
        toast.error(error);
        return;
      }
      setDialogSubmitting(true);
      const isEditing = editingTarget()?.kind === "upstream";
      const req: UpdateProxyUpstreamRequest = {
        route_id: selectedRouteId(),
        target_kind: upstreamTargetKind(),
        target_ref: upstreamTargetRef().trim() || null,
        target_host: upstreamTargetKind() === "static" ? upstreamHost().trim() || null : null,
        target_port: Number(upstreamPort()),
        upstream_scheme: upstreamScheme(),
        path_rewrite_from: upstreamRewriteFrom().trim() || null,
        path_rewrite_to: upstreamRewriteTo().trim() || null,
        enabled: upstreamEnabled()
      };
      if (isEditing) {
        await updateProxyUpstream(editingTarget()!.id, req);
      } else {
        await createProxyUpstream(req);
      }
      await refreshAll();
      closeDialog(true);
      toast.success(
        isEditing
          ? t("proxy.upstreamUpdated")
          : t("proxy.upstreamCreated")
      );
    } catch (error) {
      toast.error(String(error));
    } finally {
      setDialogSubmitting(false);
    }
  }

  function askDelete(
    kind: "listener" | "route" | "upstream" | "certificate",
    id: string,
    name: string
  ) {
    setDeleteTarget({ kind, id, name, cascadeDetail: getDeleteCascadeDetail(kind, id) });
    setDialogMode("delete");
  }

  function getDeleteCascadeDetail(
    kind: "listener" | "route" | "upstream" | "certificate",
    id: string
  ) {
    if (kind === "listener") {
      const routes = topologyQuery.data?.routesByListener.get(id) ?? [];
      const upstreamCount = routes.reduce(
        (total, route) => total + (topologyQuery.data?.upstreamsByRoute.get(route.id)?.length ?? 0),
        0
      );
      if (routes.length === 0 && upstreamCount === 0) return "";
      return t("proxy.deleteCascadeListener", {
        routes: routes.length,
        upstreams: upstreamCount
      });
    }
    if (kind === "route") {
      const upstreams = topologyQuery.data?.upstreamsByRoute.get(id) ?? [];
      if (upstreams.length === 0) return "";
      return t("proxy.deleteCascadeRoute", { upstreams: upstreams.length });
    }
    return "";
  }

  function findListenerIdForRoute(routeId: string) {
    for (const [listenerId, routes] of topologyQuery.data?.routesByListener ?? new Map<string, ProxyRoute[]>()) {
      if (routes.some((route) => route.id === routeId)) return listenerId;
    }
    return "";
  }

  function handleCanvasSelectedNodeChange(node: SelectedProxyNode) {
    if (!node) return;
    if (node.kind === "listener") {
      setSelectedListenerId(node.id);
      return;
    }
    if (node.kind === "route") {
      setSelectedRouteId(node.id);
      const listenerId = findListenerIdForRoute(node.id);
      if (listenerId) setSelectedListenerId(listenerId);
      return;
    }
    const upstream = [...(topologyQuery.data?.upstreamsByRoute.entries() ?? [])]
      .find(([, upstreams]) => upstreams.some((item) => item.id === node.id));
    if (upstream) {
      const [routeId] = upstream;
      setSelectedRouteId(routeId);
      const listenerId = findListenerIdForRoute(routeId);
      if (listenerId) setSelectedListenerId(listenerId);
    }
  }

  async function browseCertificateCertPath() {
    const selected = await openDialog({
      multiple: false,
      directory: false,
      filters: [{ name: "Certificate", extensions: ["pem", "crt", "cer"] }]
    });
    if (typeof selected === "string") {
      setCertificateCertPath(selected);
    }
  }

  async function browseCertificateKeyPath() {
    const selected = await openDialog({
      multiple: false,
      directory: false,
      filters: [{ name: "Private Key", extensions: ["pem", "key"] }]
    });
    if (typeof selected === "string") {
      setCertificateKeyPath(selected);
    }
  }

  async function handleDelete() {
    const target = deleteTarget();
    if (!target) return;
    if (dialogSubmitting()) return;
    try {
      setDialogSubmitting(true);
      if (target.kind === "listener") {
        await deleteProxyListener(target.id);
      } else if (target.kind === "route") {
        await deleteProxyRoute(target.id);
      } else if (target.kind === "certificate") {
        await deleteProxyCertificate(target.id);
      } else {
        await deleteProxyUpstream(target.id);
      }
      await refreshAll();
      closeDialog(true);
      toast.success(t("proxy.deleted"));
    } catch (error) {
      toast.error(String(error));
    } finally {
      setDialogSubmitting(false);
    }
  }

  return (
    <div class="proxy-page-shell">
      <div class="proxy-page-titlebar">
        <PageHeader title={t("proxy.title")} />
      </div>
      <Show when={false}>
        <div class="metric-grid">
          <MetricCard label={t("proxy.listenerMetric")} value={String((topologyQuery.data?.listeners ?? listenersQuery.data ?? []).length)} />
          <MetricCard label={t("proxy.certificateMetric")} value={String((certificatesQuery.data ?? []).length)} />
          <MetricCard label={t("proxy.routeMetric")} value={String(topologyRouteCount())} />
          <MetricCard label={t("proxy.upstreamMetric")} value={String(topologyUpstreamCount())} />
          <MetricCard label={t("common.running")} value={String(runtimeSummary().running)} detail={t("proxy.runtimeDetail", { error: runtimeSummary().error, stopped: runtimeSummary().stopped })} />
        </div>
      </Show>

      <div class="proxy-canvas-host">
        <Show when={showMigrationGuide()}>
          <div class="proxy-migration-float">
            <div class="proxy-migration-float-header">
              <div class="proxy-migration-float-body">
                <strong class="proxy-migration-float-title">{t("proxy.migrationGuideTitle")}</strong>
                <span class="proxy-migration-float-text">
                  {t("proxy.migrationGuideSummary", {
                    pending: migrationSummary().pending,
                    migrated: migrationSummary().migrated,
                    rollbacked: migrationSummary().rollbacked
                  })}
                </span>
                <Show when={migrationSummary().drafts > 0}>
                  <span class="proxy-migration-float-text">
                    {t("proxy.migrationGuideDrafts", { count: migrationSummary().drafts })}
                  </span>
                </Show>
              </div>
              <ActionButton size="small" onClick={() => setMigrationGuideDismissed(true)}>
                {t("proxy.migrationGuideDismiss")}
              </ActionButton>
            </div>
            <div class="proxy-migration-float-actions">
              <ActionButton size="small" onClick={() => navigate({ to: "/rules" })}>
                {t("proxy.migrationGuideOpenRules")}
              </ActionButton>
              <ActionButton size="small" onClick={() => void refreshAll()}>
                {t("common.refresh")}
              </ActionButton>
            </div>
          </div>
        </Show>
        <ProxyCanvas
          listeners={topologyQuery.data?.listeners ?? []}
          routesByListener={topologyQuery.data?.routesByListener ?? new Map()}
          upstreamsByRoute={topologyQuery.data?.upstreamsByRoute ?? new Map()}
          listenerRuntime={topologyQuery.data?.listenerRuntime ?? new Map()}
          routeRuntime={topologyQuery.data?.routeRuntime ?? new Map()}
          upstreamRuntime={topologyQuery.data?.upstreamRuntime ?? new Map()}
          loading={topologyQuery.isLoading}
          onRefresh={() => void refreshAll()}
          onOpenCertificates={() => setCertificateDrawerOpen(true)}
          onOpenGuide={() => setGuideOpen(true)}
          onCreateListener={openCreateListenerDialog}
          onCreateRoute={(listenerId) => openCreateRouteDialog(listenerId)}
          onCreateUpstream={(routeId) => openCreateUpstreamDialog(routeId)}
          onEditListener={openEditListenerDialog}
          onEditRoute={openEditRouteDialog}
          onEditUpstream={openEditUpstreamDialog}
          onDeleteNode={(node) => {
            if (node.kind === "listener") {
              askDelete("listener", node.id, (node.source as ProxyListener).name);
            } else if (node.kind === "route") {
              const route = node.source as ProxyRoute;
              askDelete("route", node.id, route.server_names.join(", ") || t("proxy.defaultRoute"));
            } else {
              const upstream = node.source as ProxyUpstream;
              askDelete(
                "upstream",
                node.id,
                `${upstream.target_host ?? upstream.target_ref ?? "-"}:${upstream.target_port}`
              );
            }
          }}
          onSelectedNodeChange={handleCanvasSelectedNodeChange}
        />
      </div>

      <BottomDrawer
        open={certificateDrawerOpen()}
        title={t("proxy.certificatesTitle")}
        subtitle={t("proxy.certificatesSubtitle")}
        onOpenChange={setCertificateDrawerOpen}
        actions={
          <>
            <ActionButton variant="primary" onClick={openCreateCertificateDialog}>
              {t("proxy.newCertificate")}
            </ActionButton>
            <ActionButton onClick={() => setCertificateDrawerOpen(false)}>
              {t("common.close")}
            </ActionButton>
          </>
        }
      >
        <div class="proxy-cert-drawer-list">
          <For each={certificatesQuery.data ?? []}>
            {(certificate) => (
              <div class="proxy-cert-card">
                <div class="proxy-cert-card-header">
                  <strong>{certificate.name}</strong>
                  <StatusBadge state="ready" label={certificate.source_type} />
                </div>
                <div class="kv-grid">
                  <span>{certificate.domains.join(", ")}</span>
                </div>
                <div class="proxy-cert-paths">
                  <span>{certificate.cert_path}</span>
                  <span>{certificate.key_path}</span>
                </div>
                <div class="row-actions" style={{ "justify-content": "flex-end" }}>
                  <ActionButton onClick={() => openEditCertificateDialog(certificate)}>
                    {t("proxy.edit")}
                  </ActionButton>
                  <ActionButton
                    variant="danger"
                    onClick={() =>
                      askDelete("certificate", certificate.id, certificate.name)
                    }
                  >
                    {t("proxy.delete")}
                  </ActionButton>
                </div>
              </div>
            )}
          </For>
          <Show when={(certificatesQuery.data ?? []).length === 0}>
            <div class="panel panel-muted">{t("proxy.emptyCertificates")}</div>
          </Show>
        </div>
      </BottomDrawer>

      <ModalShell
        open={dialogMode() === "listener"}
        title={getListenerDialogTitle()}
        onOpenChange={(open) => !open && closeDialog()}
        busy={dialogSubmitting()}
        actions={
          <>
            <ActionButton disabled={dialogSubmitting()} onClick={() => closeDialog()}>
              {t("common.close")}
            </ActionButton>
            <ActionButton
              variant="primary"
              loading={dialogSubmitting() && dialogMode() === "listener"}
              disabled={dialogSubmitting()}
              onClick={handleSubmitListener}
            >
              {editingTarget()?.kind === "listener" ? t("proxy.save") : t("proxy.create")}
            </ActionButton>
          </>
        }
      >
        <TextFieldControl label={t("proxy.listenerName")} value={listenerName()} onChange={setListenerName} />
        <div class="form-grid" style={{ "grid-template-columns": "1fr 160px" }}>
          <Show
            when={listenerBindMode() === "all_nics"}
            fallback={
              <TextFieldControl
                label={t("proxy.listenHostResolved")}
                value={resolvedListenerNicIp() || t("proxy.nicIpUnavailable")}
                onChange={() => undefined}
                disabled
              />
            }
          >
            <div class="kb-field">
              <span class="kb-label">{t("proxy.listenHost")}</span>
              <div class="proxy-listen-host-grid">
                <SimpleSelect
                  value={listenerHostSelection()}
                  onChange={handleListenerHostSelectionChange}
                  options={listenerHostOptions()}
                />
                <Show when={listenerHostSelection() === LISTENER_HOST_CUSTOM}>
                  <KTextField.Root class="kb-field" value={listenerHost()} onChange={setListenerHost}>
                    <KTextField.Input
                      class="kb-input"
                      value={listenerHost()}
                      placeholder={t("proxy.listenHostCustomPlaceholder")}
                    />
                  </KTextField.Root>
                </Show>
              </div>
            </div>
          </Show>
          <TextFieldControl label={t("proxy.listenPort")} value={listenerPort()} onChange={setListenerPort} />
        </div>
        <div class="form-grid" style={{ "grid-template-columns": listenerProtocol() === "https" ? "1fr 1fr" : "1fr" }}>
          <SelectField label={t("proxy.protocol")} value={listenerProtocol()} onChange={(value) => setListenerProtocol(value as ProxyProtocol)} options={protocolOptions} />
          <Show when={listenerProtocol() === "https"}>
            <SelectField
              label={t("proxy.tlsMode")}
              value={listenerTlsMode()}
              onChange={(value) => setListenerTlsMode(value as ProxyTlsMode)}
              options={tlsModeOptions}
            />
          </Show>
        </div>
        <Show when={listenerProtocol() === "https" && listenerTlsMode() !== "disabled"}>
          <SelectField
            label={t("proxy.boundCertificate")}
            value={listenerCertId()}
            onChange={setListenerCertId}
            options={certificateOptions()}
          />
        </Show>
        <Show when={listenerProtocol() === "https" && listenerTlsMode() === "local_ca"}>
          <Hint variant="info">{t("proxy.localCaListenerHint")}</Hint>
        </Show>
        <div class="form-grid" style={{ "grid-template-columns": "1fr 1fr" }}>
          <SelectField
            label={t("proxy.bindMode")}
            value={listenerBindMode()}
            onChange={handleListenerBindModeChange}
            options={bindModeOptions}
          />
          <Show
            when={listenerBindMode() === "single_nic"}
            fallback={
              <TextFieldControl
                label={t("proxy.nicId")}
                value={t("proxy.nicNotRequired")}
                onChange={() => undefined}
                disabled
              />
            }
          >
            <SelectField
              label={t("proxy.nicId")}
              value={listenerNicId()}
              onChange={handleListenerNicIdChange}
              options={
                listenerAdapterOptions().length > 0
                  ? [{ value: "", label: t("proxy.selectNicId") }, ...listenerAdapterOptions()]
                  : [{ value: "", label: t("proxy.noNicOptions") }]
              }
              disabled={listenerAdapterOptions().length === 0}
            />
          </Show>
        </div>
        <Show when={listenerBindMode() === "single_nic"}>
          <Hint variant="info">{t("proxy.listenHostManagedByNic")}</Hint>
        </Show>
        <CheckboxField label={t("common.enabled")} checked={listenerEnabled()} onChange={setListenerEnabled} />
      </ModalShell>

      <ModalShell
        open={dialogMode() === "route"}
        title={getRouteDialogTitle()}
        onOpenChange={(open) => !open && closeDialog()}
        busy={dialogSubmitting()}
        actions={
          <>
            <ActionButton disabled={dialogSubmitting()} onClick={() => closeDialog()}>
              {t("common.close")}
            </ActionButton>
            <ActionButton
              variant="primary"
              loading={dialogSubmitting() && dialogMode() === "route"}
              disabled={dialogSubmitting()}
              onClick={handleSubmitRoute}
            >
              {editingTarget()?.kind === "route" ? t("proxy.save") : t("proxy.create")}
            </ActionButton>
          </>
        }
      >
        <TextFieldControl
          label={t("proxy.serverNames")}
          value={routeServerNames()}
          onChange={setRouteServerNames}
          placeholder={t("proxy.serverNamesPlaceholder")}
        />
        <TextFieldControl
          label={t("proxy.pathPrefix")}
          value={routePathPrefix()}
          onChange={setRoutePathPrefix}
          placeholder="/"
        />
        <CheckboxField label={t("proxy.defaultRoute")} checked={routeIsDefault()} onChange={setRouteIsDefault} />
        <CheckboxField label={t("common.enabled")} checked={routeEnabled()} onChange={setRouteEnabled} />
      </ModalShell>

      <ModalShell
        open={dialogMode() === "upstream"}
        title={getUpstreamDialogTitle()}
        onOpenChange={(open) => !open && closeDialog()}
        busy={dialogSubmitting()}
        actions={
          <>
            <ActionButton disabled={dialogSubmitting()} onClick={() => closeDialog()}>
              {t("common.close")}
            </ActionButton>
            <ActionButton
              variant="primary"
              loading={dialogSubmitting() && dialogMode() === "upstream"}
              disabled={dialogSubmitting()}
              onClick={handleSubmitUpstream}
            >
              {editingTarget()?.kind === "upstream" ? t("proxy.save") : t("proxy.create")}
            </ActionButton>
          </>
        }
      >
        <div class="form-grid" style={{ "grid-template-columns": "1fr 1fr" }}>
          <SelectField
            label={t("proxy.targetKind")}
            value={upstreamTargetKind()}
            onChange={handleUpstreamTargetKindChange}
            options={targetKindOptions}
          />
          <SelectField
            label={t("proxy.upstreamScheme")}
            value={upstreamScheme()}
            onChange={(value) => setUpstreamScheme(value as UpstreamScheme)}
            options={upstreamSchemeOptions}
          />
        </div>
        <Show when={isGrpcScheme(upstreamScheme())}>
          <Hint variant="info">{t("proxy.grpcPendingHint")}</Hint>
        </Show>
        <Show when={upstreamScheme() === "grpc"}>
          <Hint variant="info">{t("proxy.grpcHttpListenerHint")}</Hint>
        </Show>
        <Show when={upstreamScheme() === "grpcs"}>
          <Hint variant="info">{t("proxy.grpcsHttpsListenerHint")}</Hint>
        </Show>
        <Show when={upstreamScheme() === "grpc"}>
          <Hint variant="info">{t("proxy.grpcDefaultRouteHint")}</Hint>
        </Show>
        <Show when={upstreamScheme() === "grpcs"}>
          <Hint variant="info">{t("proxy.grpcsDefaultRouteHint")}</Hint>
        </Show>
        <Show
          when={upstreamTargetKind() === "wsl" || upstreamTargetKind() === "hyperv"}
          fallback={
            <div class="form-grid" style={{ "grid-template-columns": "1fr 160px" }}>
              <TextFieldControl label={t("proxy.targetHost")} value={upstreamHost()} onChange={setUpstreamHost} />
              <TextFieldControl label={t("proxy.targetPort")} value={upstreamPort()} onChange={setUpstreamPort} />
            </div>
          }
        >
          <div class="form-grid" style={{ "grid-template-columns": "1fr 160px" }}>
            <SelectField
              label={t("proxy.targetRef")}
              value={upstreamTargetRef()}
              onChange={setUpstreamTargetRef}
              options={upstreamTargetRefOptions()}
              disabled={upstreamTargetRefOptions().length === 0}
            />
            <TextFieldControl label={t("proxy.targetPort")} value={upstreamPort()} onChange={setUpstreamPort} />
          </div>
        </Show>
        <Show when={upstreamTargetKind() === "wsl" || upstreamTargetKind() === "hyperv"}>
          <Hint variant="info">
            {t("proxy.targetResolvedIp")}: {upstreamTargetPreview() ?? t("proxy.targetNotResolved")}
          </Hint>
        </Show>
        <div class="form-grid" style={{ "grid-template-columns": "1fr 1fr" }}>
          <TextFieldControl label={t("proxy.rewriteFrom")} value={upstreamRewriteFrom()} onChange={setUpstreamRewriteFrom} />
          <TextFieldControl label={t("proxy.rewriteTo")} value={upstreamRewriteTo()} onChange={setUpstreamRewriteTo} />
        </div>
        <CheckboxField label={t("common.enabled")} checked={upstreamEnabled()} onChange={setUpstreamEnabled} />
      </ModalShell>

      <ModalShell
        open={dialogMode() === "certificate"}
        title={getCertificateDialogTitle()}
        onOpenChange={(open) => !open && closeDialog()}
        busy={dialogSubmitting()}
        actions={
          <>
            <ActionButton disabled={dialogSubmitting()} onClick={() => closeDialog()}>
              {t("common.close")}
            </ActionButton>
            <ActionButton
              variant="primary"
              loading={dialogSubmitting() && dialogMode() === "certificate"}
              disabled={dialogSubmitting()}
              onClick={handleSubmitCertificate}
            >
              {editingTarget()?.kind === "certificate" ? t("proxy.save") : t("proxy.create")}
            </ActionButton>
          </>
        }
      >
        <TextFieldControl label={t("proxy.certificateName")} value={certificateName()} onChange={setCertificateName} />
        <SelectField
          label={t("proxy.certificateSourceType")}
          value={certificateSourceType()}
          onChange={(value) => setCertificateSourceType(value as ProxyCertificateSourceType)}
          options={certificateSourceTypeOptions}
        />
        <Show when={certificateSourceType() === "manual_upload"}>
          <KTextFieldLike
            label={t("proxy.certificatePath")}
            value={certificateCertPath()}
            onChange={setCertificateCertPath}
            onBrowse={() => void browseCertificateCertPath()}
            browseLabel={t("hosts.browse")}
            disabled={dialogSubmitting()}
          />
          <KTextFieldLike
            label={t("proxy.certificateKeyPath")}
            value={certificateKeyPath()}
            onChange={setCertificateKeyPath}
            onBrowse={() => void browseCertificateKeyPath()}
            browseLabel={t("hosts.browse")}
            disabled={dialogSubmitting()}
          />
        </Show>
        <TextFieldControl
          label={t("proxy.certificateDomains")}
          value={certificateDomains()}
          onChange={setCertificateDomains}
          placeholder={t("proxy.certificateDomainsPlaceholder")}
        />
        <Show when={certificateSourceType() === "local_ca"}>
          <Hint variant="info">{t("proxy.localCaGenerateHint")}</Hint>
        </Show>
      </ModalShell>

      <ModalShell
        open={guideOpen()}
        title={t("proxy.guideTitle")}
        onOpenChange={(open) => !open && setGuideOpen(false)}
        actions={
          <ActionButton onClick={() => setGuideOpen(false)}>
            {t("common.close")}
          </ActionButton>
        }
      >
        <ProxyGuide locale={locale()} />
      </ModalShell>

      <ModalShell
        open={dialogMode() === "delete"}
        title={t("proxy.delete")}
        onOpenChange={(open) => !open && closeDialog()}
        busy={dialogSubmitting()}
        actions={
          <>
            <ActionButton disabled={dialogSubmitting()} onClick={() => closeDialog()}>
              {t("common.close")}
            </ActionButton>
            <ActionButton
              variant="danger"
              loading={dialogSubmitting() && dialogMode() === "delete"}
              disabled={dialogSubmitting()}
              onClick={handleDelete}
            >
              {t("proxy.confirmDelete")}
            </ActionButton>
          </>
        }
      >
        <div style={{ display: "grid", gap: "10px" }}>
          <div>{t("proxy.deletePrompt", { name: deleteTarget()?.name ?? "-" })}</div>
          <Show when={deleteTarget()?.cascadeDetail}>
            {(detail) => <Hint variant="warn">{detail()}</Hint>}
          </Show>
        </div>
      </ModalShell>
    </div>
  );
}

function ProtocolFamilyBadge(props: { label: string }) {
  return (
    <span
      style={{
        display: "inline-flex",
        "align-items": "center",
        padding: "2px 8px",
        "border-radius": "999px",
        "font-size": "11px",
        "line-height": 1.4,
        "background-color": "rgba(53, 116, 240, 0.12)",
        color: "rgb(53, 116, 240)"
      }}
    >
      {props.label}
    </span>
  );
}

function ListenerStatusBadge(props: {
  listener: ProxyListener;
  runtime: ProxyRuntimeStatusItem | undefined;
  t: ReturnType<typeof useI18n>["t"];
}) {
  const state = (): RuntimeState | "unknown" => {
    if (!props.listener.enabled) return "stopped";
    return props.runtime?.state ?? "unknown";
  };
  const label = () => {
    const value = state();
    return value === "unknown" ? props.t("common.ready") : props.t(`common.${value}`);
  };
  return <StatusBadge state={state()} label={label()} />;
}

function ListenerRuntimeStatus(props: {
  listenerId: string;
  runtimeMap: Map<string, ProxyRuntimeStatusItem>;
  t: ReturnType<typeof useI18n>["t"];
}) {
  const runtime = () => props.runtimeMap.get(props.listenerId);
  return (
    <Show when={runtime()?.last_error}>
      {(message) => (
        <div class="muted" style={{ "font-size": "12px" }}>
          {props.t("proxy.lastError")}: {message()}
        </div>
      )}
    </Show>
  );
}

function RouteRuntimeSummary(props: {
  runtime: ProxyRouteRuntimeItem | undefined;
  t: ReturnType<typeof useI18n>["t"];
}) {
  return (
    <div class="kv-grid muted" style={{ "font-size": "12px" }}>
      <span>
        {props.t("proxy.hitCount")}: {props.runtime?.hit_count ?? 0}
      </span>
      <span>
        {props.t("proxy.errorCount")}: {props.runtime?.error_count ?? 0}
      </span>
      <span>
        {props.t("proxy.lastServerName")}: {props.runtime?.last_server_name ?? props.t("common.none")}
      </span>
      <span>
        {props.t("proxy.lastRequestPath")}: {props.runtime?.last_request_path ?? props.t("common.none")}
      </span>
    </div>
  );
}

function UpstreamRuntimeSummary(props: {
  runtime: ProxyUpstreamRuntimeItem | undefined;
  t: ReturnType<typeof useI18n>["t"];
}) {
  return (
    <div class="kv-grid muted" style={{ "font-size": "12px" }}>
      <span>
        {props.t("proxy.hitCount")}: {props.runtime?.hit_count ?? 0}
      </span>
      <span>
        {props.t("proxy.errorCount")}: {props.runtime?.error_count ?? 0}
      </span>
      <span>
        {props.t("proxy.lastTarget")}: {props.runtime?.last_target ?? props.t("common.none")}
      </span>
      <span>
        {props.t("proxy.lastRequestPath")}: {props.runtime?.last_request_path ?? props.t("common.none")}
      </span>
    </div>
  );
}

function SelectField(props: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: SelectOption[];
  disabled?: boolean;
}) {
  return (
    <div class="kb-field">
      <span class="kb-label">{props.label}</span>
      <SimpleSelect
        value={props.value}
        onChange={props.onChange}
        options={props.options}
        disabled={props.disabled}
      />
    </div>
  );
}

function KTextFieldLike(props: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  onBrowse: () => void;
  browseLabel: string;
  disabled?: boolean;
}) {
  return (
    <KTextField.Root class="kb-field" value={props.value} onChange={props.onChange}>
      <KTextField.Label class="kb-label">{props.label}</KTextField.Label>
      <div class="row-actions">
        <KTextField.Input class="kb-input" value={props.value} disabled={props.disabled} />
        <KButton.Root class="kb-btn ghost" onClick={props.onBrowse} disabled={props.disabled}>
          {props.browseLabel}
        </KButton.Root>
      </div>
    </KTextField.Root>
  );
}
