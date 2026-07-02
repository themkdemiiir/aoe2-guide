// scripts/data-pipeline/lib/buckets.mjs
//
// Single ingestion-boundary source for elo bucketing + map canonicalization.
// Every JS generator imports from here. The two pure .sql files
// (build-duckdb.sql, build-benchmark.sql) keep an inline CASE/regex as the
// documented paired SQL face — keep thresholds in sync with eloCaseSql()/canonMap().

// Last month covered by the frozen aoestats archive. Months <= this come from
// the archive (full corpus, real maps); later months come from the crawl.
export const AOESTATS_END_MONTH = "2026-02";

export const ELO_BUCKETS = [
  "<1000",
  "1000-1199",
  "1200-1399",
  "1400-1649",
  "1650-1799",
  "1800-1999",
  "2000-2199",
  "2200-2499",
  "2500+",
];
export const ELO_BUCKETS_WITH_ALL = ["all", ...ELO_BUCKETS];

// Returns null for a null/undefined rating — NEVER fabricates a band.
// Callers decide whether to drop the row or label it. (Mirrors DuckDB's 'unknown'.)
export function eloBucket(rating) {
  if (rating == null) return null;
  const r = rating;
  return r < 1000
    ? "<1000"
    : r < 1200
      ? "1000-1199"
      : r < 1400
        ? "1200-1399"
        : r < 1650
          ? "1400-1649"
          : r < 1800
            ? "1650-1799"
            : r < 2000
              ? "1800-1999"
              : r < 2200
                ? "2000-2199"
                : r < 2500
                  ? "2200-2499"
                  : "2500+";
}

// Builds the matching SQL CASE so JS-embedded SQL shares thresholds AND always
// carries an IS NULL branch (kills the latent null -> '2500+' fall-through).
export function eloCaseSql(col, { nullLabel = "unknown" } = {}) {
  return (
    `CASE WHEN ${col} IS NULL THEN '${nullLabel}'` +
    ` WHEN ${col}<1000 THEN '<1000'` +
    ` WHEN ${col}<1200 THEN '1000-1199'` +
    ` WHEN ${col}<1400 THEN '1200-1399'` +
    ` WHEN ${col}<1650 THEN '1400-1649'` +
    ` WHEN ${col}<1800 THEN '1650-1799'` +
    ` WHEN ${col}<2000 THEN '1800-1999'` +
    ` WHEN ${col}<2200 THEN '2000-2199'` +
    ` WHEN ${col}<2500 THEN '2200-2499'` +
    ` ELSE '2500+' END`
  );
}

// Single JS mirror of the SQL canon regex (build-duckdb.sql): lowercase,
// strip a file extension, drop non-alphanumerics.
export function canonMap(raw) {
  return String(raw)
    .toLowerCase()
    .replace(/\.[a-z0-9]+$/i, "")
    .replace(/[^a-z0-9]/g, "");
}
