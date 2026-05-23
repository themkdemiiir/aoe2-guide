export const locales = ["en", "tr", "es", "de"] as const;
export const defaultLocale = "en";
export type Locale = (typeof locales)[number];

export const localeMeta: Record<Locale, { name: string; flag: string; dir: "ltr" | "rtl" }> = {
  en: { name: "English",  flag: "🇬🇧", dir: "ltr" },
  tr: { name: "Türkçe",   flag: "🇹🇷", dir: "ltr" },
  es: { name: "Español",  flag: "🇪🇸", dir: "ltr" },
  de: { name: "Deutsch",  flag: "🇩🇪", dir: "ltr" },
};

export function isLocale(value: string): value is Locale {
  return (locales as readonly string[]).includes(value);
}
