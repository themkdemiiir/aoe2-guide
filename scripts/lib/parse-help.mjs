// Generalized parser for aoe2techtree locale help strings (EN + TR).
// The civ help string lists bonuses, unique unit(s), unique techs, and team bonus,
// separated by <br>. Section headers differ per language; bullets use "•".

const MARKERS = {
  en: { civ: /civilization$/i, unit: /^Unique Unit/i, tech: /^Unique Tech/i, team: /^Team Bonus/i },
  tr: { civ: /medeniyeti$/i, unit: /^Özgün Birim/i, tech: /^Özgün Teknoloji/i, team: /^Takım Bonusu/i },
};

export function parseHelp(raw, lang) {
  if (typeof raw !== "string") return null;
  const M = MARKERS[lang];
  if (!M) throw new Error(`parseHelp: unknown lang "${lang}"`);

  const lines = raw.split(/<br\s*\/?>/i).map((l) => l.replace(/<\/?[a-z]+>/gi, "").trim());
  const out = { civType: "", civBonuses: [], teamBonus: "", uniqueTechs: [] };
  let section = "bonuses";

  for (const l of lines) {
    if (!l) continue;
    if (!out.civType && M.civ.test(l)) {
      out.civType = l.replace(M.civ, "").trim();
      continue;
    }
    if (M.unit.test(l)) { section = "skip"; continue; }
    if (M.tech.test(l)) { section = "techs"; continue; }
    if (M.team.test(l)) { section = "team"; continue; }

    const text = l.replace(/^•\s*/, "").trim();
    if (section === "bonuses" && l.startsWith("•")) {
      out.civBonuses.push(text);
    } else if (section === "team") {
      out.teamBonus = out.teamBonus ? `${out.teamBonus} ${text}` : text;
    } else if (section === "techs" && l.startsWith("•")) {
      const m = text.match(/^(.+?)\s*\(([^)]+)\)\s*$/);
      out.uniqueTechs.push(m ? { name: m[1].trim(), effect: m[2].trim() } : { name: text, effect: "" });
    }
  }
  return out.civBonuses.length ? out : null;
}
