import { createSignal, onMount, Show } from "solid-js";
import * as KTooltip from "@kobalte/core/tooltip";

interface EllipsisCellProps {
  text: string | null | undefined;
}

export function EllipsisCell(props: EllipsisCellProps) {
  const content = () => (props.text ?? "").trim() || "-";
  let ref: HTMLDivElement | undefined;
  const [isOverflowing, setIsOverflowing] = createSignal(false);

  onMount(() => {
    if (ref) {
      const checkOverflow = () => {
        if (ref) {
          setIsOverflowing(ref.scrollWidth > ref.clientWidth);
        }
      };
      checkOverflow();
      const observer = new ResizeObserver(checkOverflow);
      observer.observe(ref);
      return () => observer.disconnect();
    }
  });

  return (
    <Show
      when={isOverflowing()}
      fallback={
        <div class="table-cell-ellipsis" ref={ref}>
          {content()}
        </div>
      }
    >
      <KTooltip.Root openDelay={180}>
        <KTooltip.Trigger as="div" class="table-cell-ellipsis" ref={ref}>
          {content()}
        </KTooltip.Trigger>
        <KTooltip.Portal>
          <KTooltip.Content class="kb-tooltip-content">
            {content()}
            <KTooltip.Arrow class="kb-tooltip-arrow" />
          </KTooltip.Content>
        </KTooltip.Portal>
      </KTooltip.Root>
    </Show>
  );
}
