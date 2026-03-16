import { type Component, type JSX, Show } from "solid-js";
import "./Status.css";

export type HintVariant = "info" | "error";

export interface HintProps {
  variant?: HintVariant;
  class?: string;
  children: JSX.Element;
}

export const Hint: Component<HintProps> = (props) => {
  const variant = () => props.variant ?? "info";

  return (
    <div class={`hint ${variant()} ${props.class ?? ""}`}>
      {props.children}
    </div>
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