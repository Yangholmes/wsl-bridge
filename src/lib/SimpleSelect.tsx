import { type Component } from "solid-js";
import * as KSelect from "@kobalte/core/select";

export type SelectOption = { value: string; label: string };

export interface SimpleSelectProps {
  value: string;
  onChange: (value: string) => void;
  options: SelectOption[];
  class?: string;
  disabled?: boolean;
}

export const SimpleSelect: Component<SimpleSelectProps> = (props) => {
  const selectedOption = () => props.options.find((opt) => opt.value === props.value) ?? null;

  return (
    <KSelect.Root<SelectOption>
      options={props.options}
      optionValue="value"
      optionTextValue="label"
      value={selectedOption()}
      onChange={(opt) => opt && props.onChange(opt.value)}
      disabled={props.disabled}
      itemComponent={(itemProps) => (
        <KSelect.Item item={itemProps.item} class="kb-select-item">
          <KSelect.ItemLabel>{itemProps.item.rawValue.label}</KSelect.ItemLabel>
        </KSelect.Item>
      )}
    >
      <KSelect.Trigger class={`kb-select-trigger ${props.class ?? ""}`}>
        <KSelect.Value<SelectOption>>{(state) => state.selectedOption()?.label}</KSelect.Value>
        <KSelect.Icon class="kb-select-icon"><span class="kb-select-icon-triangle"></span></KSelect.Icon>
      </KSelect.Trigger>
      <KSelect.Portal>
        <KSelect.Content class="kb-select-content">
          <KSelect.Listbox class="kb-select-listbox" />
        </KSelect.Content>
      </KSelect.Portal>
    </KSelect.Root>
  );
};