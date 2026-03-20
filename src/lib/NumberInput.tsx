import { type Component, type JSX, createEffect, createMemo, createSignal } from "solid-js";

import "./NumberInput.css";

interface NumberInputProps {
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  disabled?: boolean;
  class?: string;
}

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max);
}

function sanitizeInput(raw: string) {
  return raw.replace(/[^0-9eE+-]/g, "");
}

export const NumberInput: Component<NumberInputProps> = (props) => {
  const [inputValue, setInputValue] = createSignal(String(props.value));
  const [isFocused, setIsFocused] = createSignal(false);

  const minValue = createMemo(() => props.min ?? 1);
  const maxValue = createMemo(() => props.max ?? 65535);
  createEffect(() => {
    if (!isFocused()) {
      setInputValue(String(props.value));
    }
  });

  const commitValue = (raw: string) => {
    console.log(raw)
    const normalized = raw.trim();
    const parsed = Number(normalized);
    const nextValue = !Number.isFinite(parsed)
      ? minValue()
      : clamp(Math.trunc(parsed), minValue(), maxValue());

    setInputValue(String(nextValue));
    if (nextValue !== props.value) {
      props.onChange(nextValue);
    }
  };

  const handleInput: JSX.EventHandler<HTMLInputElement, InputEvent> = (event) => {
    setInputValue(event.currentTarget.value);
  };

  const handleBlur: JSX.EventHandler<HTMLInputElement, FocusEvent> = (event) => {
    commitValue(event.currentTarget.value);
    setIsFocused(false);
  };

  const handleKeyDown: JSX.EventHandler<HTMLInputElement, KeyboardEvent> = (event) => {
    if (event.key === "Enter") {
      event.currentTarget.blur();
      return;
    }
    if (event.key === "Escape") {
      setInputValue(String(props.value));
      event.currentTarget.blur();
    }
  };

  const step = (delta: number) => {
    if (props.disabled) return;
    const nextValue = clamp(props.value + delta, minValue(), maxValue());
    setInputValue(String(nextValue));
    if (nextValue !== props.value) {
      props.onChange(nextValue);
    }
  };

  return (
    <div class={`number-input-wrapper ${props.class ?? ""} ${isFocused() ? "focused" : ""}`}>
      <input
        type="number"
        class="kb-input number-input-field"
        value={inputValue()}
        onInput={handleInput}
        onFocus={() => setIsFocused(true)}
        onBlur={handleBlur}
        onKeyDown={handleKeyDown}
        disabled={props.disabled}
      />
      <div class="number-input-buttons">
        <button
          type="button"
          class="number-input-btn"
          aria-label="Increase value"
          onClick={() => step(1)}
          disabled={props.disabled || props.value >= maxValue()}
        >
        </button>
        <button
          type="button"
          class="number-input-btn"
          aria-label="Decrease value"
          onClick={() => step(-1)}
          disabled={props.disabled || props.value <= minValue()}
        >
        </button>
      </div>
    </div>
  );
};
