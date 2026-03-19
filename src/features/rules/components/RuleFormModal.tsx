import { Show } from "solid-js";
import type { SetStoreFunction } from "solid-js/store";
import * as KButton from "@kobalte/core/button";
import * as KCheckbox from "@kobalte/core/checkbox";
import * as KDialog from "@kobalte/core/dialog";
import * as KSelect from "@kobalte/core/select";
import * as KSwitch from "@kobalte/core/switch";
import * as KTextField from "@kobalte/core/text-field";
import { useI18n } from "../../../i18n/context";
import { NumberInput } from "../../../lib/NumberInput";
import { toLocalTime } from "../../../lib/datetime";
import { Hint } from "../../../lib/Hint";

import type { BindMode, RuleType, TargetKind } from "../../../lib/types";

export type FormState = {
  name: string;
  type: RuleType;
  listen_host: string;
  listen_port: string;
  target_kind: TargetKind;
  target_ref: string;
  target_host: string;
  target_port: string;
  bind_mode: BindMode;
  nic_id: string;
  enabled: boolean;
  fw_domain: boolean;
  fw_private: boolean;
  fw_public: boolean;
};

export type SelectOption = {
  value: string;
  label: string;
};

type AppSelectProps = {
  value: string;
  onChange: (value: string) => void;
  options: SelectOption[];
  disabled?: boolean;
  placeholder?: string;
  triggerClass?: string;
};

type RuleFormModalProps = {
  open: boolean;
  isEditing: boolean;
  canToggleEnabled: boolean;
  canManageFirewall: boolean;
  form: FormState;
  setForm: SetStoreFunction<FormState>;
  message: { type: "info" | "error"; text: string } | null;
  isProxyType: boolean;
  isSingleNic: boolean;
  targetPreview: string | null;
  topologyTimestamp: string | null;
  ruleTypeOptions: SelectOption[];
  targetKindOptions: SelectOption[];
  bindModeOptions: SelectOption[];
  adapterOptions: SelectOption[];
  targetRefOptions: SelectOption[];
  onOpenChange: (open: boolean) => void;
  onTargetKindChange: (kind: TargetKind) => void;
  onTargetRefChange: (targetRef: string) => void;
  onSubmit: () => void;
  onCancel: () => void;
};

function AppSelect(props: AppSelectProps) {
  const selectedOption = () => props.options.find((option) => option.value === props.value) ?? null;

  return (
    <KSelect.Root<SelectOption>
      options={props.options}
      optionValue="value"
      optionTextValue="label"
      value={selectedOption()}
      onChange={(option) => {
        if (option) props.onChange(option.value);
      }}
      itemComponent={(itemProps) => (
        <KSelect.Item item={itemProps.item} class="kb-select-item">
          <KSelect.ItemLabel>{itemProps.item.rawValue.label}</KSelect.ItemLabel>
          <KSelect.ItemIndicator class="kb-select-item-indicator">✓</KSelect.ItemIndicator>
        </KSelect.Item>
      )}
      disabled={props.disabled}
      placeholder={props.placeholder ?? ""}
    >
      <KSelect.Trigger class={`kb-select-trigger ${props.triggerClass ?? ""}`}>
        <KSelect.Value<SelectOption>>{(state) => state.selectedOption()?.label}</KSelect.Value>
        <KSelect.Icon class="kb-select-icon">▾</KSelect.Icon>
      </KSelect.Trigger>
      <KSelect.Portal>
        <KSelect.Content class="kb-select-content">
          <KSelect.Listbox class="kb-select-listbox" />
        </KSelect.Content>
      </KSelect.Portal>
    </KSelect.Root>
  );
}

export function RuleFormModal(props: RuleFormModalProps) {
  const { t } = useI18n();

  return (
    <Show when={props.open}>
      <KDialog.Root open={props.open} onOpenChange={props.onOpenChange}>
        <KDialog.Portal>
          <KDialog.Overlay class="kb-dialog-overlay" />
          <KDialog.Content class="kb-dialog-content">
            <div class="panel-title">
              <KDialog.Title>{props.isEditing ? t("rules.formEditTitle") : t("rules.formCreateTitle")}</KDialog.Title>
              <div class="modal-title-actions">
                <KSwitch.Root
                  checked={props.form.enabled}
                  onChange={(checked) => props.setForm("enabled", checked)}
                  class="kb-switch modal-header-switch"
                  disabled={!props.canToggleEnabled}
                >
                  <KSwitch.Input aria-label={t("rules.formEnableRule")} />
                  <KSwitch.Control class="kb-switch-control">
                    <KSwitch.Thumb class="kb-switch-thumb" />
                  </KSwitch.Control>
                </KSwitch.Root>
              </div>
            </div>

            <Show when={props.isEditing}>
              <KDialog.Description>
                <Hint>{t("rules.formEditHint")}</Hint>
              </KDialog.Description>
            </Show>

            <div class="form-grid">
              <KTextField.Root
                class="kb-field"
                value={props.form.name}
                onChange={(value) => props.setForm("name", value)}
              >
                <KTextField.Label>{t("rules.formName")}</KTextField.Label>
                <KTextField.Input class="kb-input" />
              </KTextField.Root>

              <div class="kb-field">
                <label class="kb-label">{t("rules.formType")}</label>
                <AppSelect
                  value={props.form.type}
                  onChange={(value) => props.setForm("type", value as RuleType)}
                  options={props.ruleTypeOptions}
                  disabled={props.isEditing}
                />
              </div>

              <KTextField.Root
                class="kb-field"
                value={props.form.listen_host}
                onChange={(value) => props.setForm("listen_host", value)}
              >
                <KTextField.Label>{t("rules.formListenHost")}</KTextField.Label>
                <KTextField.Input class="kb-input" />
              </KTextField.Root>

              <KTextField.Root
                class="kb-field"
                value={props.form.listen_port}
                onChange={(value) => props.setForm("listen_port", value)}
              >
                <KTextField.Label>{t("rules.formListenPort")}</KTextField.Label>
                <NumberInput
                  value={parseInt(props.form.listen_port, 10) || 1}
                  onChange={(v) => props.setForm("listen_port", String(v))}
                  min={1}
                  max={65535}
                />
              </KTextField.Root>

              <div class="kb-field">
                <label class="kb-label">{t("rules.formTargetKind")}</label>
                <AppSelect
                  value={props.form.target_kind}
                  onChange={(value) => props.onTargetKindChange(value as TargetKind)}
                  options={props.targetKindOptions}
                  disabled={props.isEditing || props.isProxyType}
                />
              </div>

              <Show
                when={props.form.target_kind === "wsl" || props.form.target_kind === "hyperv"}
                fallback={
                  <KTextField.Root
                    class="kb-field"
                    value={props.form.target_ref}
                    onChange={(value) => props.setForm("target_ref", value)}
                  >
                    <KTextField.Label>{t("rules.formTargetRef")}</KTextField.Label>
                    <KTextField.Input
                      class="kb-input"
                      disabled={props.isProxyType || props.form.target_kind === "static"}
                    />
                  </KTextField.Root>
                }
              >
                <div class="kb-field">
                  <label class="kb-label">{t("rules.formTargetRef")}</label>
                  <AppSelect
                    value={props.form.target_ref}
                    onChange={props.onTargetRefChange}
                    options={props.targetRefOptions}
                    disabled={props.isProxyType || props.targetRefOptions.length === 0}
                    placeholder={props.targetRefOptions.length === 0 ? t("rules.formNoTargetAvailable") : t("rules.formSelectTarget")}
                  />
                </div>
              </Show>

              <Show when={!props.isProxyType && (props.form.target_kind === "wsl" || props.form.target_kind === "hyperv")}>
                <Hint class="target-preview">
                  {t("rules.formIpPreview")}: {props.targetPreview ?? t("rules.formIpNotResolved")}
                  <br />
                  {t("rules.formLastScan")}: {toLocalTime(props.topologyTimestamp)}
                </Hint>
              </Show>

              <KTextField.Root
                class="kb-field"
                value={props.form.target_host}
                onChange={(value) => props.setForm("target_host", value)}
              >
                <KTextField.Label>{t("rules.formTargetHost")}</KTextField.Label>
                <KTextField.Input class="kb-input" disabled={props.isProxyType || props.form.target_kind !== "static"} />
              </KTextField.Root>

              <KTextField.Root
                class="kb-field"
                value={props.form.target_port}
                onChange={(value) => props.setForm("target_port", value)}
              >
                <KTextField.Label>{t("rules.formTargetPort")}</KTextField.Label>
                <NumberInput
                  value={parseInt(props.form.target_port, 10) || 1}
                  onChange={(v) => props.setForm("target_port", String(v))}
                  min={1}
                  max={65535}
                  disabled={props.isProxyType}
                />
              </KTextField.Root>

              <div class="kb-field">
                <label class="kb-label">{t("rules.formBindMode")}</label>
                <AppSelect
                  value={props.form.bind_mode}
                  onChange={(value) => props.setForm("bind_mode", value as BindMode)}
                  options={props.bindModeOptions}
                />
              </div>

              <div class="kb-field">
                <label class="kb-label">{t("rules.formNic")}</label>
                <AppSelect
                  value={props.form.nic_id}
                  onChange={(value) => props.setForm("nic_id", value)}
                  options={props.adapterOptions}
                  disabled={!props.isSingleNic}
                />
              </div>
            </div>

            <Show
              when={props.canManageFirewall}
              fallback={<Hint>{t("rules.firewallAdminHint")}</Hint>}
            >
              <div class="checks kb-checks">
                <KCheckbox.Root checked={props.form.fw_domain} onChange={(checked) => props.setForm("fw_domain", checked)} class="kb-checkbox">
                  <KCheckbox.Input />
                  <KCheckbox.Control class="kb-checkbox-control">
                    <KCheckbox.Indicator class="kb-checkbox-indicator" />
                  </KCheckbox.Control>
                  <KCheckbox.Label class="kb-checkbox-label">Domain</KCheckbox.Label>
                </KCheckbox.Root>

                <KCheckbox.Root checked={props.form.fw_private} onChange={(checked) => props.setForm("fw_private", checked)} class="kb-checkbox">
                  <KCheckbox.Input />
                  <KCheckbox.Control class="kb-checkbox-control">
                    <KCheckbox.Indicator class="kb-checkbox-indicator" />
                  </KCheckbox.Control>
                  <KCheckbox.Label class="kb-checkbox-label">Private</KCheckbox.Label>
                </KCheckbox.Root>

                <KCheckbox.Root checked={props.form.fw_public} onChange={(checked) => props.setForm("fw_public", checked)} class="kb-checkbox">
                  <KCheckbox.Input />
                  <KCheckbox.Control class="kb-checkbox-control">
                    <KCheckbox.Indicator class="kb-checkbox-indicator" />
                  </KCheckbox.Control>
                  <KCheckbox.Label class="kb-checkbox-label">Public</KCheckbox.Label>
                </KCheckbox.Root>
              </div>
            </Show>

            <div class="actions modal-actions">
              <KButton.Root class="kb-btn accent" onClick={props.onSubmit}>
                {props.isEditing ? t("rules.formSaveChanges") : t("rules.formCreateRule")}
              </KButton.Root>
              <KButton.Root class="kb-btn ghost" onClick={props.onCancel}>
                {t("rules.formCancel")}
              </KButton.Root>
            </div>

            <Show when={props.message}>
              {(msg) => (
                <div class={`hint ${msg().type === "error" ? "error" : "info"}`}>{msg().text}</div>
              )}
            </Show>
          </KDialog.Content>
        </KDialog.Portal>
      </KDialog.Root>
    </Show>
  );
}
