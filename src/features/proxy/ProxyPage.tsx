import { open as openDialog } from "@tauri-apps/plugin-dialog";
import * as KButton from "@kobalte/core/button";
import * as KDialog from "@kobalte/core/dialog";
import * as KTextField from "@kobalte/core/text-field";
import { queryOptions, useQuery } from "@tanstack/solid-query";
import { useNavigate } from "@tanstack/solid-router";
import { createEffect, createMemo, createSignal, For, Show } from "solid-js";

import { useI18n } from "../../i18n/context";
import { Hint } from "../../lib/Hint";
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

type DialogMode = "listener" | "route" | "upstream" | "certificate" | "delete" | null;
type DeleteTarget = { kind: "listener" | "route" | "upstream" | "certificate"; id: string; name: string } | null;
type EditingTarget = { kind: "listener" | "route" | "upstream" | "certificate"; id: string } | null;

function ModalShell(props: {
  open: boolean;
  title: string;
  onOpenChange: (open: boolean) => void;
  children: any;
  actions: any;
}) {
  return (
    <KDialog.Root open={props.open} onOpenChange={props.onOpenChange}>
      <KDialog.Portal>
        <KDialog.Overlay class="kb-dialog-overlay" />
        <KDialog.Content class="kb-dialog-content close-guard-dialog">
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
  const { t } = useI18n();
  const toast = useToast();
  const navigate = useNavigate();
  const [selectedListenerId, setSelectedListenerId] = createSignal("");
  const [selectedRouteId, setSelectedRouteId] = createSignal("");
  const [dialogMode, setDialogMode] = createSignal<DialogMode>(null);
  const [deleteTarget, setDeleteTarget] = createSignal<DeleteTarget>(null);
  const [editingTarget, setEditingTarget] = createSignal<EditingTarget>(null);
  const [migrationGuideDismissed, setMigrationGuideDismissed] = createSignal(false);

  const [listenerName, setListenerName] = createSignal("http-listener");
  const [listenerHost, setListenerHost] = createSignal("127.0.0.1");
  const [listenerPort, setListenerPort] = createSignal("8080");
  const [listenerProtocol, setListenerProtocol] = createSignal<ProxyProtocol>("http");
  const [listenerTlsMode, setListenerTlsMode] = createSignal<ProxyTlsMode>("disabled");
  const [listenerCertId, setListenerCertId] = createSignal("");
  const [listenerBindMode, setListenerBindMode] = createSignal<BindMode>("all_nics");
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

  function closeDialog() {
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
    setListenerNicId(listener.nic_id ?? "");
    setListenerEnabled(listener.enabled);
    setDialogMode("listener");
  }

  function openCreateRouteDialog() {
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

  function openCreateUpstreamDialog() {
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

  function validateListenerForm() {
    if (!listenerName().trim()) return t("proxy.validationListenerName");
    if (!listenerHost().trim()) return t("proxy.validationListenHost");
    const port = Number(listenerPort());
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      return t("proxy.validationListenPort");
    }
    if (listenerBindMode() === "single_nic" && !listenerNicId().trim()) {
      return t("proxy.validationNicId");
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
    try {
      const error = validateListenerForm();
      if (error) {
        toast.error(error);
        return;
      }
      const req: UpdateProxyListenerRequest = {
        name: listenerName(),
        listen_host: listenerHost(),
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
      if (editingTarget()?.kind === "listener") {
        await updateProxyListener(editingTarget()!.id, req);
      } else {
        const id = await createProxyListener(req);
        setSelectedListenerId(id);
      }
      await refreshAll();
      closeDialog();
      toast.success(
        editingTarget()?.kind === "listener"
          ? t("proxy.listenerUpdated")
          : t("proxy.listenerCreated")
      );
    } catch (error) {
      toast.error(String(error));
    }
  }

  async function handleSubmitCertificate() {
    try {
      const error = validateCertificateForm();
      if (error) {
        toast.error(error);
        return;
      }
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
      if (editingTarget()?.kind === "certificate") {
        await updateProxyCertificate(editingTarget()!.id, req);
      } else {
        await createProxyCertificate(req);
      }
      await refreshAll();
      closeDialog();
      toast.success(
        editingTarget()?.kind === "certificate"
          ? t("proxy.certificateUpdated")
          : t("proxy.certificateCreated")
      );
    } catch (error) {
      toast.error(String(error));
    }
  }

  async function handleSubmitRoute() {
    if (!selectedListenerId()) return;
    try {
      const error = validateRouteForm();
      if (error) {
        toast.error(error);
        return;
      }
      const req: UpdateProxyRouteRequest = {
        server_names: routeServerNames()
          .split(/[,\n]/)
          .map((item) => item.trim())
          .filter(Boolean),
        path_prefix: routePathPrefix().trim() || null,
        is_default: routeIsDefault(),
        enabled: routeEnabled()
      };
      if (editingTarget()?.kind === "route") {
        await updateProxyRoute(editingTarget()!.id, req);
      } else {
        const id = await createProxyRoute({
          listener_id: selectedListenerId(),
          ...req
        });
        setSelectedRouteId(id);
      }
      await refreshAll();
      closeDialog();
      toast.success(
        editingTarget()?.kind === "route"
          ? t("proxy.routeUpdated")
          : t("proxy.routeCreated")
      );
    } catch (error) {
      toast.error(String(error));
    }
  }

  async function handleSubmitUpstream() {
    if (!selectedRouteId()) return;
    try {
      const error = validateUpstreamForm();
      if (error) {
        toast.error(error);
        return;
      }
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
      if (editingTarget()?.kind === "upstream") {
        await updateProxyUpstream(editingTarget()!.id, req);
      } else {
        await createProxyUpstream(req);
      }
      await refreshAll();
      closeDialog();
      toast.success(
        editingTarget()?.kind === "upstream"
          ? t("proxy.upstreamUpdated")
          : t("proxy.upstreamCreated")
      );
    } catch (error) {
      toast.error(String(error));
    }
  }

  function askDelete(
    kind: "listener" | "route" | "upstream" | "certificate",
    id: string,
    name: string
  ) {
    setDeleteTarget({ kind, id, name });
    setDialogMode("delete");
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
    try {
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
      closeDialog();
      toast.success(t("proxy.deleted"));
    } catch (error) {
      toast.error(String(error));
    }
  }

  return (
    <div style={{ display: "grid", gap: "20px" }}>
      <PageHeader
        title={t("proxy.title")}
        eyebrow={t("nav.proxy")}
        actions={
          <div class="row-actions">
            <ActionButton onClick={() => refreshAll()}>
              {t("common.refresh")}
            </ActionButton>
            <ActionButton variant="primary" onClick={openCreateListenerDialog}>
              {t("proxy.newListener")}
            </ActionButton>
          </div>
        }
      />

      <div class="metric-grid">
        <MetricCard label={t("proxy.listenerMetric")} value={String((listenersQuery.data ?? []).length)} />
        <MetricCard label={t("proxy.certificateMetric")} value={String((certificatesQuery.data ?? []).length)} />
        <MetricCard label={t("proxy.routeMetric")} value={String((routesQuery.data ?? []).length)} />
        <MetricCard label={t("proxy.upstreamMetric")} value={String((upstreamsQuery.data ?? []).length)} />
        <MetricCard label={t("common.running")} value={String(runtimeSummary().running)} detail={t("proxy.runtimeDetail", { error: runtimeSummary().error, stopped: runtimeSummary().stopped })} />
      </div>

      <Show when={showMigrationGuide()}>
        <Hint variant="info">
          <div style={{ display: "grid", gap: "12px" }}>
            <div style={{ display: "grid", gap: "4px" }}>
              <strong>{t("proxy.migrationGuideTitle")}</strong>
              <span>
                {t("proxy.migrationGuideSummary", {
                  pending: migrationSummary().pending,
                  migrated: migrationSummary().migrated,
                  rollbacked: migrationSummary().rollbacked
                })}
              </span>
              <Show when={migrationSummary().drafts > 0}>
                <span>{t("proxy.migrationGuideDrafts", { count: migrationSummary().drafts })}</span>
              </Show>
            </div>
            <div class="row-actions">
              <ActionButton onClick={() => navigate({ to: "/rules" })}>
                {t("proxy.migrationGuideOpenRules")}
              </ActionButton>
              <ActionButton onClick={() => void refreshAll()}>
                {t("common.refresh")}
              </ActionButton>
              <ActionButton onClick={() => setMigrationGuideDismissed(true)}>
                {t("proxy.migrationGuideDismiss")}
              </ActionButton>
            </div>
          </div>
        </Hint>
      </Show>

      <div style={{ display: "grid", gap: "20px", "grid-template-columns": "1.1fr 1fr 1fr" }}>
        <SectionCard
          title={t("proxy.listenersTitle")}
          subtitle={t("proxy.listenersSubtitle")}
          actions={
            <ActionButton onClick={openCreateListenerDialog}>
              {t("proxy.newListener")}
            </ActionButton>
          }
        >
          <div style={{ display: "grid", gap: "10px" }}>
            <For each={listenersQuery.data ?? []}>
              {(listener) => (
                <SelectableCard
                  selected={listener.id === selectedListenerId()}
                  onClick={() => setSelectedListenerId(listener.id)}
                >
                  <ListenerRuntimeStatus listenerId={listener.id} runtimeMap={runtimeMap()} t={t} />
                  <div class="row-actions" style={{ "justify-content": "space-between" }}>
                    <strong>{listener.name}</strong>
                    <ListenerStatusBadge listener={listener} runtime={runtimeMap().get(listener.id)} t={t} />
                  </div>
                  <div class="kv-grid">
                    <span>{listener.listen_host}:{listener.listen_port}</span>
                    <span>{listener.protocol} / {listener.tls_mode}</span>
                  </div>
                  <div class="row-actions" style={{ "justify-content": "flex-end" }}>
                    <ActionButton
                      onClick={(event) => {
                        event.stopPropagation();
                        openEditListenerDialog(listener);
                      }}
                    >
                      {t("proxy.edit")}
                    </ActionButton>
                    <ActionButton
                      variant="danger"
                      onClick={(event) => {
                        event.stopPropagation();
                        askDelete("listener", listener.id, listener.name);
                      }}
                    >
                      {t("proxy.delete")}
                    </ActionButton>
                  </div>
                </SelectableCard>
              )}
            </For>
            <Show when={(listenersQuery.data ?? []).length === 0}>
              <div class="panel panel-muted">{t("proxy.emptyListeners")}</div>
            </Show>
          </div>
        </SectionCard>

        <SectionCard
          title={t("proxy.routesTitle")}
          subtitle={selectedListener()?.name ?? t("proxy.selectListener")}
          actions={
            <ActionButton
              disabled={!selectedListenerId()}
              onClick={openCreateRouteDialog}
            >
              {t("proxy.newRoute")}
            </ActionButton>
          }
        >
          <div style={{ display: "grid", gap: "10px" }}>
            <For each={routesQuery.data ?? []}>
              {(route) => (
                <SelectableCard
                  selected={route.id === selectedRouteId()}
                  onClick={() => setSelectedRouteId(route.id)}
                >
                  <div class="row-actions" style={{ "justify-content": "space-between" }}>
                    <strong>{route.is_default ? t("proxy.defaultRoute") : route.server_names.join(", ")}</strong>
                    <StatusBadge
                      state={route.enabled ? "running" : "stopped"}
                      label={route.enabled ? t("common.enabled") : t("common.disabled")}
                    />
                  </div>
                  <div class="kv-grid">
                    <span>{route.path_prefix ?? "/"}</span>
                    <span>{t("proxy.priorityNewest")}</span>
                  </div>
                  <RouteRuntimeSummary runtime={routeRuntimeMap().get(route.id)} t={t} />
                  <div class="row-actions" style={{ "justify-content": "flex-end" }}>
                    <ActionButton
                      onClick={(event) => {
                        event.stopPropagation();
                        openEditRouteDialog(route);
                      }}
                    >
                      {t("proxy.edit")}
                    </ActionButton>
                    <ActionButton
                      variant="danger"
                      onClick={(event) => {
                        event.stopPropagation();
                        askDelete("route", route.id, route.server_names.join(", ") || t("proxy.defaultRoute"));
                      }}
                    >
                      {t("proxy.delete")}
                    </ActionButton>
                  </div>
                </SelectableCard>
              )}
            </For>
            <Show when={selectedListenerId() && (routesQuery.data ?? []).length === 0}>
              <div class="panel panel-muted">{t("proxy.emptyRoutes")}</div>
            </Show>
            <Show when={!selectedListenerId()}>
              <div class="panel panel-muted">{t("proxy.selectListener")}</div>
            </Show>
          </div>
        </SectionCard>

        <SectionCard
          title={t("proxy.upstreamsTitle")}
          subtitle={selectedRoute()?.id ?? t("proxy.selectRoute")}
          actions={
            <ActionButton
              disabled={!selectedRouteId()}
              onClick={openCreateUpstreamDialog}
            >
              {t("proxy.newUpstream")}
            </ActionButton>
          }
        >
          <div style={{ display: "grid", gap: "10px" }}>
            <For each={upstreamsQuery.data ?? []}>
              {(upstream) => (
                <div class="panel" style={{ display: "grid", gap: "10px" }}>
                  <div class="row-actions" style={{ "justify-content": "space-between" }}>
                    <strong>{upstream.target_kind}</strong>
                    <StatusBadge
                      state={upstream.enabled ? "running" : "stopped"}
                      label={upstream.enabled ? t("common.enabled") : t("common.disabled")}
                    />
                  </div>
                  <div class="kv-grid">
                    <span>
                      {(upstream.target_host ?? upstream.target_ref ?? "-")}:{upstream.target_port}
                    </span>
                    <div class="row-actions" style={{ gap: "8px", "justify-content": "flex-end" }}>
                      <ProtocolFamilyBadge label={getUpstreamProtocolFamilyLabel(t, upstream.upstream_scheme)} />
                      <span>{upstream.upstream_scheme}</span>
                    </div>
                  </div>
                  <Show when={upstream.path_rewrite_from || upstream.path_rewrite_to}>
                    <div class="kv-grid">
                      <span>{upstream.path_rewrite_from ?? "-"}</span>
                      <span>{upstream.path_rewrite_to ?? "-"}</span>
                    </div>
                  </Show>
                  <UpstreamRuntimeSummary runtime={upstreamRuntimeMap().get(upstream.id)} t={t} />
                  <div class="row-actions" style={{ "justify-content": "flex-end" }}>
                    <ActionButton onClick={() => openEditUpstreamDialog(upstream)}>
                      {t("proxy.edit")}
                    </ActionButton>
                    <ActionButton
                      variant="danger"
                      onClick={() =>
                        askDelete(
                          "upstream",
                          upstream.id,
                          `${upstream.target_host ?? upstream.target_ref ?? "-"}:${upstream.target_port}`
                        )
                      }
                    >
                      {t("proxy.delete")}
                    </ActionButton>
                  </div>
                </div>
              )}
            </For>
            <Show when={selectedRouteId() && (upstreamsQuery.data ?? []).length === 0}>
              <div class="panel panel-muted">{t("proxy.emptyUpstreams")}</div>
            </Show>
            <Show when={!selectedRouteId()}>
              <div class="panel panel-muted">{t("proxy.selectRoute")}</div>
            </Show>
          </div>
        </SectionCard>
      </div>

      <SectionCard
        title={t("proxy.certificatesTitle")}
        subtitle={t("proxy.certificatesSubtitle")}
        actions={
          <ActionButton onClick={openCreateCertificateDialog}>
            {t("proxy.newCertificate")}
          </ActionButton>
        }
      >
        <div style={{ display: "grid", gap: "10px" }}>
          <For each={certificatesQuery.data ?? []}>
            {(certificate) => (
              <div class="panel" style={{ display: "grid", gap: "10px" }}>
                <div class="row-actions" style={{ "justify-content": "space-between" }}>
                  <strong>{certificate.name}</strong>
                  <StatusBadge state="ready" label={certificate.source_type} />
                </div>
                <div class="kv-grid">
                  <span>{certificate.domains.join(", ")}</span>
                  <span>{certificate.cert_path}</span>
                </div>
                <div class="muted" style={{ "font-size": "12px" }}>
                  {certificate.key_path}
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
      </SectionCard>

      <ModalShell
        open={dialogMode() === "listener"}
        title={getListenerDialogTitle()}
        onOpenChange={(open) => !open && closeDialog()}
        actions={
          <>
            <ActionButton onClick={closeDialog}>
              {t("common.close")}
            </ActionButton>
            <ActionButton variant="primary" onClick={handleSubmitListener}>
              {editingTarget()?.kind === "listener" ? t("proxy.save") : t("proxy.create")}
            </ActionButton>
          </>
        }
      >
        <TextFieldControl label={t("proxy.listenerName")} value={listenerName()} onChange={setListenerName} />
        <div class="form-grid" style={{ "grid-template-columns": "1fr 160px" }}>
          <TextFieldControl label={t("proxy.listenHost")} value={listenerHost()} onChange={setListenerHost} />
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
            onChange={(value) => setListenerBindMode(value as BindMode)}
            options={bindModeOptions}
          />
          <TextFieldControl
            label={t("proxy.nicId")}
            value={listenerNicId()}
            onChange={setListenerNicId}
            placeholder={t("proxy.nicIdPlaceholder")}
          />
        </div>
        <CheckboxField label={t("common.enabled")} checked={listenerEnabled()} onChange={setListenerEnabled} />
      </ModalShell>

      <ModalShell
        open={dialogMode() === "route"}
        title={getRouteDialogTitle()}
        onOpenChange={(open) => !open && closeDialog()}
        actions={
          <>
            <ActionButton onClick={closeDialog}>
              {t("common.close")}
            </ActionButton>
            <ActionButton variant="primary" onClick={handleSubmitRoute}>
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
        actions={
          <>
            <ActionButton onClick={closeDialog}>
              {t("common.close")}
            </ActionButton>
            <ActionButton variant="primary" onClick={handleSubmitUpstream}>
              {editingTarget()?.kind === "upstream" ? t("proxy.save") : t("proxy.create")}
            </ActionButton>
          </>
        }
      >
        <div class="form-grid" style={{ "grid-template-columns": "1fr 1fr" }}>
          <SelectField
            label={t("proxy.targetKind")}
            value={upstreamTargetKind()}
            onChange={(value) => setUpstreamTargetKind(value as TargetKind)}
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
        <div class="form-grid" style={{ "grid-template-columns": "1fr 160px" }}>
          <TextFieldControl label={t("proxy.targetHost")} value={upstreamHost()} onChange={setUpstreamHost} />
          <TextFieldControl label={t("proxy.targetPort")} value={upstreamPort()} onChange={setUpstreamPort} />
        </div>
        <TextFieldControl label={t("proxy.targetRef")} value={upstreamTargetRef()} onChange={setUpstreamTargetRef} />
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
        actions={
          <>
            <ActionButton onClick={closeDialog}>
              {t("common.close")}
            </ActionButton>
            <ActionButton variant="primary" onClick={handleSubmitCertificate}>
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
          />
          <KTextFieldLike
            label={t("proxy.certificateKeyPath")}
            value={certificateKeyPath()}
            onChange={setCertificateKeyPath}
            onBrowse={() => void browseCertificateKeyPath()}
            browseLabel={t("hosts.browse")}
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
        open={dialogMode() === "delete"}
        title={t("proxy.delete")}
        onOpenChange={(open) => !open && closeDialog()}
        actions={
          <>
            <ActionButton onClick={closeDialog}>
              {t("common.close")}
            </ActionButton>
            <ActionButton variant="danger" onClick={handleDelete}>
              {t("proxy.confirmDelete")}
            </ActionButton>
          </>
        }
      >
        <div>{t("proxy.deletePrompt", { name: deleteTarget()?.name ?? "-" })}</div>
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
}) {
  return (
    <div class="kb-field">
      <span class="kb-label">{props.label}</span>
      <SimpleSelect value={props.value} onChange={props.onChange} options={props.options} />
    </div>
  );
}

function KTextFieldLike(props: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  onBrowse: () => void;
  browseLabel: string;
}) {
  return (
    <KTextField.Root class="kb-field" value={props.value} onChange={props.onChange}>
      <KTextField.Label class="kb-label">{props.label}</KTextField.Label>
      <div class="row-actions">
        <KTextField.Input class="kb-input" value={props.value} />
        <KButton.Root class="kb-btn ghost" onClick={props.onBrowse}>
          {props.browseLabel}
        </KButton.Root>
      </div>
    </KTextField.Root>
  );
}
