import { describe, it, expect } from "vitest";
import { parseHelp } from "../scripts/lib/parse-help.mjs";

const EN = [
  "Archer civilization", "",
  "• Shepherds work +25% faster",
  "• Town Centers cost -50% wood starting in the Castle Age", "",
  "Unique Unit:", "Longbowman (Archer)", "",
  "Unique Tech:",
  "• Yeomen (Foot archers +1 range; Towers +2 attack)",
  "• Warwolf (Trebuchets deal blast damage and are more accurate)", "",
  "Team Bonus:", "Archery Ranges work +10% faster",
].join("<br>");

const TR = [
  "Yaya Okçu medeniyeti", "",
  "• Çobanlar %25 daha hızlı çalışır",
  "• Şehir Merkezleri, Kale Çağı'ndan itibaren %50 daha az odun gerektirir", "",
  "Özgün Birim:", "Uzun Yay Okçusu (Yaya Okçu)", "",
  "Özgün Teknoloji:",
  "• Levazımcı (Yaya Okçulara 1 menzil; Gözcü Kulesi türlerine 2 saldırı)",
  "• Savaş Kurdu (Katapultlar patlama hasarı verir ve daha isabetlidir)", "",
  "Takım Bonusu:", "Okçuluk Binası %10 daha hızlı çalışır",
].join("<br>");

describe("parseHelp", () => {
  it("parses English help text", () => {
    const r = parseHelp(EN, "en");
    expect(r.civType).toBe("Archer");
    expect(r.civBonuses).toEqual([
      "Shepherds work +25% faster",
      "Town Centers cost -50% wood starting in the Castle Age",
    ]);
    expect(r.teamBonus).toBe("Archery Ranges work +10% faster");
    expect(r.uniqueTechs).toEqual([
      { name: "Yeomen", effect: "Foot archers +1 range; Towers +2 attack" },
      { name: "Warwolf", effect: "Trebuchets deal blast damage and are more accurate" },
    ]);
  });

  it("parses Turkish help text with localized section markers", () => {
    const r = parseHelp(TR, "tr");
    expect(r.civType).toBe("Yaya Okçu");
    expect(r.civBonuses).toEqual([
      "Çobanlar %25 daha hızlı çalışır",
      "Şehir Merkezleri, Kale Çağı'ndan itibaren %50 daha az odun gerektirir",
    ]);
    expect(r.teamBonus).toBe("Okçuluk Binası %10 daha hızlı çalışır");
    expect(r.uniqueTechs.map((t) => t.effect)).toEqual([
      "Yaya Okçulara 1 menzil; Gözcü Kulesi türlerine 2 saldırı",
      "Katapultlar patlama hasarı verir ve daha isabetlidir",
    ]);
  });

  it("returns null when there are no bullet bonuses", () => {
    expect(parseHelp("Archer civilization<br>Unique Unit:<br>Longbowman (Archer)", "en")).toBeNull();
  });

  it("throws on an unknown lang", () => {
    expect(() => parseHelp("x", "de")).toThrow(/unknown lang/);
  });
});
