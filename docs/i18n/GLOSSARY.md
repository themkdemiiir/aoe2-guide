# Translation Glossary (EN ↔ TR)

Canonical terms for translating AOE2 Guide content. **Claude Code and any translator must use these exactly.** Living file — add new terms as they come up.

> v2 — verified **2026-06-09** against `src/i18n/ui.ts` and `src/content/glossary/tr/*` (repo @ `3bf752b`).

Marker legend:
- ✅ verified in repo (ui.ts string or glossary/tr content) — keep as-is
- ⚠ proposed — confirm before relying on it
- 🔶 **site is internally inconsistent — decision + code fix needed (see "Conflicts" below)**

## Policy
- **Register:** casual (informal, second-person). Avoid stiff formal phrasing.
- **Loanwords take Turkish suffixes with an apostrophe:** `counter'ları`, `Knight'ı`, `Skirmisher'lar`, `Mangonel'i`. ✅ (pattern confirmed in `ui.ts` — "Counter'lar", "counter'ları", "pocket'ların")

### Do NOT translate (keep in English)
- **Unit names:** Archer, Skirmisher, Knight, Camel Rider, Mangonel, Eagle Warrior, Man-at-Arms, … ✅ (glossary/tr keeps them)
- **Civilization names:** Romans, Bulgarians, Vikings, …
- **Tech & unique-unit names:** Ballistas, Comitatenses, Legionary, Chu Ko Nu, …
- **Building names in prose:** Town Center, Mill, Barracks, Archery Range, Stable, Blacksmith, Market, Watch Tower ✅ — `glossary/tr/feudal-age.md` keeps all of these in English. (The ⚠ Turkish building table in v1 is **dropped**: site practice is English buildings, which also matches in-game EN UI most TR players use.)
- **Established player jargon (loanwords):** build order ✅, micro ✅, macro ✅, boom, rush, drush, flush, sniping ✅, trash ✅, flank/pocket (as terms), uptime, laming. Glossary/tr entries keep the EN term as the headword and explain in Turkish.
- **Brand:** "AOE2 Guide", "Age of Empires II".

### DO translate
Ages, resources, core mechanics vocabulary, UI strings, and all long-form prose.

---

## UI / navigation (✅ all verified in `ui.ts` unless marked)
| EN | TR |
|---|---|
| Civilizations | Uygarlıklar |
| Build Orders (nav/UI label) | Yapım Sıraları |
| build order (in prose) | build order ✅ *(loanword — `glossary/tr` uses "build order'a göre")* |
| Units | Birimler |
| Maps | Haritalar |
| Matchups | Eşleşmeler |
| Counters (nav) | 🔶 see Conflicts |
| counter (in prose) | counter ✅ *(loanword with apostrophe suffixes — site body text uses this consistently; v1's "Karşıtlar in body" guidance is dropped)* |
| Unit Counters (page title) | Birim Karşıtları |
| Learn | Öğren |
| Glossary | Sözlük |
| About | Hakkında |
| Blog | Blog |
| Search… | Ara… ("Rehberlerde ara…" placeholder) |
| Reset | Sıfırla |
| results | sonuç |
| All | Tümü |
| beats / counters (verb) | yener |
| Strong vs / Weak vs | Karşı güçlü / Karşı zayıf |
| Civ Bonuses | Uygarlık Bonusları |
| Team Bonus | Takım Bonusu |
| Unique Units | Özgün Birimler |
| Unique Techs | Özgün Teknolojiler |
| Tier | Sınıf |
| Win rate / Play rate | Kazanma oranı / Oynama oranı |
| Difficulty: beginner / intermediate / advanced | Başlangıç / Orta / İleri |
| Map types: open / closed / hybrid / water / nomad | Açık / Kapalı / Karma / Su / Göçebe |
| Flank / Pocket (team-game UI) | Kanat (Flank) / İç Oyuncu (Pocket) ✅ *(term itself stays English in prose)* |
| Recommended Civilizations | Önerilen Uygarlıklar |
| Sources / Updated | Kaynaklar / Güncellendi |
| On this page (ToC) | Bu sayfada |

## Ages — canonical: **Karanlık / Feodal / Kale Çağı / İmparatorluk Çağı**
| EN | TR |
|---|---|
| Dark Age | Karanlık Çağ ✅ (6× in content) |
| Feudal Age | Feodal Çağ ✅ (`ui.ts age.feudal`; 15× content) 🔶 one stray "Feudal Çağ" in `counters.feudal` |
| Castle Age | Kale Çağı ✅ |
| Imperial Age | İmparatorluk Çağı 🔶 (12× in content vs "Emperyal Çağ" in `ui.ts` + 3× content — standardize to İmparatorluk, see Conflicts) |
| age up / advancing | çağ atlama ✅ (`glossary/tr/feudal-age.md`) |
| target age (filter) | Hedef çağ ✅ |

## Resources (⚠ unchanged — confirm against TR community usage)
| EN | TR |
|---|---|
| Food | Yiyecek ⚠ (500 yiyecek ✅ appears in glossary/tr) |
| Wood | Odun ⚠ |
| Gold | Altın ⚠ ("altın" usage ✅ in trash-unit.md) |
| Stone | Taş ⚠ |
| Villager | Köylü ✅ ("Köylü zamanlamalı") |
| Population (pop) | Nüfus ✅ (`home.pop`) |

## Combat & mechanics
| EN | TR |
|---|---|
| pierce armour | delici zırh ✅ (counters.intro) |
| melee armour | yakın dövüş zırhı ⚠ |
| bonus damage | bonus hasar ✅ (counters.intro) |
| attack | saldırı ⚠ |
| range / ranged | menzil / menzilli ✅ ("menzilli birimlerle") |
| hit points (HP) | can ⚠ |
| line of sight (LOS) | görüş alanı ⚠ |
| blast / splash | alan hasarı ⚠ |
| garrison | garnizon ⚠ |
| conversion (Monk) | dönüştürme ⚠ |
| trash units | trash birim(ler) ✅ (`glossary/tr/trash-unit.md` — v1's "çöp birimler" is **wrong**, drop it) |
| micro | micro ✅ (loanword, headword in glossary/tr) |
| kiting / hit-and-run | vur-kaç ✅ (`glossary/tr/micro.md` — v1 said keep "kiting"; site translates it) |
| sniping (picking off targets) | ayıklamak (sniping) ✅ — TR verb + EN term in parens, as glossary/tr does |
| upgrade | yükseltme ✅ ("yükseltme hatları", "Man-at-Arms yükseltmesi") |
| tech / technology | teknoloji ✅ ("tek tip teknolojilerini" 🔶 — should be "özgün teknolojilerini", see Conflicts) |
| meta | meta ✅ ("Güncel Meta") |
| timing | zamanlama ✅ |

---

## 🔶 Conflicts to resolve (code fixes, one small PR)
The site disagrees with itself; the canonical column above wins. Fix list:
1. `ui.ts age.imperial: "Emperyal Çağ"` → `"İmparatorluk Çağı"`; same for `matchups.imperialAge`. Then sweep the 3 "Emperyal" occurrences in `src/content/**`.
2. `ui.ts counters.feudal: "Feudal Çağ counter döngüsü"` → `"Feodal Çağ counter döngüsü"`.
3. `ui.ts nav.counters: "Counters"` (untranslated EN in the TR nav) → pick **"Karşıtlar"** (matches page title "Birim Karşıtları") or **"Counter'lar"** (matches body register). Recommendation: Karşıtlar for the nav, counter-as-loanword everywhere in prose.
4. `ui.ts matchups.intro: "tek tip teknolojilerini"` — mistranslation of "unique techs"; → "özgün teknolojilerini" (consistent with `matchups.uniqueTechs`).

### When a term isn't here
Pick the best Turkish term, use it consistently across the whole file, and **add it here** (the `/translate` prompt lists new terms it had to invent so you can review them).
