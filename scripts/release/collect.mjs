#!/usr/bin/env node
// Copies a target's bundles into one folder with clear names:
//   DarkLoad_<version>_<platform>_<arch>[-setup].<ext>
// and creates a portable archive containing the application binary.
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { basename, join } from "node:path";
import { parseArgs } from "node:util";

const { values } = parseArgs({
  options: {
    target: { type: "string" },
    platform: { type: "string" },
    arch: { type: "string" },
    out: { type: "string", default: "release" },
  },
});
const { target, platform, arch, out } = values;
if (!target || !platform || !arch) {
  console.error("Usage: collect.mjs --target <triple> --platform <windows|linux> --arch <x64|arm64> [--out dir]");
  process.exit(1);
}

const version = JSON.parse(readFileSync("package.json", "utf8")).version;
const releaseDir = join("src-tauri", "target", target, "release");
const bundleDir = join(releaseDir, "bundle");
const prefix = `DarkLoad_${version}_${platform}_${arch}`;
mkdirSync(out, { recursive: true });

function files(dir, predicate) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir)
    .map((name) => join(dir, name))
    .filter((p) => statSync(p).isFile() && predicate(basename(p)));
}

const collected = [];
function take(src, name) {
  const dest = join(out, name);
  copyFileSync(src, dest);
  collected.push(name);
  if (existsSync(`${src}.sig`)) {
    copyFileSync(`${src}.sig`, `${dest}.sig`);
    collected.push(`${name}.sig`);
  }
}

const rules =
  platform === "windows"
    ? [
        ["nsis", (n) => n.endsWith("-setup.exe"), `${prefix}-setup.exe`],
        ["msi", (n) => n.endsWith(".msi"), `${prefix}.msi`],
      ]
    : [
        ["appimage", (n) => n.endsWith(".AppImage"), `${prefix}.AppImage`],
        ["deb", (n) => n.endsWith(".deb"), `${prefix}.deb`],
        ["rpm", (n) => n.endsWith(".rpm"), `${prefix}.rpm`],
      ];

for (const [dir, predicate, name] of rules) {
  const [match] = files(join(bundleDir, dir), predicate);
  if (match) take(match, name);
  else console.warn(`No ${dir} bundle found for ${target}.`);
}

// Portable archive: the application binary plus license and notices.
const staging = join(out, ".portable");
rmSync(staging, { recursive: true, force: true });
mkdirSync(staging, { recursive: true });
const binary = platform === "windows" ? "darkload.exe" : "darkload";
copyFileSync(join(releaseDir, binary), join(staging, binary));
for (const doc of ["LICENSE", "README.md", "THIRD_PARTY_NOTICES.md"]) copyFileSync(doc, join(staging, doc));
if (platform === "windows") {
  const name = `${prefix}_portable.zip`;
  execFileSync("tar", ["-a", "-c", "-f", join("..", name), "."], { cwd: staging, stdio: "inherit" });
  collected.push(name);
} else {
  const name = `${prefix}_portable.tar.gz`;
  execFileSync("tar", ["-czf", join("..", name), "."], { cwd: staging, stdio: "inherit" });
  collected.push(name);
}
rmSync(staging, { recursive: true, force: true });

console.log(`Collected:\n  ${collected.join("\n  ")}`);
