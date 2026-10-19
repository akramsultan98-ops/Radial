// Download the pinned PDFium build (bblanchon/pdfium-binaries, BSD-3/Apache-2.0)
// for this platform into src-tauri/resources/pdfium/, verifying its SHA-256.
// Used by local development, tests, CI and the Tauri bundle.
//
//   node scripts/fetch-pdfium.mjs            # current platform
//   node scripts/fetch-pdfium.mjs win-x64    # explicit platform
//
// Re-running is cheap: an existing, verified library is left alone.

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync, cpSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const VERSION = "chromium/7881";
const BUILDS = {
  "linux-x64": { sha256: "1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d", lib: "lib/libpdfium.so" },
  "win-x64": { sha256: "73cc0de638ac2095e7445bf56a38200a5b7c7ca0e9f4ba144598f2457377ac08", lib: "bin/pdfium.dll" },
};

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const platform =
  process.argv[2] ?? (process.platform === "win32" ? "win-x64" : process.platform === "linux" ? "linux-x64" : null);
const build = platform && BUILDS[platform];
if (!build) {
  console.error(`No pinned PDFium build for ${process.platform}/${process.arch}. Supported: ${Object.keys(BUILDS).join(", ")}`);
  process.exit(1);
}

const dest = join(root, "src-tauri", "resources", "pdfium");
const libName = build.lib.split("/").pop();
const stamp = join(dest, ".source");
const expectedStamp = `${VERSION} ${platform} ${build.sha256}\n`;
if (existsSync(join(dest, libName)) && existsSync(stamp) && readFileSync(stamp, "utf8") === expectedStamp) {
  console.log(`PDFium ${VERSION} (${platform}) already present.`);
  process.exit(0);
}

const url = `https://github.com/bblanchon/pdfium-binaries/releases/download/${VERSION}/pdfium-${platform}.tgz`;
console.log(`Downloading ${url}`);
const res = await fetch(url);
if (!res.ok) throw new Error(`Download failed: HTTP ${res.status}`);
const data = Buffer.from(await res.arrayBuffer());
const digest = createHash("sha256").update(data).digest("hex");
if (digest !== build.sha256) {
  throw new Error(`Checksum mismatch for ${url}\n expected ${build.sha256}\n got      ${digest}`);
}

const work = mkdtempSync(join(tmpdir(), "pdfium-"));
try {
  const archive = join(work, "pdfium.tgz");
  writeFileSync(archive, data);
  execFileSync("tar", ["-xzf", archive, "-C", work]);
  rmSync(dest, { recursive: true, force: true });
  mkdirSync(dest, { recursive: true });
  cpSync(join(work, build.lib), join(dest, libName));
  cpSync(join(work, "LICENSE"), join(dest, "LICENSE"));
  cpSync(join(work, "licenses"), join(dest, "licenses"), { recursive: true });
  writeFileSync(stamp, expectedStamp);
  console.log(`PDFium ${VERSION} (${platform}) installed in ${dest}`);
} finally {
  rmSync(work, { recursive: true, force: true });
}
