import * as KButton from "@kobalte/core/button";
import * as KCheckbox from "@kobalte/core/checkbox";
import * as KTextField from "@kobalte/core/text-field";
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

export const HostsIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={
      <>
        <path d="M4 6h16" />
        <path d="M4 12h16" />
        <path d="M4 18h10" />
        <path d="M16.5 15.5v5" />
        <path d="M14 18h5" />
      </>
    }
  />
);

export const ProxyIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={
      <>
        <path d="M5 7.5h8" />
        <path d="M5 12h14" />
        <path d="M5 16.5h8" />
        <path d="M14 5l5 7-5 7" />
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
        <g id="SVGRepo_bgCarrier" stroke-width="0"></g><g id="SVGRepo_tracerCarrier" stroke-linecap="round" stroke-linejoin="round"></g><g id="SVGRepo_iconCarrier"> <path d="M21 12C21 16.9706 16.9706 21 12 21C9.69494 21 7.59227 20.1334 6 18.7083L3 16M3 12C3 7.02944 7.02944 3 12 3C14.3051 3 16.4077 3.86656 18 5.29168L21 8M3 21V16M3 16H8M21 3V8M21 8H16" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></path> </g>
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

export const DownloadIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={
      <>
        <path d="M12 4v10" />
        <path d="M8 10l4 4 4-4" />
        <path d="M5 19h14" />
      </>
    }
  />
);

export const UploadIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={
      <>
        <path d="M12 20V10" />
        <path d="M8 14l4-4 4 4" />
        <path d="M5 5h14" />
      </>
    }
  />
);

export const ArrowUpIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={
      <>
        <path d="M12 19V5" />
        <path d="M6 11l6-6 6 6" />
      </>
    }
  />
);

export const ArrowDownIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={
      <>
        <path d="M12 5v14" />
        <path d="M6 13l6 6 6-6" />
      </>
    }
  />
);

export const ListEditIcon: Component<IconProps> = (props) => (
  <AppIconBase
    {...props}
    path={
      <>
        <path d="M5 7h8" />
        <path d="M5 12h6" />
        <path d="M5 17h5" />
        <path d="M14 19h3l3.5-3.5a1.6 1.6 0 0 0-2.3-2.3L14 17.4V19Z" />
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

export const ActionButton: Component<{
  variant?: "primary" | "ghost" | "danger";
  size?: "default" | "small";
  disabled?: boolean;
  onClick?: JSX.EventHandlerUnion<HTMLButtonElement, MouseEvent>;
  children: JSX.Element;
}> = (props) => (
  <KButton.Root
    class={`kb-btn ${props.variant ?? "ghost"} ${props.size === "small" ? "small" : ""}`.trim()}
    disabled={props.disabled}
    onClick={props.onClick}
  >
    {props.children}
  </KButton.Root>
);

export const SelectableCard: Component<{
  selected?: boolean;
  onClick?: JSX.EventHandlerUnion<HTMLButtonElement, MouseEvent>;
  children: JSX.Element;
}> = (props) => (
  <KButton.Root
    class="panel selectable-card"
    data-selected={props.selected ? "true" : undefined}
    onClick={props.onClick}
  >
    {props.children}
  </KButton.Root>
);

export const TextFieldControl: Component<{
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}> = (props) => (
  <KTextField.Root class="kb-field" value={props.value} onChange={props.onChange}>
    <KTextField.Label class="kb-label">{props.label}</KTextField.Label>
    <KTextField.Input class="kb-input" value={props.value} placeholder={props.placeholder} />
  </KTextField.Root>
);

export const CheckboxField: Component<{
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}> = (props) => (
  <KCheckbox.Root
    class="kb-checkbox"
    checked={props.checked}
    onChange={(checked) => props.onChange(Boolean(checked))}
  >
    <KCheckbox.Control class="kb-checkbox-control">
      <KCheckbox.Indicator class="kb-checkbox-indicator" />
    </KCheckbox.Control>
    <KCheckbox.Label class="kb-checkbox-label">{props.label}</KCheckbox.Label>
  </KCheckbox.Root>
);
