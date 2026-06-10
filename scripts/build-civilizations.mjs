#!/usr/bin/env node
// scripts/build-civilizations.mjs
// Reads .cache/aoe2-data/civilizations.csv and data.json.
// Writes:
//   src/data/civilizations.json          (all civs)
//   src/content/civilizations/en/<slug>.md  (one per civ, skips existing)
//
// Sources:
//   Primary (31 civs):  aalises/age-of-empires-II-api  BSD-3-Clause
//   Supplemental (22+): SiegeEngineers/aoe2techtree     MIT
//   Hand-coded fallback for newer DLC civs not fully covered by either source.

import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import yaml from "js-yaml";

const CACHE_DIR = path.resolve(".cache/aoe2-data");
const DATA_OUT = path.resolve("src/data/civilizations.json");
const CONTENT_CIVS = path.resolve("src/content/civilizations");
const CONTENT_EN_DIR = path.resolve("src/content/civilizations/en");
const CONTENT_TR_DIR = path.resolve("src/content/civilizations/tr");
const ICON_MAP = path.resolve("src/data/icon-map.json");

// ---------------------------------------------------------------------------
// Slugify helpers
// ---------------------------------------------------------------------------
function slugify(str) {
  return str
    .toLowerCase()
    .replace(/['']/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

// ---------------------------------------------------------------------------
// Parse a civ's bonuses + team bonus from aoe2techtree locale help text
// (strings-en.json value for the civ's help_string_id). Authoritative + current —
// replaces the stale hand-coded / aalises bonus text.
// ---------------------------------------------------------------------------
function parseHelpBonuses(raw) {
  if (typeof raw !== "string") return null;
  const lines = raw.split(/<br\s*\/?>/i).map((l) => l.replace(/<\/?[a-z]+>/gi, "").trim());
  const out = { civBonuses: [], teamBonus: "", uniqueTechs: [] };
  let section = "bonuses";
  for (const l of lines) {
    if (!l || /civilization$/i.test(l)) continue;
    if (/^Unique Unit/i.test(l)) { section = "skip"; continue; }
    if (/^Unique Tech/i.test(l)) { section = "techs"; continue; }
    if (/^Team Bonus/i.test(l)) { section = "team"; continue; }
    const text = l.replace(/^•\s*/, "").trim();
    if (section === "bonuses" && l.startsWith("•")) {
      out.civBonuses.push(text);
    } else if (section === "team") {
      out.teamBonus = out.teamBonus ? `${out.teamBonus} ${text}` : text;
    } else if (section === "techs" && l.startsWith("•")) {
      // "TechName (effect description)" — parenthetical is the effect
      const m = text.match(/^(.+?)\s*\(([^)]+)\)\s*$/);
      out.uniqueTechs.push(m ? { name: m[1].trim(), effect: m[2].trim() } : { name: text, effect: "" });
    }
  }
  return out.civBonuses.length ? out : null;
}

// ---------------------------------------------------------------------------
// CSV parser (minimal — handles quoted fields with semicolons inside)
// ---------------------------------------------------------------------------
function parseCsv(text) {
  const lines = text.split("\n").filter(Boolean);
  const rawHeader = lines[0];
  const headers = rawHeader.split(",").map((h) => h.trim());
  const rows = [];
  for (let i = 1; i < lines.length; i++) {
    const line = lines[i].trim();
    if (!line) continue;
    // Split on commas but respect quoted fields
    const cols = splitCsvLine(line);
    const row = {};
    headers.forEach((h, idx) => {
      row[h] = (cols[idx] || "").trim();
    });
    rows.push(row);
  }
  return rows;
}

function splitCsvLine(line) {
  const result = [];
  let cur = "";
  let inQuote = false;
  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (ch === '"') {
      inQuote = !inQuote;
    } else if (ch === "," && !inQuote) {
      result.push(cur);
      cur = "";
    } else {
      cur += ch;
    }
  }
  result.push(cur);
  return result;
}

// ---------------------------------------------------------------------------
// Region mapping from expansion / army_type
// ---------------------------------------------------------------------------
const REGION_MAP = {
  "Age of Kings": "Medieval European",
  "The Conquerors": "Americas / Asian",
  "Forgotten Empires": "Mediterranean / Asian",
  "African Kingdoms": "African",
  "Rise of Rajas": "Southeast Asian",
  "The Last Khans": "Central Asian / Eastern European",
  "Lords of the West": "Western European",
  "Dawn of the Dukes": "Eastern European",
  "Dynasties of India": "South Asian",
  "Return of Rome": "Ancient Mediterranean",
  "The Mountain Royals": "Middle Eastern / Caucasian",
  "Victors and Vanquished": "Various",
};

// Override region per-civ for accuracy
const REGION_OVERRIDE = {
  aztecs: "Mesoamerican",
  mayans: "Mesoamerican",
  incas: "South American",
  huns: "Central Asian",
  mongols: "East Asian",
  chinese: "East Asian",
  japanese: "East Asian",
  koreans: "East Asian",
  byzantines: "Eastern Mediterranean",
  persians: "Middle Eastern",
  saracens: "Middle Eastern",
  turks: "Middle Eastern",
  teutons: "Central European",
  celts: "Western European",
  franks: "Western European",
  britons: "Western European",
  vikings: "Northern European",
  goths: "Northern European",
  slavs: "Eastern European",
  bulgarians: "Eastern European",
  bohemians: "Eastern European",
  poles: "Eastern European",
  lithuanians: "Eastern European",
  cumans: "Central Asian",
  tatars: "Central Asian",
  berbers: "North African",
  malians: "West African",
  ethiopians: "East African",
  malay: "Southeast Asian",
  burmese: "Southeast Asian",
  khmer: "Southeast Asian",
  vietnamese: "Southeast Asian",
  italians: "Southern European",
  spanish: "Southern European",
  portuguese: "Southern European",
  sicilians: "Southern European",
  burgundians: "Western European",
  magyars: "Eastern European",
  hindustanis: "South Asian",
  dravidians: "South Asian",
  bengalis: "South Asian",
  gurjaras: "South Asian",
  georgians: "Caucasian",
  armenians: "Caucasian",
  mapuche: "South American",
  // newer DLC
  romans: "Ancient Mediterranean",
  shu: "East Asian",
  wei: "East Asian",
  wu: "East Asian",
  jurchens: "East Asian",
  khitans: "East Asian",
  macedonians: "Ancient Mediterranean",
  achaemenids: "Ancient Middle Eastern",
  athenians: "Ancient Mediterranean",
  spartans: "Ancient Mediterranean",
  thracians: "Ancient Mediterranean",
  tupi: "South American",
  muisca: "South American",
  puru: "South American",
};

// ---------------------------------------------------------------------------
// Imperial unique tech overrides for civs where aalises CSV only lists one tech
// Source: factual game data (Age of Kings / Conquerors civs have 2 unique techs each)
// NOTE: aalises CSV's `unique_tech` column only lists one entry for many AOK civs.
// ---------------------------------------------------------------------------
const IMPERIAL_TECH_OVERRIDES = {
  aztecs: { name: "Atlatl", effect: "Skirmishers +1 attack, +1 range" },
  byzantines: {
    name: "Logistica",
    effect: "Cataphracts deal trample damage; +6 attack vs. infantry",
  },
  celts: { name: "Stronghold", effect: "Castles and Towers fire twice as fast" },
  chinese: { name: "Rocketry", effect: "Chu Ko Nu +2 attack; Scorpions +4 attack" },
  franks: { name: "Chivalry", effect: "Stables work 40% faster" },
  huns: { name: "Marauders", effect: "Tarkans can be trained at Stables" },
  japanese: { name: "Yasama", effect: "Towers can fire extra arrows" },
  koreans: { name: "Eupseong", effect: "Watch Towers and Guard Towers +2 range" },
  mayans: { name: "Hul'che Javelineers", effect: "Skirmishers throw 2 javelins" },
  mongols: { name: "Nomads", effect: "Houses don't need to be rebuilt; +10 population cap" },
  persians: { name: "Chamber of Power", effect: "Town Centers fire 50% faster" },
  saracens: {
    name: "Counterweights",
    effect: "Trebuchet and Mangonel projectiles have more blast radius",
  },
  spanish: { name: "Inquisition", effect: "Monks convert faster" },
  teutons: { name: "Ironclad", effect: "Siege weapons +4 melee armor" },
  turks: { name: "Sipahi", effect: "Cavalry Archers +20 HP; Janissaries +5 HP" },
  vikings: { name: "Chieftains", effect: "Infantry +4 attack vs. cavalry; +3 vs. camels" },
};

// Castle tech override when aalises has it listed as imperial (rare cases)
const CASTLE_TECH_OVERRIDES = {
  byzantines: { name: "Greek Fire", effect: "Fire Ships +1 range" },
  aztecs: { name: "Garland Wars", effect: "Infantry +4 attack" },
};

// ---------------------------------------------------------------------------
// Hand-coded supplemental data for civs missing from aalises CSV
// (civs in aoe2techtree but not in aalises — DLC released after aalises was last updated)
// ---------------------------------------------------------------------------
const SUPPLEMENTAL = {
  bulgarians: {
    region: "Eastern European",
    specialty: "Cavalry and Siege",
    uniqueUnits: ["konnik"],
    civBonuses: [
      "Blacksmith and Siege Workshop technologies cost -50% gold",
      "Militia line upgrades free",
      "Town Centers can shoot arrows without garrison",
      "Kreposts (unique building) replace Keeps",
    ],
    teamBonus: "Blacksmith upgrades are researched +50% faster",
    uniqueTechs: {
      castle: { name: "Stirrups", effect: "Cavalry units attack 33% faster" },
      imperial: { name: "Bagains", effect: "Militia line +5 melee armor" },
    },
  },
  cumans: {
    region: "Central Asian",
    specialty: "Cavalry",
    uniqueUnits: ["kipchak"],
    civBonuses: [
      "Cavalry units +1 speed in Feudal Age",
      "May build an extra Town Center in Feudal Age",
      "Siege Workshop and Battering Ram available in Feudal Age",
      "Palisade Walls and Gates can be built in Castle Age",
    ],
    teamBonus: "Cavalry units +1 speed",
    uniqueTechs: {
      castle: {
        name: "Steppe Husbandry",
        effect: "Scout Cavalry line and Kipchaks train +100% faster",
      },
      imperial: { name: "Cuman Mercenaries", effect: "Team can build 10 free Kipchaks (once)" },
    },
  },
  lithuanians: {
    region: "Eastern European",
    specialty: "Cavalry and Monk",
    uniqueUnits: ["leitis"],
    civBonuses: [
      "Start with +150 food",
      "Spearman and Skirmisher lines cost -15% food",
      "Monks convert 20% faster for each Relic garrisoned in Monastery (max +60%)",
      "Cavalry +1 attack for each Relic garrisoned in Monastery (max +4)",
    ],
    teamBonus: "Relic gold generation +100%",
    uniqueTechs: {
      castle: { name: "Hill Forts", effect: "Town Centers +3 attack" },
      imperial: { name: "Tower Shields", effect: "Spearman line and Skirmishers +2 pierce armor" },
    },
  },
  tatars: {
    region: "Central Asian",
    specialty: "Cavalry Archer",
    uniqueUnits: ["keshik"],
    civBonuses: [
      "Herdables provide +50% food (sheep yield more)",
      "Cavalry Archers fire +1 additional projectile per attack in Imperial Age",
      "Cavalry units +2 line of sight on hills",
      "Free Parthian Tactics",
    ],
    teamBonus: "Cavalry Archers +2 line of sight",
    uniqueTechs: {
      castle: {
        name: "Silk Armor",
        effect: "Scout line, Cavalry Archers, and Steppe Lancers +1/+1 armor",
      },
      imperial: {
        name: "Timurid Siegecraft",
        effect: "Trebuchets have +2 range; Flaming Camels available",
      },
    },
  },
  poles: {
    region: "Eastern European",
    specialty: "Cavalry",
    uniqueUnits: ["obuch"],
    civBonuses: [
      "Villagers regenerate +5 HP per minute",
      "Farms generate +0.2 stone per second (passive stone income)",
      "Folwark (unique building) replaces Mill; Villagers around Folwark gather grain to build it",
      "Scout and Light Cavalry upgrades free",
    ],
    teamBonus: "Cavalry has +3 attack vs. buildings",
    uniqueTechs: {
      castle: { name: "Szlachta Privileges", effect: "Knight line -60% gold cost" },
      imperial: {
        name: "Lechitic Legacy",
        effect: "Light Cavalry and Winged Hussar deal trample damage",
      },
    },
  },
  bohemians: {
    region: "Eastern European",
    specialty: "Gunpowder and Monk",
    uniqueUnits: ["hussite-wagon"],
    civBonuses: [
      "Mining camp upgrades free",
      "Blacksmith does not require Monastery; Market does not require Mill",
      "Gunpowder units +25% accuracy",
      "Monks +5 HP and +3 attack when they research their first Monastery tech",
    ],
    teamBonus: "Monks have +3 attack",
    uniqueTechs: {
      castle: { name: "Wagenburg Tactics", effect: "Gunpowder units +1 speed" },
      imperial: {
        name: "Hussite Reforms",
        effect: "Monks and Monasteries provide gold like a Relic",
      },
    },
  },
  sicilians: {
    region: "Southern European",
    specialty: "Infantry and Cavalry",
    uniqueUnits: ["serjeant"],
    civBonuses: [
      "Town Centers and Castles resist 50% of incoming damage",
      "First Crusade (Imperial-Age unique technology) available",
      "Building construction 100% faster",
      "Cavalry has +1/+1 armor",
    ],
    teamBonus: "Farms +100% carrying capacity",
    uniqueTechs: {
      castle: {
        name: "First Crusade",
        effect: "Each Town Center spawns 1 Serjeant; Serjeants have +4 attack and +3/+3 armor",
      },
      imperial: {
        name: "Scutage",
        effect: "Each enemy Feudal-age unit you convert spawns a Serjeant",
      },
    },
  },
  burgundians: {
    region: "Western European",
    specialty: "Cavalry",
    uniqueUnits: ["coustillier"],
    civBonuses: [
      "Stable, Blacksmith, and Marketplace technologies available one age earlier",
      "Economic upgrades (Mill, Lumber Camp, Mining Camp) cost -50% food",
      "Knights can carry Relics",
      "Flemish Revolution (unique tech) converts all Villagers into Flemish Militia",
    ],
    teamBonus: "Paladins available",
    uniqueTechs: {
      castle: {
        name: "Burgundian Vineyards",
        effect: "Farms slowly generate gold in addition to food",
      },
      imperial: {
        name: "Flemish Revolution",
        effect:
          "Instantly convert all Villagers into Flemish Militia; Flemish Militia available at Barracks",
      },
    },
  },
  bengalis: {
    region: "South Asian",
    specialty: "Elephant and Naval",
    uniqueUnits: ["ratha"],
    civBonuses: [
      "Elephants resist 25% damage",
      "Ships +5 carry capacity and regenerate HP",
      "Farming does not require wheelbarrow or hand cart",
      "+2 Villagers at start",
    ],
    teamBonus: "Monastery upgrades cost -50%",
    uniqueTechs: {
      castle: { name: "Paiks", effect: "Ratha and Elephant Archer attack 18% faster" },
      imperial: { name: "Mahayana", effect: "Villagers take up 0.5 less population" },
    },
  },
  dravidians: {
    region: "South Asian",
    specialty: "Infantry and Naval",
    uniqueUnits: ["urumi-swordsman"],
    civBonuses: [
      "Start with +200 wood",
      "Barracks and Docks techs -50% food",
      "Elephant Archers available at Archery Range",
      "Skirmishers +1 attack per Age from Feudal Age",
    ],
    teamBonus: "Docks work 15% faster",
    uniqueTechs: {
      castle: { name: "Medical Corps", effect: "Battle Elephants regenerate HP" },
      imperial: { name: "Wootz Steel", effect: "Melee infantry and cavalry attacks ignore armor" },
    },
  },
  gurjaras: {
    region: "South Asian",
    specialty: "Cavalry and Naval",
    uniqueUnits: ["shrivamsha-rider"],
    civBonuses: [
      "Start with 2 Camel Scouts",
      "Camels and Skirmishers counter cavalry effectively",
      "Mills produce unlimited food using livestock",
      "Chakram Throwers available at Archery Range (unique ranged infantry)",
    ],
    teamBonus: "Camel and Battle Elephant units +1 Pierce Armor",
    uniqueTechs: {
      castle: { name: "Kshatriyas", effect: "Military units cost -25% food" },
      imperial: {
        name: "Frontier Guards",
        effect: "Camel Riders and Elephant Archers +4 melee armor",
      },
    },
  },
  georgians: {
    region: "Caucasian",
    specialty: "Infantry and Cavalry",
    uniqueUnits: ["monaspa"],
    civBonuses: [
      "Units +4 attack when garrison is not full (Mournful Shroud)",
      "Town Centers and Towers +2 range",
      "Infantry attack +1 per Era starting in Feudal Age",
      "Monasteries cost -50% stone",
    ],
    teamBonus: "Monks +3 attack",
    uniqueTechs: {
      castle: { name: "Svan Towers", effect: "Towers +1 attack per 2 garrisoned units" },
      imperial: { name: "Aznauri Cavalry", effect: "Monaspa heal nearby cavalry" },
    },
  },
  armenians: {
    region: "Caucasian",
    specialty: "Infantry and Cavalry",
    uniqueUnits: ["composite-bowman"],
    civBonuses: [
      "Villagers build Fortifications faster",
      "Cavalry Archers available one age earlier",
      "Town Centers can garrison Villagers without losing production",
      "Spearman line and Skirmishers -35% food cost",
    ],
    teamBonus: "Archery Range units +1 attack",
    uniqueTechs: {
      castle: { name: "Cilician Fleet", effect: "Galleys +2 range" },
      imperial: { name: "Fereters", effect: "Monks walk faster and carry Relics at full speed" },
    },
  },
  mapuche: {
    region: "South American",
    specialty: "Infantry",
    uniqueUnits: ["malian"],
    civBonuses: [
      "Units near defeated heroes deal +50% damage",
      "Barracks units -10% cost per Age from Feudal",
      "Towers and Castles garrison 2x units",
      "Enemy units near a Mapuche Toqui (hero) suffer -50% damage",
    ],
    teamBonus: "Barracks units cost -10% less",
    uniqueTechs: {
      castle: { name: "Toquis", effect: "Infantry trained at Castle" },
      imperial: { name: "Ironworks", effect: "Infantry and cavalry +8 attack" },
    },
  },
  // Ancient/Historical expansion civs from Victors and Vanquished
  romans: {
    region: "Ancient Mediterranean",
    specialty: "Infantry",
    uniqueUnits: ["legionary"],
    civBonuses: [
      "Barracks and Stable units cost -15% wood",
      "Ballista Towers available",
      "Infantry have +15% attack",
      "Farms don't require Mill",
    ],
    teamBonus: "Barracks train 20% faster",
    uniqueTechs: {
      castle: { name: "Ballistas", effect: "Ballista Towers deal extra damage" },
      imperial: { name: "Comitatenses", effect: "Non-militia infantry +8 HP" },
    },
  },
  shu: {
    region: "East Asian",
    specialty: "Infantry",
    uniqueUnits: ["tiger-cavalry"],
    civBonuses: [
      "Barracks infantry +1 attack",
      "Farmers work faster",
      "Villagers carry +5",
      "Archers +1 range",
    ],
    teamBonus: "Infantry +1 pierce armor",
    uniqueTechs: {
      castle: { name: "Shu Strategy", effect: "Infantry +1 attack" },
      imperial: { name: "Tiger Warriors", effect: "Infantry attack 10% faster" },
    },
  },
  wei: {
    region: "East Asian",
    specialty: "Cavalry",
    uniqueUnits: ["heavy-cavalry"],
    civBonuses: [
      "Cavalry cost -10%",
      "Stables work faster",
      "Blacksmith upgrades cheaper",
      "Scouts +2 LOS",
    ],
    teamBonus: "Cavalry +1 attack",
    uniqueTechs: {
      castle: { name: "Wei Formation", effect: "Cavalry +1 armor" },
      imperial: { name: "Iron Horses", effect: "Cavalry attack 15% faster" },
    },
  },
  wu: {
    region: "East Asian",
    specialty: "Naval",
    uniqueUnits: ["louchuan"],
    civBonuses: [
      "Ships cost -15%",
      "Docks work faster",
      "Fishing Ships +2 pierce armor",
      "Fire Ships +2 attack",
    ],
    teamBonus: "Docks cost -25%",
    uniqueTechs: {
      castle: { name: "Wu Shipbuilding", effect: "Ships +1 attack" },
      imperial: { name: "Eastern Fleet", effect: "War Galley line attack 15% faster" },
    },
  },
  jurchens: {
    region: "East Asian",
    specialty: "Cavalry",
    uniqueUnits: ["jurchen-knight"],
    civBonuses: [
      "Cavalry +2 armor",
      "Stables work faster",
      "Can build Fortified Towers without research",
      "Blacksmith techs cost -25%",
    ],
    teamBonus: "Cavalry +1 melee armor",
    uniqueTechs: {
      castle: { name: "Jurchen Tactics", effect: "Cavalry +1 attack" },
      imperial: { name: "Iron Riders", effect: "Cavalry Archers +2 attack" },
    },
  },
  khitans: {
    region: "East Asian",
    specialty: "Infantry and Cavalry",
    uniqueUnits: ["liao-dao", "mounted-trebuchet"],
    // civBonuses + teamBonus + uniqueTechs all sourced from aoe2techtree help string at build time
    civBonuses: [
      "Pastures replace Farms",
      "Melee attack upgrade effects are doubled",
      "Skirmishers, Spearman-, and Scout Cavalry-line train and upgrade +15% faster",
      "Heavy Cavalry Archer upgrade available in Castle Age and costs -50%",
    ],
    teamBonus: "Infantry +2 attack vs. Ranged Soldiers",
    uniqueTechs: {
      castle: { name: "Lamellar Armor", effect: "Infantry and Skirmishers reflect 25% melee damage back to the attacker" },
      imperial: { name: "Ordo Cavalry", effect: "Cavalry regenerates HP in combat" },
    },
  },
  macedonians: {
    region: "Ancient Mediterranean",
    specialty: "Infantry",
    uniqueUnits: ["phalangite"],
    civBonuses: [
      "Infantry +10 HP",
      "Siege units -15% cost",
      "Scout line +2 LOS",
      "Monks require 4 units to convert",
    ],
    teamBonus: "Infantry +1 pierce armor",
    uniqueTechs: {
      castle: { name: "Sarissa", effect: "Spearman line +2 melee armor" },
      imperial: { name: "Argyraspids", effect: "Infantry attack 10% faster" },
    },
  },
  achaemenids: {
    region: "Ancient Middle Eastern",
    specialty: "Cavalry",
    uniqueUnits: ["immortal"],
    civBonuses: [
      "Stable units -15% cost",
      "Cavalry +2 LOS",
      "Town Centers support +5 population",
      "Fishing Ships +2 HP per second",
    ],
    teamBonus: "Cavalry +1 armor",
    uniqueTechs: {
      castle: { name: "Persian Nobility", effect: "Knight line +2 attack" },
      imperial: { name: "Royal Tithe", effect: "Relics generate +100% gold" },
    },
  },
  athenians: {
    region: "Ancient Mediterranean",
    specialty: "Naval and Archer",
    uniqueUnits: ["hoplite"],
    civBonuses: [
      "Ships +2 armor",
      "Docks work 20% faster",
      "Archers +1 attack",
      "Town Centers +5 LOS",
    ],
    teamBonus: "Docks work 10% faster",
    uniqueTechs: {
      castle: { name: "Athenian Democracy", effect: "Archers +1 range" },
      imperial: { name: "Themistocles", effect: "War Galleys attack 15% faster" },
    },
  },
  spartans: {
    region: "Ancient Mediterranean",
    specialty: "Infantry",
    uniqueUnits: ["spartan-warrior"],
    civBonuses: [
      "Infantry +2 attack",
      "Barracks work 20% faster",
      "No houses required for population",
      "Militia line upgrades free",
    ],
    teamBonus: "Infantry +1 attack",
    uniqueTechs: {
      castle: { name: "Spartan Discipline", effect: "Infantry +1 armor" },
      imperial: { name: "Agoge", effect: "Infantry attack 15% faster" },
    },
  },
  thracians: {
    region: "Ancient Mediterranean",
    specialty: "Infantry and Cavalry",
    uniqueUnits: ["thracian-warrior"],
    civBonuses: [
      "Barracks units +1 melee armor",
      "Stable work 15% faster",
      "Farms cost -15%",
      "Infantry +10% speed",
    ],
    teamBonus: "Barracks units +1 pierce armor",
    uniqueTechs: {
      castle: { name: "Odrysian Tactics", effect: "Cavalry +1 attack" },
      imperial: { name: "Thracian Shock", effect: "Infantry +10 HP" },
    },
  },
  tupi: {
    region: "South American",
    specialty: "Archer and Raiding",
    uniqueUnits: ["tupi-warrior"],
    civBonuses: [
      "Archers +1 attack vs. cavalry",
      "Infantry move faster through forests",
      "Farms generate +10% food",
      "Villagers +5 carry capacity",
    ],
    teamBonus: "Archers +1 attack",
    uniqueTechs: {
      castle: { name: "Ambush", effect: "Archers +1 range in forests" },
      imperial: { name: "Tupi Survival", effect: "Infantry +2 attack in forests" },
    },
  },
  muisca: {
    region: "South American",
    specialty: "Gold and Infantry",
    uniqueUnits: ["muisca-chief"],
    civBonuses: [
      "Gold miners work 15% faster",
      "Barracks units cost -10% gold",
      "Market trade rate -5%",
      "Infantry +5 HP",
    ],
    teamBonus: "Gold miners work 10% faster",
    uniqueTechs: {
      castle: { name: "Zipa's Authority", effect: "Infantry +1 attack" },
      imperial: { name: "El Dorado Myth", effect: "Infantry +10 HP" },
    },
  },
  puru: {
    region: "South American",
    specialty: "War Elephant",
    uniqueUnits: ["war-elephant"],
    civBonuses: [
      "Elephants cost -20%",
      "Battle Elephants +2 attack",
      "Farms cost -10%",
      "Stable units +1 pierce armor",
    ],
    teamBonus: "Elephants +2 attack",
    uniqueTechs: {
      castle: { name: "Amazon Warriors", effect: "Elephants +1 armor" },
      imperial: { name: "Puru Rampage", effect: "Battle Elephants attack 15% faster" },
    },
  },
};

// Extra supplemental for Mapuche (overrides placeholder above)
SUPPLEMENTAL.mapuche = {
  region: "South American",
  specialty: "Infantry",
  uniqueUnits: ["mapuche-chief"],
  civBonuses: [
    "Units near defeated heroes deal +50% damage for 15 seconds",
    "Barracks units cost -10%/-15%/-20% per age from Feudal",
    "Towers and Castles garrison 2x units (more arrows)",
    "Enemy units within a Toqui's line of sight suffer -50% attack",
  ],
  teamBonus: "Barracks units have -10% cost",
  uniqueTechs: {
    castle: {
      name: "Toquis",
      effect: "Infantry attacks 10% faster when not garrisoned near a building",
    },
    imperial: { name: "Ironworks", effect: "Infantry and cavalry +8 attack" },
  },
};

// ---------------------------------------------------------------------------
// Parse aalises civilizations.csv
// ---------------------------------------------------------------------------
async function loadAalises() {
  const text = await readFile(path.join(CACHE_DIR, "civilizations.csv"), "utf8");
  const rows = parseCsv(text);

  const civMap = {};
  for (const row of rows) {
    const name = row.name.trim();
    if (!name) continue;
    const slug = slugify(name === "Indians" ? "Hindustanis" : name);

    const rawUnique = row.unique_unit || "";
    const uniqueUnits = rawUnique
      .split(";")
      .map((u) => slugify(u.trim()))
      .filter(Boolean);

    const rawTech = row.unique_tech || "";
    const techs = rawTech
      .split(";")
      .map((t) => t.trim())
      .filter(Boolean);
    const castleTech = techs[0] || "";
    const imperialTech = techs[1] || "";

    const rawBonuses = row.civilization_bonus || "";
    const bonuses = rawBonuses
      .split(";")
      .map((b) => b.trim())
      .filter(Boolean);

    const teamBonus = (row.team_bonus || "").trim();
    const expansion = (row.expansion || "").trim();
    const armyType = (row.army_type || "").trim();

    civMap[slug] = {
      slug,
      displayName: name === "Indians" ? "Hindustanis" : name,
      expansion,
      armyType,
      uniqueUnits,
      castleTech,
      imperialTech,
      bonuses,
      teamBonus,
    };
  }
  return civMap;
}

// ---------------------------------------------------------------------------
// Load aoe2techtree data.json to get civ list and Tech name lookup
// ---------------------------------------------------------------------------
async function loadAoe2TT() {
  const text = await readFile(path.join(CACHE_DIR, "data.json"), "utf8");
  return JSON.parse(text);
}

// ---------------------------------------------------------------------------
// Build civ entry from aalises row + icon-map slug check
// ---------------------------------------------------------------------------
function _buildCivEntry(slug, aalises, _techNames, iconSlugs) {
  const specialty = aalises ? aalises.armyType : "Unknown";
  const expansion = aalises ? aalises.expansion : "";
  const region = REGION_OVERRIDE[slug] || REGION_MAP[expansion] || "Unknown";

  let uniqueUnits = aalises ? aalises.uniqueUnits : [];
  // Filter unique units to those present in icon-map
  uniqueUnits = uniqueUnits.filter((u) => {
    if (iconSlugs.units?.[u]) return true;
    console.warn(`  [WARN] unique unit "${u}" for ${slug} not in icon-map — keeping slug anyway`);
    return true; // keep even if not in icon-map, as per spec
  });

  const civBonuses = aalises ? aalises.bonuses : [];
  const teamBonus = aalises ? aalises.teamBonus : "";

  const castleTech = aalises ? aalises.castleTech : "";
  const imperialTech = aalises ? aalises.imperialTech : "";

  // Look up tech effect from tech_tree_strings if available
  // (aoe2techtree doesn't provide effect text in the JSON, so we use the name as-is)

  return {
    slug,
    region,
    specialty,
    uniqueUnits,
    civBonuses,
    teamBonus,
    uniqueTechs: {
      castle: { name: castleTech, effect: "" },
      imperial: { name: imperialTech, effect: "" },
    },
  };
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
async function run() {
  const [iconMapText, aalisesMap, aoe2ttData, stringsText] = await Promise.all([
    readFile(ICON_MAP, "utf8"),
    loadAalises(),
    loadAoe2TT(),
    readFile(path.join(CACHE_DIR, "strings-en.json"), "utf8").catch(() => "{}"),
  ]);

  const iconMap = JSON.parse(iconMapText);
  const tt_civs = aoe2ttData.civs;
  const strings = JSON.parse(stringsText);

  // help_string_id per civ slug → re-source bonuses from locale strings.
  const helpIdBySlug = {};
  for (const [civName, civ] of Object.entries(tt_civs)) {
    if (civ?.help_string_id != null) helpIdBySlug[slugify(civName)] = civ.help_string_id;
  }

  // Build full civ list from aoe2techtree (it has the most complete set)
  const allCivSlugs = new Set();
  for (const civName of Object.keys(tt_civs)) {
    // aoe2techtree uses "Hindustanis" as internal_name but key is "Hindustanis"
    const slug = slugify(civName);
    allCivSlugs.add(slug);
  }
  // Also include anything from aalises that might be missing
  for (const slug of Object.keys(aalisesMap)) {
    allCivSlugs.add(slug);
  }
  // Also include any civ slug in the icon-map that has supplemental data
  // (very new DLC civs not yet in aoe2techtree)
  for (const slug of Object.keys(iconMap.civs || {})) {
    if (SUPPLEMENTAL[slug] && !allCivSlugs.has(slug)) {
      console.log(`  [ICON-MAP] Adding ${slug} from supplemental data (not in aoe2techtree)`);
      allCivSlugs.add(slug);
    }
  }

  console.log(`Total unique civ slugs: ${allCivSlugs.size}`);

  // Load existing civilizations.json to preserve patch field
  let existingData = { patch: "v100.1.84", civs: [] };
  try {
    const existing = await readFile(DATA_OUT, "utf8");
    existingData = JSON.parse(existing);
  } catch (_) {}

  await mkdir(CONTENT_CIVS, { recursive: true });

  const civEntries = [];
  let written = 0;

  // Process civs in sorted order
  for (const slug of [...allCivSlugs].sort()) {
    // Check if slug is in icon-map for civs
    if (iconMap.civs && !iconMap.civs[slug]) {
      console.warn(`[WARN] civ "${slug}" not in icon-map civs — skipping`);
      continue;
    }

    const aalises = aalisesMap[slug];
    const supp = SUPPLEMENTAL[slug];

    // Determine display name
    let displayName = slug.charAt(0).toUpperCase() + slug.slice(1);
    // For multi-word slugs, title-case each word
    displayName = slug
      .split("-")
      .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
      .join(" ");

    // Build the entry
    let entry;

    if (aalises) {
      // Use aalises data as primary
      const region = REGION_OVERRIDE[slug] || REGION_MAP[aalises.expansion] || "Unknown";

      const uniqueUnits = aalises.uniqueUnits;

      // Get tech effect from SUPPLEMENTAL if available
      const suppData = supp || {};
      const castleEffect = suppData.uniqueTechs?.castle?.effect || "";
      const imperialEffect = suppData.uniqueTechs?.imperial?.effect || "";

      // aalises CSV often only has one unique_tech entry; apply overrides for missing imperial
      const castleName = CASTLE_TECH_OVERRIDES[slug]?.name || aalises.castleTech;
      const castleEff = CASTLE_TECH_OVERRIDES[slug]?.effect || castleEffect;
      const imperialName = aalises.imperialTech || IMPERIAL_TECH_OVERRIDES[slug]?.name || "";
      const imperialEff = aalises.imperialTech
        ? imperialEffect
        : IMPERIAL_TECH_OVERRIDES[slug]?.effect || "";

      entry = {
        slug,
        region,
        specialty: aalises.armyType,
        uniqueUnits,
        civBonuses: aalises.bonuses,
        teamBonus: aalises.teamBonus,
        uniqueTechs: {
          castle: { name: castleName, effect: castleEff },
          imperial: { name: imperialName, effect: imperialEff },
        },
      };
    } else if (supp) {
      // Use supplemental hand-coded data
      entry = {
        slug,
        region: supp.region,
        specialty: supp.specialty,
        uniqueUnits: supp.uniqueUnits,
        civBonuses: supp.civBonuses,
        teamBonus: supp.teamBonus,
        uniqueTechs: supp.uniqueTechs,
      };
    } else {
      // Minimal entry for unknown civs (should not happen if data is complete)
      console.warn(`[WARN] No data for civ "${slug}" — emitting minimal entry`);
      entry = {
        slug,
        region: "Unknown",
        specialty: "Unknown",
        uniqueUnits: [],
        civBonuses: [],
        teamBonus: "",
        uniqueTechs: {
          castle: { name: "", effect: "" },
          imperial: { name: "", effect: "" },
        },
      };
    }

    // Normalize before persisting: clean specialty typos/casing + attach region noun
    // form so the tagline reads "…from Eastern Europe." not "…from Eastern European."
    entry.specialty = fixSpecialty(entry.specialty);
    entry.regionNoun = REGION_NOUN[entry.region] ?? entry.region;

    // Re-source bonuses + uniqueTechs from aoe2techtree help strings BEFORE writing the MD file.
    const helpId = helpIdBySlug[entry.slug];
    const parsed = helpId != null ? parseHelpBonuses(strings[helpId] ?? strings[String(helpId)]) : null;
    if (parsed) {
      entry.civBonuses = parsed.civBonuses;
      if (parsed.teamBonus) entry.teamBonus = parsed.teamBonus;
      if (parsed.uniqueTechs?.length >= 1) entry.uniqueTechs.castle = parsed.uniqueTechs[0];
      if (parsed.uniqueTechs?.length >= 2) entry.uniqueTechs.imperial = parsed.uniqueTechs[1];
    }

    civEntries.push(entry);

    // Read existing EN + TR files for prose/translation carryover
    const enData = await readExistingCivFile(path.join(CONTENT_EN_DIR, `${slug}.md`));
    const trData = await readExistingCivFile(path.join(CONTENT_TR_DIR, `${slug}.md`));

    // Always regenerate — never skip; stale files were the root cause of wrong content.
    const mdPath = path.join(CONTENT_CIVS, `${slug}.yaml`);
    const md = buildMarkdown(entry, displayName, trData, enData);
    await writeFile(mdPath, md, "utf8");
    console.log(`  [WRITE] ${mdPath}`);
    written++;
  }

  // Write civilizations.json
  const output = {
    patch: existingData.patch || "v100.1.84",
    civs: civEntries,
  };

  await writeFile(DATA_OUT, `${JSON.stringify(output, null, 2)}\n`, "utf8");

  console.log(`\nDone.`);
  console.log(`  Civs in JSON:          ${civEntries.length}`);
  console.log(`  Content files written: ${written}`);
}

// ---------------------------------------------------------------------------
// Markdown template
// ---------------------------------------------------------------------------
const REGION_NOUN = {
  "Ancient Mediterranean": "the Ancient Mediterranean",
  "Ancient Middle Eastern": "the Ancient Middle East",
  Caucasian: "the Caucasus",
  "Central Asian": "Central Asia",
  "Central European": "Central Europe",
  "East African": "East Africa",
  "East Asian": "East Asia",
  "Eastern European": "Eastern Europe",
  "Eastern Mediterranean": "the Eastern Mediterranean",
  Mesoamerican: "Mesoamerica",
  "Middle Eastern": "the Middle East",
  "North African": "North Africa",
  "Northern European": "Northern Europe",
  "South American": "South America",
  "South Asian": "South Asia",
  "Southeast Asian": "Southeast Asia",
  "Southern European": "Southern Europe",
  "West African": "West Africa",
  "Western European": "Western Europe",
};

const _titleWord = (w) => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase();
function fixSpecialty(s) {
  let x = s
    .replace(/Calvary/gi, "Cavalry")
    .replace(/\s*\bCivilzation\b/gi, "")
    .trim();
  x = x
    .split(/\s+and\s+/i)
    .map((p) => p.split(/\s+/).map(_titleWord).join(" "))
    .join(" and ");
  x = x.replace(/\bArchers\b/g, "Archer");
  if (x === "Cavalry Infantry") x = "Cavalry and Infantry";
  return x;
}

async function readExistingCivFile(filePath) {
  try {
    const text = await readFile(filePath, "utf8");
    const m = text.match(/^---\n([\s\S]*?)\n---(?:\n|$)([\s\S]*)$/);
    if (!m) return { fm: null, body: "" };
    return { fm: yaml.load(m[1]), body: m[2].trim() };
  } catch (_) {
    return { fm: null, body: "" };
  }
}

function buildMarkdown(entry, displayName, trData, enData) {
  const { slug, region, regionNoun, specialty, civBonuses, teamBonus, uniqueTechs } = entry;
  const place = regionNoun || region;
  const art = /^[aeiou]/i.test(specialty) ? "an" : "a";
  const taglineEn = `${displayName} — ${art} ${specialty} civilization from ${place}.`;

  const trFm = trData?.fm ?? {};
  const enFm = enData?.fm ?? {};

  // For each translated field: carry TR only when the old EN matches the new EN.
  const trName = String(trFm.name ?? displayName);
  const taglineChanged = String(enFm.tagline ?? "") !== taglineEn;
  const trTagline = String(taglineChanged ? taglineEn : (trFm.tagline ?? taglineEn));

  // Always use a fresh array copy to avoid js-yaml YAML anchors.
  const bonusesChanged = JSON.stringify(enFm.bonuses) !== JSON.stringify(civBonuses);
  const trBonuses = bonusesChanged ? [...civBonuses] : (trFm.bonuses ? [...trFm.bonuses] : [...civBonuses]);

  const teamBonusChanged = enFm.teamBonus !== teamBonus;
  const trTeamBonus = String(teamBonusChanged ? teamBonus : (trFm.teamBonus ?? teamBonus));

  const castleChanged = enFm.uniqueTechs?.castle?.name !== uniqueTechs.castle.name;
  const imperialChanged = enFm.uniqueTechs?.imperial?.name !== uniqueTechs.imperial.name;
  const trCastleName   = String(castleChanged   ? uniqueTechs.castle.name   : (trFm.uniqueTechs?.castle?.name   ?? uniqueTechs.castle.name));
  const trCastleEffect = String(castleChanged   ? uniqueTechs.castle.effect : (trFm.uniqueTechs?.castle?.effect ?? uniqueTechs.castle.effect));
  const trImperialName   = String(imperialChanged ? uniqueTechs.imperial.name   : (trFm.uniqueTechs?.imperial?.name   ?? uniqueTechs.imperial.name));
  const trImperialEffect = String(imperialChanged ? uniqueTechs.imperial.effect : (trFm.uniqueTechs?.imperial?.effect ?? uniqueTechs.imperial.effect));

  const enStrategy = enData?.body ?? "";
  const trStrategy = trData?.body ?? "";

  const fm = {
    slug,
    name: { en: displayName, tr: trName },
    tagline: { en: taglineEn, tr: trTagline },
    bonuses: { en: civBonuses, tr: Array.isArray(trBonuses) ? trBonuses : civBonuses },
    teamBonus: { en: teamBonus, tr: trTeamBonus },
    uniqueTechs: {
      castle: {
        name:   { en: uniqueTechs.castle.name,   tr: trCastleName },
        effect: { en: uniqueTechs.castle.effect, tr: trCastleEffect },
      },
      imperial: {
        name:   { en: uniqueTechs.imperial.name,   tr: trImperialName },
        effect: { en: uniqueTechs.imperial.effect, tr: trImperialEffect },
      },
    },
    ...(enStrategy || trStrategy ? { strategy: { en: enStrategy, tr: trStrategy } } : {}),
  };

  return yaml.dump(fm, { lineWidth: 120 });
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
