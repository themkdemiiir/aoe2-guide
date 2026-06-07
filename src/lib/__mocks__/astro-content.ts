// Minimal stub for astro:content used in vitest — only what content.ts imports.
const collections = new Map<string, unknown[]>();

export function setMockCollection(type: string, entries: unknown[]): void {
  collections.set(type, entries);
}

export function resetMockCollections(): void {
  collections.clear();
}

export async function getCollection(type: string): Promise<unknown[]> {
  return collections.get(type) ?? [];
}

export type CollectionEntry<_T extends string> = {
  id: string;
  data: Record<string, unknown>;
};
