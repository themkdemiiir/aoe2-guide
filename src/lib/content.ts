import { type CollectionEntry, getCollection } from "astro:content";
import { defaultLocale, type Locale } from "@/i18n/locales";

type ContentType =
  | "civilizations"
  | "build-orders"
  | "units"
  | "maps"
  | "beginner"
  | "glossary"
  | "articles";

/** Extract locale from a Content Layer entry id like "en/britons" → "en". */
export function localeFromEntryId(id: string): Locale {
  const seg = id.split("/")[0];
  return seg === "en" || seg === "tr" ? seg : defaultLocale;
}

/** Extract the canonical (locale-stripped) slug from an entry id: "en/britons" → "britons". */
export function canonicalSlug(id: string): string {
  const segs = id.split("/");
  segs.shift();
  return segs.join("/").replace(/\.(md|mdx)$/i, "");
}

/** Get all entries of a collection in a given locale; if none exist, fall back to defaultLocale. */
export async function getLocalizedEntries<T extends ContentType>(
  type: T,
  locale: Locale,
): Promise<Array<CollectionEntry<T>>> {
  const all = await getCollection(type);
  const fallbackEntries = all.filter((e) => localeFromEntryId(e.id) === defaultLocale);

  let result: Array<CollectionEntry<T>>;
  if (locale === defaultLocale) {
    result = fallbackEntries;
  } else {
    const localizedBySlug = new Map(
      all
        .filter((e) => localeFromEntryId(e.id) === locale)
        .map((entry) => [canonicalSlug(entry.id), entry]),
    );
    result = fallbackEntries.map((entry) => localizedBySlug.get(canonicalSlug(entry.id)) ?? entry);
  }

  // Alphabetize the name-keyed list collections (locale-aware so Turkish sorts
  // correctly). Order-sensitive types (beginner chapters, articles) keep glob order.
  if (type === "civilizations" || type === "maps") {
    const collator = new Intl.Collator(locale);
    const nameOf = (e: CollectionEntry<T>) => String((e.data as { name?: string }).name ?? "");
    result = [...result].sort((a, b) => collator.compare(nameOf(a), nameOf(b)));
  }

  return result;
}

/** Get a single entry by slug in a given locale; falls back to defaultLocale if not found. */
export async function getLocalizedEntry<T extends ContentType>(
  type: T,
  slug: string,
  locale: Locale,
): Promise<{ entry: CollectionEntry<T>; fallback: boolean } | null> {
  const all = await getCollection(type);
  const exact = all.find((e) => localeFromEntryId(e.id) === locale && canonicalSlug(e.id) === slug);
  if (exact) return { entry: exact, fallback: false };
  const fb = all.find(
    (e) => localeFromEntryId(e.id) === defaultLocale && canonicalSlug(e.id) === slug,
  );
  return fb ? { entry: fb, fallback: true } : null;
}
