// Minimal stub for astro:content used in vitest — only what content.ts imports at type level.
export async function getCollection(_type: string): Promise<unknown[]> {
  return [];
}

export type CollectionEntry<_T extends string> = {
  id: string;
  data: Record<string, unknown>;
};
