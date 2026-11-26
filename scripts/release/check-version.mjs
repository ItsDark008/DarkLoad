#!/usr/bin/env node
// Verifies that a release tag (vX.Y.Z) matches package.json and src-tauri/Cargo.toml.
import { readFileSync } from "node:fs";

const tag = process.argv[2] ?? "";
const match = /^v(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)$/.exec(tag);
if (!match) {
  console.error(`Release tags must look like v1.2.3 (got "${tag}").`);
  process.exit(1);
}
const version = match[1];
const pkg = JSON.parse(readFileSync("package.json", "utf8")).version;
const cargo = /^version\s*=\s*"([^"]+)"/m.exec(readFileSync("src-tauri/Cargo.toml", "utf8"))?.[1];

const problems = [];
if (pkg !== version) problems.push(`package.json is ${pkg}`);
if (cargo !== version) problems.push(`src-tauri/Cargo.toml is ${cargo}`);
if (problems.length) {
  console.error(`Tag ${tag} does not match the project version: ${problems.join(", ")}.`);
  process.exit(1);
}
console.log(`Version ${version} confirmed.`);
