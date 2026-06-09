# Writing & Style Guide (EN + TR)

Keeps prose consistent across the site and across translators/agents. Pair with `GLOSSARY.md`.

> v2 — aligned with the Epic-1 template fix and verified TR usage (2026-06-09).

## Shared rules (both languages)
- **Never invent facts.** Stats, numbers, costs, and timings must be identical across EN and TR.
- **Preserve structure.** Don't change frontmatter keys, IDs, slugs, image paths, URLs, or markdown layout — only the human-readable text.
- **Don't translate placeholder sections.** Unit md bodies currently contain a junk `## Stats summary` table (`Free`/`Melee`) scheduled for deletion (checklist Epic 2). If you encounter it, **skip it and flag it** — never carry it into TR.
- **One idea per sentence.** Aim for ≤ 25 words. Break up run-ons.
- **No marketing tone / no hype.** Concrete and useful beats flashy.
- **Consistent capitalization** of unit/term names (see glossary).

## English voice
- Second person ("you"), present tense.
- Imperative for build-order steps: "Lure the boar," "Send 3 to wood."
- Terse and concrete; cut filler adjectives.
- **Civ description template (canonical, post Epic-1 fix):**
  `{Civ} — a/an {specialty} civilization from {regionNoun}.`
  - Article computed: `/^[aeiou]/i.test(specialty) ? "an" : "a"`.
  - `{regionNoun}` is the **noun** form ("Western Europe", "North Africa") — the current `region` field holds adjectives ("Western European"), which is why the live taglines are broken. Add a `regionNoun` field or map adjective→noun in `build-civilizations.mjs`; don't hand-write off-template intros (the Britons bug).

**Before (live bug):** "Magyars — a Calvary Infantry civilization from Eastern European."
**After:** "Magyars — a Cavalry civilization from Eastern Europe."

**Before (off-template, Britons):** "Foot archer specialists with the longest-range Longbowman."
**After:** "Britons — an Archer civilization from Western Europe."

## Turkish voice
- **Casual register.** Write the way a player explains things to a friend, not a manual. Don't over-formalize ("yapmaktasınız" → "yapıyorsun").
- **Keep English proper nouns** (units, civs, techs, buildings) and established jargon (build order, micro, counter, trash, flank/pocket); attach suffixes with an apostrophe: `Knight'ı`, `Skirmisher'lar`, `counter'ları`, `Mangonel'i`, `pocket'ların`.
- **Use glossary terms verbatim**, including ages: Karanlık / Feodal / Kale Çağı / İmparatorluk Çağı.
- Mirror the EN structure; don't expand or editorialize.
- TR can gloss a kept-English term once per page if helpful, glossary-style: "ayıklamak (sniping)".

**Site-verified reference passages (this is the target voice — taken from shipped TR content):**

> "Micromanagement'ın kısaltması: bir savaş sırasında birimlerden daha fazla değer almak için onları tek tek kontrol etmek — mermilerden kaçınmak, kritik hedefleri ayıklamak (sniping) ve menzilli birimlerle vur-kaç yapmak."
> — `glossary/tr/micro.md`

> "Hangi birim neyi yener. Counter'lar yalnızca maliyetle değil; delici zırh, bonus hasar ve hızla ilgilidir. Bir counter ancak hedefini gerçekten öldürebiliyorsa işe yarar."
> — `ui.ts counters.intro`

**Translation examples (EN → TR, glossary-aware):**

EN: "Cheap ranged DPS; fragile. Fletching/Bodkin are huge power spikes."
TR ⚠: "Ucuz menzilli DPS, ama kırılgan. Fletching ve Bodkin büyük güç sıçramaları."

EN: "Splash devastates massed archers — unless they are microed apart."
TR ⚠: "Alan hasarı yığılmış okçuları biçer — micro ile dağıtılmadıkça."

(⚠ examples are starting points; once you confirm them, drop the marker and they become the reference.)
