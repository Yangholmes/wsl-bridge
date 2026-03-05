import { createContext, createEffect, createMemo, createSignal, type Accessor, type ParentComponent, useContext } from "solid-js";
import { flatten, resolveTemplate, translator } from "@solid-primitives/i18n";

import { dict as enUSDict } from "./locales/en-US";
import { dict as jaJPDict } from "./locales/ja-JP";
import { dict as zhCNDict } from "./locales/zh-CN";
import { dict as zhHKDict } from "./locales/zh-HK";
import { AppLocale, detectSystemLocale, isSupportedLocale, LOCALE_STORAGE_KEY } from "./locale";

type TemplateArgs = Record<string, string | number | boolean>;

const dictionaries: Record<AppLocale, Record<string, unknown>> = {
  "zh-CN": flatten(zhCNDict) as Record<string, unknown>,
  "en-US": flatten(enUSDict) as Record<string, unknown>,
  "zh-HK": flatten(zhHKDict) as Record<string, unknown>,
  "ja-JP": flatten(jaJPDict) as Record<string, unknown>
};

type I18nContextValue = {
  locale: Accessor<AppLocale>;
  systemLocale: Accessor<AppLocale>;
  setLocale: (next: AppLocale) => void;
  t: (key: string, args?: TemplateArgs) => string;
};

const I18nContext = createContext<I18nContextValue>();

function readSavedLocale(): AppLocale | null {
  try {
    const saved = localStorage.getItem(LOCALE_STORAGE_KEY);
    if (saved && isSupportedLocale(saved)) {
      return saved;
    }
  } catch {
    // Ignore storage access failures.
  }
  return null;
}

function writeSavedLocale(next: AppLocale) {
  try {
    localStorage.setItem(LOCALE_STORAGE_KEY, next);
  } catch {
    // Ignore storage access failures.
  }
}

export const I18nProvider: ParentComponent = (props) => {
  const systemLocale = detectSystemLocale();
  const [locale, setLocaleSignal] = createSignal<AppLocale>(readSavedLocale() ?? systemLocale);

  const dictionary = createMemo(() => dictionaries[locale()]);
  const fallbackDictionary = createMemo(() => dictionaries["zh-CN"]);
  const translate = translator(dictionary, resolveTemplate);
  const fallbackTranslate = translator(fallbackDictionary, resolveTemplate);

  const setLocale = (next: AppLocale) => {
    setLocaleSignal(next);
    writeSavedLocale(next);
  };

  const t = (key: string, args?: TemplateArgs): string => {
    const translateAny = translate as (path: string, args?: TemplateArgs) => unknown;
    const fallbackAny = fallbackTranslate as (path: string, args?: TemplateArgs) => unknown;
    const value = args
      ? (translateAny(key, args) as string | undefined)
      : (translateAny(key) as string | undefined);
    if (typeof value === "string" && value.length > 0) return value;

    const fallbackValue = args
      ? (fallbackAny(key, args) as string | undefined)
      : (fallbackAny(key) as string | undefined);
    if (typeof fallbackValue === "string" && fallbackValue.length > 0) return fallbackValue;
    return key;
  };

  createEffect(() => {
    document.documentElement.lang = locale();
  });

  return (
    <I18nContext.Provider value={{ locale, systemLocale: () => systemLocale, setLocale, t }}>
      {props.children}
    </I18nContext.Provider>
  );
};

export function useI18n() {
  const context = useContext(I18nContext);
  if (!context) {
    throw new Error("useI18n must be used within I18nProvider");
  }
  return context;
}
