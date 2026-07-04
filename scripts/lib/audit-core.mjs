// Walk a bilingual content object and report field paths where tr === en
// (untranslated), excluding allow-listed proper-noun fields.

// Field-path suffixes that are intentionally EN in the tr slot.
const ALLOW = [
  /(^|\.)name$/, // civ/unit/build/map names + tech names
  /(^|\.)term$/, // glossary terms
];
// A tr value that merely repeats a proper noun is allowed if the EN value itself is
// a recognized proper-noun pattern (contains "(SomeProperNoun)").
const PROPER_NOUN_VALUE = /\([A-ZÇĞİÖŞÜ][\wçğıöşü]*\)\s*$/;

function isLocalized(v) {
  return (
    v &&
    typeof v === "object" &&
    "en" in v &&
    "tr" in v &&
    typeof v.en === "string" &&
    typeof v.tr === "string"
  );
}

export function auditEntry(_typeDir, _fileName, data) {
  const issues = [];
  walk(data, "", issues);
  return issues;
}

function walk(node, prefix, issues) {
  if (Array.isArray(node)) {
    // localizedString arrays are { en: [...], tr: [...] } handled below, not here
    node.forEach((v, i) => {
      walk(v, `${prefix}[${i}]`, issues);
    });
    return;
  }
  if (node && typeof node === "object") {
    if (isLocalized(node)) {
      if (node.en === node.tr && !allowed(prefix, node.en)) issues.push(prefix);
      return;
    }
    // localized array field: { en: string[], tr: string[] }
    if (Array.isArray(node.en) && Array.isArray(node.tr)) {
      if (JSON.stringify(node.en) === JSON.stringify(node.tr) && !allowed(prefix, "")) {
        issues.push(prefix);
      }
      return;
    }
    for (const [k, v] of Object.entries(node)) {
      walk(v, prefix ? `${prefix}.${k}` : k, issues);
    }
  }
}

function allowed(path, enValue) {
  if (ALLOW.some((re) => re.test(path))) return true;
  if (PROPER_NOUN_VALUE.test(enValue)) return true;
  return false;
}
