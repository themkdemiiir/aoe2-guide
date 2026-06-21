# Build Order Source Index

Generated: 2026-06-21

Maps all 46 build-order YAML slugs to their best available Hera source.
Used by Task 8 (build verification/correction) — **do not modify YAML files here**.

## Source types
- **transcript** — per-build Hera transcript in `md/build-orders/hera-*.md`
- **guide-section** — named section in `md/reference/hera-strategy-guide-2025-12-pdftext.txt`
- **needs-public-source** — no Hera source covers this build; needs a reputable public source (e.g. AoE2 Wiki, CaptureAge, ZeroEmpires, Spirit Of The Law)

## Counts
- transcript: **11**
- guide-section: **26**
- needs-public-source: **9**
- **Total: 46**

---

## Index table

| slug | source_type | source_file | section / lines | notes |
|---|---|---|---|---|
| `17pop-japanese-maa-rush` | transcript | `md/build-orders/hera-japanese-maa-rush.md` | full build (TR text) | proposed_slug matches exactly; guide also has "17 Vils Japanese Man-at-Arms Rush" L1424 as a secondary cross-check |
| `17pop-teuton-tower-rush` | transcript | `md/build-orders/hera-teuton-tower-rush.md` | full build (TR text) | guide also has "17 Vils Teuton Tower Rush" L2402 as cross-check |
| `18pop-cumans-2tc-boom` | transcript | `md/build-orders/hera-cumans-2tc-boom.md` | full build (TR text) | guide also has "18 Vils Cuman 2 TC Boom" L379 |
| `18pop-double-barracks-eagles` | transcript | `md/build-orders/hera-double-barracks-eagles.md` | full build (TR text) | guide also has "18 Vils Double Barracks Eagles" L438 |
| `18pop-feudal-drush` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "18 Vils Feudal Drush" L1150–L1218 | distinct from `drush-fc` (this stays feudal; drush-fc goes to castle); Malians, Lithuanians, Japanese, Britons, Vikings |
| `18pop-scouts` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "18 Vils 1-Stable Scouts" L143–L211 | YAML name "1-Stable Scouts" matches guide title exactly; Franks, Huns, Lithuanians, Magyars, Malians |
| `18pop-scouts-into-archers` | transcript | `md/build-orders/hera-scouts-into-archers.md` | full build (TR text) | guide also has "18 Vils Scouts into Archers" L2150 |
| `18pop-scouts-into-cavalry-archers` | transcript | `md/build-orders/hera-scouts-into-cavalry-archers.md` | full build (TR text) | guide also has "18 Vils Scouts into Cavalry Archers" L2225 |
| `19pop-archers` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "19 Vils 1-Range Archers" L79–L142 | YAML likely "19 Pop 1-Range Archers"; Britons, Mayans, Ethiopians, Tatars, Vikings |
| `23pop-fast-castle-boom-arena` | transcript | `md/build-orders/hera-fast-castle-boom-arena.md` | full build (TR text) | Arena FC Boom; Burgundians, Poles, Bohemians, Portuguese, Bengalis |
| `25pop-fast-castle-unique-unit` | transcript | `md/build-orders/hera-fast-castle-unique-unit.md` | full build (TR text) | guide also has "25+2 Vils Fast Castle into Unique Unit" L962 |
| `25pop-knight-rush` | transcript | `md/build-orders/hera-knight-rush-25pop.md` | full build (TR text) | guide section is "Knight Rush (Beginner)" L1703 — 25+4 vils; note guide labels it "Beginner", transcript is "25pop", confirm pop count matches |
| `28pop-turks-fast-imp` | transcript | `md/build-orders/hera-turks-fast-imp.md` | full build (TR text) | guide has "28+2+2 Vils Turk Fast Imperial" L2481 — pop counts differ slightly (28 vs 28+2+2); verify |
| `30pop-fast-imp-generic` | transcript | `md/build-orders/hera-fast-imp-generic.md` | full build (TR text) | no exact guide section; guide covers Turk Fast Imp but not a generic 30pop version — transcript is primary source |
| `anti-lame-dark-age` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "17 Vils Anti-Lame Fast Up Dark Age (Advanced)" L212–L258 | advanced build; any civ standard start |
| `arena-fc-monks` | needs-public-source | — | — | Arena Fast Castle with Monk Rush; Aztecs, Byzantines, Bohemians, Spanish, Saracens; no dedicated Hera section found — need public source (AoE2 Wiki / TheViper/Hera Arena builds) |
| `armenian-spear-fc-relic` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "26+2 Vils Armenian Spear Rush Into Fast Castle Relic Control" L259–L330 | section spans two pages; full steps plus continued block |
| `chinese-fast-feudal` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "20 Vils Chinese Fast Feudal" L331–L378 | China-only build; note YAML pop count matches guide (20 vils) |
| `dark-age-rush-archers` | needs-public-source | — | — | Dark Age Rush into Archers; Britons, Mayans, Ethiopians, Vikings, Japanese; guide has a brief mention of dark-age feudal archers but no dedicated section; no Hera transcript |
| `double-stable-scouts` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "18 Vils Double Stable Scouts" L504–L588 | section spans two pages (continued at L565); Franks, Huns, Cumans, Malians |
| `drush-fc` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "27+2 Vils Drush Fast Castle" L589–L663 | castle-age destination; distinct from `18pop-feudal-drush` which stays feudal |
| `eagle-range-feudal` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "19 Vils Eagle and Range Feudal Rush" L664–L741 | Aztecs, Mayans, Incas, Lithuanians — YAML name "Eagle and Range Feudal Rush" |
| `ethiopian-2range-archers` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "18 Vils Ethiopian 2-Range Archers for Team Games" L742–L824 | Ethiopia-specific team game build |
| `fast-castle-boom` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "23+2 Vils Fast Castle Boom" L825–L892 | generic FC boom (not Arena); note `23pop-fast-castle-boom-arena` is the Arena variant with transcript |
| `fast-chickens-dark-age` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "18 Vils Fast Chickens Dark Age" L1099–L1149 | YAML name "Fast Chickens Dark Age"; any civ |
| `fc-crossbows` | needs-public-source | — | — | Fast Castle into Crossbows; Britons, Ethiopians, Vietnamese, Mayans, Chinese; guide has 1-Range Archers (L79) going into crossbows but no standalone FC-into-Crossbows section — uncertain match; marking needs-public-source |
| `fc-fortress` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "22+2 Vils Fast Castle on Fortress" L1029–L1098 | Fortress map variant of FC; YAML slug "fc-fortress" matches |
| `fc-knights-pocket` | needs-public-source | — | — | Fast Castle 2-Stable Knights (Pocket); Franks, Lithuanians, Berbers, Huns, Magyars; no Hera section for pocket-role FC Knights specifically — the guide "Knight Rush (Beginner)" L1703 is a different variant; marking needs-public-source |
| `fc-light-cav-relic` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "25+2 Vils Fast Castle Light Cav Relic Control" L893–L961 | section spans two pages (continued at L948) |
| `fire-galleys-and-archers` | needs-public-source | — | — | Fire Galleys and Archers; Italians, Japanese, Vikings, Byzantines, Portuguese; the Fishing Ship Build (L1219) mentions fire galleys as a variation but gives no dedicated build order steps — need public source |
| `fishing-ship-build` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "19 Vils + 3 Fishing Ship Build" L1219–L1265 | hybrid/water map opener; Italians, Japanese, Vikings, Lithuanians, Malians |
| `galley-rush` | needs-public-source | — | — | Feudal Galley Rush; Vikings, Portuguese, Italians, Byzantines, Malay; the Fishing Ship Build section (L1256–L1260) mentions galley rushes as a tip but provides no dedicated build steps — need public source |
| `georgians-healing-scouts` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "16 Vils Georgians Healing Scout Rush" L1336–L1423 | civ-specific; section spans two pages |
| `jurchens-fc-fire-lancer` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "22+2 Vils Jurchens Fast Castle Fire Lancer Rush" L1494–L1576 | civ-specific; section spans two pages |
| `khmer-fast-scouts` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "16 Vils Khmer Super Fast Scout Rush" L1656–L1702 | Khmer-specific; note guide section is "Super Fast" variant |
| `khmer-fc-knights-scorpions` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "23 Vils Khmer Fast Castle Knights and Scorpions" L1577–L1655 | Khmer-specific; section spans two pages |
| `korean-spear-skirm` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "18 Vils Korean Spear Skirm Rush" L1777–L1844 | Korea-specific |
| `maa-archers` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "18 Vils Generic Modern Man-at-Arms Rush" L1266–L1335 | YAML name "18 Pop Generic Man-at-Arms Rush" — close match; guide transitions MAA → archers in feudal; civsRecommended is empty in YAML, guide confirms any civ |
| `maa-into-skirms` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "19 Vils Man-at-Arms Into Skirms" L1904–L1993 | section spans two pages (continued at L1971) |
| `malay-flexible-opening` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "21 Vils Malay Flexible Opening" L1845–L1903 | Malay-specific |
| `men-at-arms-towers` | needs-public-source | — | — | Men-at-Arms + Tower Rush (advanced); Koreans, Incas, Japanese, Ethiopians, Vikings; no Hera section for MAA+Tower combo — guide only covers pure tower rush (Teutons) or pure MAA; need public source |
| `mongol-scouts-ca` | needs-public-source | — | — | Scouts into Cavalry Archers (Mongols/cav-archer civs); Mongols, Huns, Magyars, Japanese, Tatars; guide has "18 Vils Scouts into Cavalry Archers" L2225 but that transcript is already mapped to `18pop-scouts-into-cavalry-archers` — the `mongol-scouts-ca` variant may be a Mongol-specific version; **ambiguity**: could share source with `18pop-scouts-into-cavalry-archers`, but YAML shows different civ focus and slug implies Mongol-specific variation; marking needs-public-source pending clarification |
| `portuguese-monk-rush` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "21+1 Vils Portuguese Monk Rush Strategy" L1994–L2084 | Portugal-specific; section spans two pages |
| `romans-5-maa` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "19 Vils Romans 5 Man-at-Arms Rush" L2085–L2149 | Romans-specific; section spans two pages |
| `scouts-into-skirms` | guide-section | `md/reference/hera-strategy-guide-2025-12-pdftext.txt` | "18 Vils Scouts into Skirms" L2311–L2401 | section spans two pages (continued at L2375); Huns, Mongols, Magyars, Japanese, Tatars |
| `win-water` | needs-public-source | — | — | Win Water (generic water opener); Vikings, Italians, Portuguese, Malay, Japanese, Byzantines; the Fishing Ship Build section (L1219) covers water economy but is not a "Win Water" build — no dedicated Hera section; need public source |

---

## Ambiguities and notes for Task 8

1. **`mongol-scouts-ca` vs `18pop-scouts-into-cavalry-archers`**: Both are "scouts into cav archers" variants. The transcript (`hera-scouts-into-cavalry-archers.md`) maps to `18pop-scouts-into-cavalry-archers`. The `mongol-scouts-ca` slug has a different YAML name ("Scouts into Cavalry Archers") and Mongol-focused civs. The guide section L2225 likely covers the same base build — Task 8 should cross-check whether `mongol-scouts-ca` and `18pop-scouts-into-cavalry-archers` share the same step table or differ.

2. **`fc-crossbows`**: The guide's "1-Range Archers" (L79) ends with "get Crossbowman/Bodkin in Castle Age" but is a feudal opener, not a FC-into-Crossbows build. The `fc-crossbows` YAML is a fast-castle build going into crossbows — no exact guide section. Consider using AoE2 Wiki or Cicero's build order site.

3. **`25pop-knight-rush` / `30pop-fast-imp-generic`**: Both have transcripts. The guide's Knight Rush section (L1703) is labeled "Beginner" with 25+4 pop. Transcript should be checked for step-count alignment. `30pop-fast-imp-generic` has no guide section at all.

4. **`28pop-turks-fast-imp`**: Transcript exists. The guide section (L2481) is "28+2+2 Vils" (28 vil pop + 2 monks + 2 petards implied, or villager counts differ). Task 8 should confirm whether the YAML's 28pop count matches the guide's total or if there is a discrepancy.

5. **`arena-fc-monks`**: The guide covers Portuguese Monk Rush (non-Arena) but there is no Arena FC Monk section. Task 8 needs a separate public source — recommend searching AoE2 Builds (aoe2builds.com), AoE2 Wiki, or YouTube transcripts for Arena FC Monk openings.

6. **`dark-age-rush-archers`**: YAML TR translation is still in EN ("Dark Age Rush into Archers") suggesting it may be a placeholder. No Hera source covers a DA-rush-into-archers specifically; the "19 Vils 1-Range Archers" (L79) goes to feudal, not a dark age rush. Needs clarification of what this build is before sourcing.
