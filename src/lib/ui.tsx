import type { Component, JSX } from "solid-js";
import { Show } from "solid-js";

type IconProps = JSX.SvgSVGAttributes<SVGSVGElement> & {
  size?: number;
};

function AppIconBase(props: IconProps & { path?: JSX.Element }) {
  const size = props.size ?? 20;
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      stroke-width="1.8"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
      {...props}
    >
      {props.path}
    </svg>
  );
}

export const SearchIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={
      <>
        <circle cx="11" cy="11" r="6" />
        <path d="M20 20l-3.5-3.5" />
      </>
    }
  />
);

export const DashboardIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={
      <>
        <rect x="3.5" y="3.5" width="7" height="7" rx="1.5" />
        <rect x="13.5" y="3.5" width="7" height="11" rx="1.5" />
        <rect x="3.5" y="13.5" width="7" height="7" rx="1.5" />
        <rect x="13.5" y="17.5" width="7" height="3" rx="1.5" />
      </>
    }
  />
);

export const RulesIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={
      <>
        <path d="M8 6h12" />
        <path d="M8 12h12" />
        <path d="M8 18h12" />
        <circle cx="4.5" cy="6" r="1.5" />
        <circle cx="4.5" cy="12" r="1.5" />
        <circle cx="4.5" cy="18" r="1.5" />
      </>
    }
  />
);

export const RuntimeIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={
      <>
        <path d="M4 17h3l2.5-5 3 8L15 11l2 6h3" />
      </>
    }
  />
);

export const TopologyIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={
      <>
        <rect x="3.5" y="3.5" width="6" height="6" rx="1.5" />
        <rect x="14.5" y="3.5" width="6" height="6" rx="1.5" />
        <rect x="9" y="14.5" width="6" height="6" rx="1.5" />
        <path d="M9.5 6.5h5" />
        <path d="M12 9.5v5" />
      </>
    }
  />
);

export const SettingsIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={
      <>
        <circle cx="12" cy="12" r="3.25" />
        <path d="M19.4 15a1 1 0 0 0 .2 1.1l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1 1 0 0 0-1.1-.2 1 1 0 0 0-.6.9V20a2 2 0 1 1-4 0v-.2a1 1 0 0 0-.6-.9 1 1 0 0 0-1.1.2l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1 1 0 0 0 .2-1.1 1 1 0 0 0-.9-.6H4a2 2 0 1 1 0-4h.2a1 1 0 0 0 .9-.6 1 1 0 0 0-.2-1.1l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1 1 0 0 0 1.1.2 1 1 0 0 0 .6-.9V4a2 2 0 1 1 4 0v.2a1 1 0 0 0 .6.9 1 1 0 0 0 1.1-.2l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1 1 0 0 0-.2 1.1 1 1 0 0 0 .9.6H20a2 2 0 1 1 0 4h-.2a1 1 0 0 0-.9.6Z" />
      </>
    }
  />
);

export const RefreshIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={
      <>
        <path d="M20 6v5h-5" />
        <path d="M4 18v-5h5" />
        <path d="M6.8 9A7 7 0 0 1 18 6" />
        <path d="M17.2 15A7 7 0 0 1 6 18" />
      </>
    }
  />
);

export const PlayIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={<path d="M8 6.5v11l8-5.5Z" />}
  />
);

export const StopIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={<rect x="7" y="7" width="10" height="10" rx="1.5" />}
  />
);

export const PlusIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={
      <>
        <path d="M12 5v14" />
        <path d="M5 12h14" />
      </>
    }
  />
);

export const EditIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={
      <>
        <path d="M4 20h4l9.5-9.5a2.12 2.12 0 1 0-3-3L5 17v3Z" />
      </>
    }
  />
);

export const TrashIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={
      <>
        <path d="M4 7h16" />
        <path d="M9 7V4h6v3" />
        <path d="M7 7l1 13h8l1-13" />
      </>
    }
  />
);

export const CopyIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={
      <>
        <rect x="9" y="9" width="11" height="11" rx="2" />
        <path d="M6 15H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v1" />
      </>
    }
  />
);

export const SparkIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={
      <>
        <path d="M12 3l1.6 4.4L18 9l-4.4 1.6L12 15l-1.6-4.4L6 9l4.4-1.6L12 3Z" />
      </>
    }
  />
);

export const StatusBadge: Component<{
  state: "running" | "stopped" | "error" | "ready" | "unknown";
  label: string;
}> = (props) => <span class={`status-badge ${props.state}`}>{props.label}</span>;

export const PageHeader: Component<{
  title: string;
  eyebrow?: string;
  actions?: JSX.Element;
}> = (props) => (
  <header class="page-header">
    <div class="page-title-group">
      <Show when={props.eyebrow}>
        <span class="page-eyebrow">{props.eyebrow}</span>
      </Show>
      <h1 class="page-title">{props.title}</h1>
    </div>
    <Show when={props.actions}>
      <div class="page-actions">{props.actions}</div>
    </Show>
  </header>
);

export const MetricCard: Component<{
  label: string;
  value: JSX.Element | string;
  detail?: JSX.Element | string;
}> = (props) => (
  <section class="metric-card">
    <span class="metric-label">{props.label}</span>
    <strong class="metric-value">{props.value}</strong>
    <Show when={props.detail}>
      <span class="metric-detail">{props.detail}</span>
    </Show>
  </section>
);

export const SectionCard: Component<{
  title: string;
  subtitle?: string;
  actions?: JSX.Element;
  children: JSX.Element;
}> = (props) => (
  <section class="surface-card section-card">
    <div class="section-card-header">
      <div>
        <h2 class="section-card-title">{props.title}</h2>
        <Show when={props.subtitle}>
          <p class="section-card-subtitle">{props.subtitle}</p>
        </Show>
      </div>
      <Show when={props.actions}>
        <div class="section-card-actions">{props.actions}</div>
      </Show>
    </div>
    {props.children}
  </section>
);
