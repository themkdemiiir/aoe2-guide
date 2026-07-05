# scripts/migrations

`2026-07-02-rebuild-crawl-rows.mjs` is the **canonical crawl-rows rebuild** —
still live, not a retired one-off. See `scripts/data-pipeline/README.md`'s
"Full regen chain" for where it fits in the VM-side regen order.

The five one-off content-migration scripts that used to live here
(`migrate-builds-to-bilingual.mjs`, `migrate-locale-pairs.mjs`,
`fix-civ-content.mjs`, `strip-attribution.mjs`, `strip-unit-placeholders.mjs`)
were each run once against the content tree to perform a structural change,
then retired and deleted.
