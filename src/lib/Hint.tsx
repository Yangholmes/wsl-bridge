import { type Component, type JSX, Show, createSignal } from "solid-js";
import { ActionButton } from "./ui";
import "./Status.css";

export type HintVariant = "info" | "error" | "warn";

export interface HintProps {
  variant?: HintVariant;
  class?: string;
  closable?: boolean;
  closeLabel?: string;
  onClose?: () => void;
  children: JSX.Element;
}

export const Hint: Component<HintProps> = (props) => {
  const variant = () => props.variant ?? "info";
  const [visible, setVisible] = createSignal(true);

  const close = () => {
    setVisible(false);
    props.onClose?.();
  };

  return (
    <Show when={visible()}>
      <div class={`hint ${variant()} ${props.closable ? "closable" : ""} ${props.class ?? ""}`}>
        <Show when={props.closable}>
          <ActionButton
            variant="ghost-borderless"
            size="tiny"
            class="icon-btn hint-close-btn"
            ariaLabel={props.closeLabel ?? "Close"}
            onClick={close}
          >
            <span class="hint-close-icon" aria-hidden="true" />
          </ActionButton>
        </Show>
        {props.children}
      </div>
    </Show>
  );
};

export interface HintTextProps {
  variant?: HintVariant;
  text: string | null | undefined;
  class?: string;
}

export const HintText: Component<HintTextProps> = (props) => {
  return (
    <Show when={props.text}>
      {(text) => <Hint variant={props.variant} class={props.class}>{text()}</Hint>}
    </Show>
  );
};
