// Regenerate src-tauri/resources/notices/THIRD-PARTY-NOTICES.md with cargo-about
// (`cargo install cargo-about --locked --features cli`). cargo-about HTML-escapes
// its data, so entities are decoded for this Markdown file.
//
//   node scripts/generate-notices.mjs

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "src-tauri", "resources", "notices", "THIRD-PARTY-NOTICES.md");
execFileSync("cargo", ["about", "generate", "scripts/about.hbs", "-o", out], { cwd: root, stdio: ["ignore", "inherit", "inherit"] });
const entities = { amp: "&", lt: "<", gt: ">", quot: '"', "#x27": "'", "#39": "'", "#x2F": "/", "#x60": "`", "#x3D": "=" };
const text = readFileSync(out, "utf8").replace(/&(#x[0-9a-fA-F]+|#\d+|[a-z]+);/g, (m, e) => {
  if (e in entities) return entities[e];
  if (e.startsWith("#x")) return String.fromCodePoint(parseInt(e.slice(2), 16));
  if (e.startsWith("#")) return String.fromCodePoint(parseInt(e.slice(1), 10));
  return m;
});
writeFileSync(out, text);
console.log(`Wrote ${out}`);
