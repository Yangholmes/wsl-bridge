export const SUPPORTED_LOCALES = ["zh-CN", "en-US", "zh-HK", "ja-JP"] as const;

export type AppLocale = (typeof SUPPORTED_LOCALES)[number];

export const LOCALE_STORAGE_KEY = "wsl-bridge.locale";

export function isSupportedLocale(value: string): value is AppLocale {
  return (SUPPORTED_LOCALES as readonly string[]).includes(value);
}

function mapLocaleTag(raw: string): AppLocale {
  const normalized = raw.toLowerCase();
  if (normalized.startsWith("zh-hk") || normalized.startsWith("zh-mo") || normalized.startsWith("zh-tw")) {
    return "zh-HK";
  }
  if (normalized.startsWith("zh")) {
    return "zh-CN";
  }
  if (normalized.startsWith("ja")) {
    return "ja-JP";
  }
  return "en-US";
}

export function detectSystemLocale(languages: readonly string[] | undefined = navigator.languages): AppLocale {
  const candidates = languages && languages.length > 0 ? [...languages] : [navigator.language];
  for (const item of candidates) {
    if (!item) continue;
    const mapped = mapLocaleTag(item);
    if (isSupportedLocale(mapped)) return mapped;
  }
  return "en-US";
}

