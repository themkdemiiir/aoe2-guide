import { createRequire } from "module";
const require = createRequire(import.meta.url);
const sharp = require("sharp");
import { readdirSync } from "fs";
import { join } from "path";

const CIVS_DIR = "public/images/aoe2/Civs";
const SIZES = [24, 64, 108];

const files = readdirSync(CIVS_DIR).filter((f) => f.endsWith(".png"));
for (const file of files) {
  const base = join(CIVS_DIR, file.replace(".png", ""));
  const src = join(CIVS_DIR, file);
  for (const s of SIZES) {
    await sharp(src)
      .resize(s, s, { fit: "contain", background: { r: 0, g: 0, b: 0, alpha: 0 } })
      .webp({ quality: 85 })
      .toFile(`${base}-${s}.webp`);
  }
}
console.log(`optimized ${files.length} civ icons → ${files.length * SIZES.length} WebP variants`);
