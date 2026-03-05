import { For } from "solid-js";

import { useI18n } from "../../i18n/context";
import { SUPPORTED_LOCALES, type AppLocale } from "../../i18n/locale";

const LOCALE_EMOJI: Record<AppLocale, string> = {
  "zh-CN": "🇨🇳",
  "en-US": "🇺🇸",
  "zh-HK": "🇭🇰",
  "ja-JP": "🇯🇵"
};

export function SettingsPage() {
  const { locale, setLocale, systemLocale, t } = useI18n();

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
          <label for="app-language" class="kb-label">
            {t("settings.languageLabel")}
          </label>
          <select
            id="app-language"
            class="kb-input settings-language-select"
            value={locale()}
            onInput={(event) => setLocale(event.currentTarget.value as AppLocale)}
          >
            <For each={SUPPORTED_LOCALES}>
              {(item) => (
                <option value={item}>
                  {LOCALE_EMOJI[item]} {t(`locale.${item}`)}
                </option>
              )}
            </For>
          </select>
        </div>
        <div class="muted">{t("settings.languageHelp")}</div>
        <div class="hint info">
          {t("settings.currentLanguage", { locale: t(`locale.${locale()}`) })}
          <br />
          {t("settings.systemLanguage", { locale: t(`locale.${systemLocale()}`) })}
        </div>
      </section>
    </div>
  );
}

