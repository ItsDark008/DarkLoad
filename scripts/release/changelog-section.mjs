#!/usr/bin/env node
// Prints the CHANGELOG.md section for a version, used as the GitHub Release notes.
import { readFileSync } from "node:fs";

const version = (process.argv[2] ?? "").replace(/^v/, "");
const changelog = readFileSync("CHANGELOG.md", "utf8");
const escaped = version.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const match = new RegExp(`## \\[${escaped}\\][^\\n]*\\n([\\s\\S]*?)(?=\\n## \\[|$)`).exec(changelog);
const body = match ? match[1].trim() : `See CHANGELOG.md for details about DarkLoad ${version}.`;

process.stdout.write(`${body}

### Downloads

| Platform | Files |
| --- | --- |
| Windows 10/11 (x64, ARM64) | \`-setup.exe\` (recommended), \`.msi\`, \`_portable.zip\` |
| Linux (x64, ARM64) | \`.AppImage\`, \`.deb\`, \`.rpm\`, \`_portable.tar.gz\` |

Verify downloads against \`SHA256SUMS.txt\`. Installers are not code-signed with a platform certificate unless stated in these notes, so Windows SmartScreen may show a warning.
`);
