#!/usr/bin/env node
// Builds latest.json for the Tauri updater from signed release artifacts.
// Skipped (with a message) when no signatures exist, so unsigned releases never claim updates.
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [dir = "release", tag = ""] = process.argv.slice(2);
const repository = process.env.GITHUB_REPOSITORY;
const version = tag.replace(/^v/, "");
if (!repository || !version) {
  console.error("GITHUB_REPOSITORY and a tag are required.");
  process.exit(1);
}

const names = existsSync(dir) ? readdirSync(dir) : [];
const platforms = {};
const archKey = { x64: "x86_64", arm64: "aarch64" };
for (const name of names) {
  const match = /^DarkLoad_[^_]+_(windows|linux)_(x64|arm64)(-setup\.exe|\.AppImage)$/.exec(name);
  if (!match || !names.includes(`${name}.sig`)) continue;
  const [, platform, arch] = match;
  platforms[`${platform}-${archKey[arch]}`] = {
    signature: readFileSync(join(dir, `${name}.sig`), "utf8").trim(),
    url: `https://github.com/${repository}/releases/download/${tag}/${name}`,
  };
}

if (Object.keys(platforms).length === 0) {
  console.log("No signed updater artifacts found; latest.json was not created.");
  process.exit(0);
}

const changelog = existsSync("CHANGELOG.md") ? readFileSync("CHANGELOG.md", "utf8") : "";
const section = new RegExp(`## \\[${version.replace(/\./g, "\\.")}\\][^\\n]*\\n([\\s\\S]*?)(?=\\n## \\[|$)`).exec(changelog);
const manifest = {
  version,
  notes: section ? section[1].trim() : `DarkLoad ${version}`,
  pub_date: new Date().toISOString(),
  platforms,
};
writeFileSync(join(dir, "latest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`latest.json written for: ${Object.keys(platforms).join(", ")}`);
