import { type Component, type JSX } from "solid-js";
import "./Skeleton.css";

export type SkeletonVariant = "title" | "line" | "wide" | "grid" | "dashboard";

export interface SkeletonProps {
  variant?: SkeletonVariant;
  count?: number;
  class?: string;
}

function SkeletonItem(props: { variant: SkeletonVariant; class?: string }): JSX.Element {
  const className = () => {
    const base = "skeleton";
    switch (props.variant) {
      case "title":
        return `${base}-title`;
      case "wide":
        return `${base}-line wide`;
      case "grid":
        return `${base}-grid`;
      case "dashboard":
        return `${base}-grid dashboard-skeleton-grid`;
      default:
        return `${base}-line`;
    }
  };

  return <div class={`${className()} ${props.class ?? ""}`} />;
}

export const Skeleton: Component<SkeletonProps> = (props) => {
  const variant = () => props.variant ?? "line";
  const count = () => props.count ?? 1;

  if (variant() === "grid" || variant() === "dashboard") {
    return <SkeletonItem variant={variant()} class={props.class} />;
  }

  return (
    <>
      {Array.from({ length: count() }, (_, i) => (
        <SkeletonItem variant={variant()} class={props.class} />
      ))}
    </>
  );
};

export const SkeletonTitle: Component<{ class?: string }> = (props) => (
  <div class={`skeleton-title ${props.class ?? ""}`} />
);

export const SkeletonLine: Component<{ wide?: boolean; count?: number; class?: string }> = (props) => {
  const count = () => props.count ?? 1;
  return (
    <>
      {Array.from({ length: count() }, (_, i) => (
        <div class={`skeleton-line ${props.wide ? "wide" : ""} ${props.class ?? ""}`} />
      ))}
    </>
  );
};

export const SkeletonGrid: Component<{ dashboard?: boolean; class?: string }> = (props) => (
  <div class={`skeleton-grid ${props.dashboard ? "dashboard-skeleton-grid" : ""} ${props.class ?? ""}`} />
);