import type { Locale } from "./locales";
import { defaultLocale, isLocale, locales } from "./locales";
import { type UIKey, ui } from "./ui";

export function getLocaleFromUrl(url: URL): Locale {
  const segments = url.pathname.split("/").filter(Boolean);
  const first = segments[0];
  return first && isLocale(first) ? first : defaultLocale;
}

export function t(locale: Locale, key: UIKey): string {
  return ui[locale][key] ?? ui[defaultLocale][key];
}

export function localizedPath(path: string, locale: Locale): string {
  const trimmed = path.startsWith("/") ? path.slice(1) : path;
  const segments = trimmed.split("/");
  if (segments[0] && isLocale(segments[0])) {
    segments[0] = locale;
  } else {
    segments.unshift(locale);
  }
  return `/${segments.join("/")}`;
}

export function allLocalePaths(path: string): Array<{ locale: Locale; path: string }> {
  return locales.map((l) => ({ locale: l, path: localizedPath(path, l) }));
}

export type { Locale };
export { defaultLocale, isLocale, locales };
