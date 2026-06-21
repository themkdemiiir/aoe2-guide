// Controlled EN→TR taxonomy for unit roles (editorial; no source equivalent).
// Proper nouns kept in EN (e.g. "Britons").
export const ROLE_TR = {
  "melee infantry": "yakın dövüş piyadesi",
  "anti-cavalry infantry": "süvari avcısı piyade",
  "light infantry": "hafif piyade",
  "ranged infantry": "menzilli piyade",
  "anti-archer ranged": "okçu avcısı menzilli birim",
  "mounted archer": "atlı okçu",
  "gunpowder ranged": "barutlu menzilli birim",
  "light cavalry": "hafif süvari",
  "heavy cavalry": "ağır süvari",
  "anti-cavalry cavalry": "süvari avcısı süvari",
  "support / conversion": "destek / din değiştirme",
  siege: "kuşatma",
  "siege gunpowder": "barutlu kuşatma",
  "siege long-range": "uzun menzilli kuşatma",
  naval: "deniz birimi",
  "anti-ship naval": "gemi avcısı deniz birimi",
  "naval suicide": "intihar deniz birimi",
  "naval siege": "deniz kuşatma birimi",
  "economic naval": "ekonomik deniz birimi",
  "utility naval": "yardımcı deniz birimi",
  "unique unit": "özgün birim",
  "Unique foot archer (Britons)": "Özgün yaya okçu (Britons)",
};

export function roleTr(en) {
  if (!(en in ROLE_TR)) throw new Error(`roleTr: unmapped role "${en}"`);
  return ROLE_TR[en];
}
