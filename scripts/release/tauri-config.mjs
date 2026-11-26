#!/usr/bin/env node
// Writes a Tauri configuration overlay for release builds.
//
// Signed application updates are enabled only when both the private signing key (secret
// TAURI_SIGNING_PRIVATE_KEY) and the matching public key (variable TAURI_UPDATER_PUBKEY) are
// configured. Without them the build still succeeds, but the app reports that automatic updates
// are not configured instead of pretending to be signed.
import { writeFileSync } from "node:fs";

const out = process.argv[2] ?? "release-config.json";
const privateKey = process.env.TAURI_SIGNING_PRIVATE_KEY?.trim();
const pubkey = process.env.TAURI_UPDATER_PUBKEY?.trim();
const repository = process.env.GITHUB_REPOSITORY?.trim();

const config = {};
if (privateKey && pubkey && repository) {
  config.bundle = { createUpdaterArtifacts: true };
  config.plugins = {
    updater: {
      pubkey,
      endpoints: [`https://github.com/${repository}/releases/latest/download/latest.json`],
      windows: { installMode: "passive" },
    },
  };
  console.log("Signed updater artifacts enabled.");
} else {
  console.log("Updater signing is not configured; building without update artifacts.");
}
writeFileSync(out, `${JSON.stringify(config, null, 2)}\n`);
