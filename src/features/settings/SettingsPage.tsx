import * as KSelect from "@kobalte/core/select";

import { useI18n } from "../../i18n/context";
import { SUPPORTED_LOCALES, type AppLocale } from "../../i18n/locale";
import FlagCn from "../../assets/flag-cn.svg?url";
import FlagUs from "../../assets/flag-us.svg?url";
import FlagHk from "../../assets/flag-hk.svg?url";
import FlagJp from "../../assets/flag-jp.svg";

const LOCALE_FLAG: Record<AppLocale, string> = {
  "zh-CN": FlagCn,
  "en-US": FlagUs,
  "zh-HK": FlagHk,
  "ja-JP": FlagJp,
};

const localeOptions: { value: AppLocale; label: string }[] = SUPPORTED_LOCALES.map((locale) => ({
  value: locale,
  label: locale
}));

export function SettingsPage() {
  const { locale, setLocale, t } = useI18n();

  return (
    <div class="page">
      <section class="panel">
        <div class="panel-title">
          <h2>{t("settings.title")}</h2>
        </div>
        <div class="muted">{t("settings.subtitle")}</div>
      </section>

      <section class="panel">
        <h2>{t("settings.languageTitle")}</h2>
        <div class="settings-lang-row">
          <label class="kb-label">{t("settings.languageLabel")}</label>
          <KSelect.Root<{ value: AppLocale; label: string }>
            value={localeOptions.find((opt) => opt.value === locale())}
            onChange={(option) => option && setLocale(option.value)}
            options={localeOptions}
            optionValue="value"
            optionTextValue="label"
            itemComponent={(itemProps) => (
              <KSelect.Item item={itemProps.item} class="kb-select-item">
                <img
                  src={LOCALE_FLAG[itemProps.item.rawValue.value]}
                  style="width:20px;height:14px;margin-right:6px;vertical-align:middle"
                />
                <KSelect.ItemLabel>{t(`locale.${itemProps.item.rawValue.value}`)}</KSelect.ItemLabel>
              </KSelect.Item>
            )}
          >
            <KSelect.Trigger class="kb-input settings-language-select">
              <KSelect.Value<{ value: AppLocale; label: string }>>{(state) => (
                <>
                  <img
                    src={LOCALE_FLAG[state.selectedOption()?.value ?? "en-US"]}
                    style="width:20px;height:14px;margin-right:6px;vertical-align:middle"
                  />
                  {t(`locale.${state.selectedOption()?.value ?? "en-US"}`)}
                </>
              )}</KSelect.Value>
              <KSelect.Icon class="kb-select-icon">▾</KSelect.Icon>
            </KSelect.Trigger>
            <KSelect.Portal>
              <KSelect.Content class="kb-select-content">
                <KSelect.Listbox class="kb-select-listbox" />
              </KSelect.Content>
            </KSelect.Portal>
          </KSelect.Root>
        </div>
      </section>
    </div>
  );
}
