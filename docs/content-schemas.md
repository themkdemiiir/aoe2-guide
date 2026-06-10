# Content Schemas Reference

Authoritative source: `src/content/config.ts`. This document is the human-readable companion — if the two disagree, the code wins.

## civilizations

File: `src/content/civilizations/<lang>/<slug>.md` (e.g. `src/content/civilizations/en/britons.md`)

```yaml
---
slug: britons                                    # required, English-canonical
name: Britons                                    # required, localized
tagline: Foot archer specialists.                # required, localized one-liner
bonuses:                                         # required, array of localized strings
  - Town Centers cost 50% less wood from Castle Age.
  - Foot archers gain +1/+2 range.
teamBonus: Archery Ranges work 20% faster.       # required, localized
uniqueTechs:                                     # required
  castle:
    name: Yeomen
    effect: Foot archers gain +1 range; Towers +2 attack.
  imperial:
    name: Warwolf
    effect: Trebuchets do blast damage.
---

Long-form prose...
```

Lang-agnostic facts (era, region, tier, unique unit slugs, strong-against, weak-against) live in `src/data/civilizations.json`.

## build-orders

File: `src/content/build-orders/<lang>/<slug>.md`

```yaml
---
slug: 21pop-archers
name: "21pop Archer Rush"                        # localized
difficulty: beginner | intermediate | advanced
targetAge: feudal | castle | imperial
durationMin: 14
civsRecommended: [britons, mayans, ethiopians]   # civ slugs
steps:                                           # 6–12 entries recommended
  - { villagers: 6,  time: "0:00",  assign: "6 → sheep under TC" }
  - { villagers: 9,  time: "1:05",  assign: "+3 → wood, build lumber camp" }
  - { villagers: 19, time: "4:20",  assign: "+2 → wood", note: "Research Loom; click Feudal" }
source:
  author: a pro player's build guide
  url: https://aoecompanion.com/...
---

Prose: when to use this build, key milestones, common mistakes...
```

## units

File: `src/content/units/<lang>/<slug>.md`

```yaml
---
slug: longbowman
name: Longbowman                                 # localized
role: Unique foot archer (Britons)               # localized
civ: britons                                     # optional; civ slug if unique unit
---

How to use the unit, common compositions, counters...
```

Numeric stats live in `src/data/unit-stats.json`:

```json
{
  "slug": "longbowman",
  "hp": 35,
  "attack": 7,
  "range": 6,
  "minRange": 0,
  "cost": { "wood": 35, "food": 0, "gold": 40, "stone": 0 },
  "trainTime": 18,
  "armorPiercing": 0,
  "armorMelee": 0
}
```

## maps

File: `src/content/maps/<lang>/<slug>.md`

```yaml
---
slug: arabia
name: Arabia                                     # localized
type: open | closed | hybrid | water | nomad
size: tiny | small | medium | large              # optional
recommendedCivs: [mongols, mayans, franks]
---

Playstyle, opening, mid-game, late-game, watch-out-for...
```

## matchups

File: `src/content/matchups/<lang>/<slug>.md` (slug e.g. `britons-vs-franks`)

```yaml
---
slug: britons-vs-franks
civA: britons
civB: franks
difficulty: even | favored | unfavored
---

Analysis...
```

## beginner

File: `src/content/beginner/<lang>/<NN>-<topic>.md` (e.g. `01-resources.md`)

```yaml
---
slug: 01-resources
title: Resources & Villagers                     # localized
order: 1                                         # chapter sequence
prereq: [00-getting-started]                     # optional, slugs of prerequisites
---

Chapter content...
```

## glossary

File: `src/content/glossary/<lang>/<slug>.md`

```yaml
---
slug: boom
term: Boom                                        # localized
letter: B                                         # for alphabetical index
---

Definition + examples...
```

## Common validation errors

- **`Invalid type. Expected string, received undefined`** — you forgot a required field. Check the schema in `config.ts`.
- **`Expected one of: feudal | castle | imperial`** — `targetAge` typo.
- **`steps.0.time: Invalid string`** — `time` must be a string in `"m:ss"` format, not a number.
- **`Cannot find entry with slug X`** — `getLocalizedEntry` couldn't resolve; verify the file name matches the slug.

Run `pnpm build` to see exact error locations.
