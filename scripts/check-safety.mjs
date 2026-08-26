#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const repositoryRoot = path.resolve(scriptDirectory, "..");
const listed = spawnSync("git", ["ls-files", "-co", "--exclude-standard"], {
  cwd: repositoryRoot,
  encoding: "utf8",
});
if (listed.status !== 0) {
  console.error("PARITY_G10_SAFETY_FAIL: could not enumerate repository files");
  process.exit(listed.status ?? 1);
}

const credentialPatterns = [
  /sk-[A-Za-z0-9]{10,}/,
  /gh[pousr]_[A-Za-z0-9]{20,}/,
  /github_pat_[A-Za-z0-9_]{20,}/,
  /Bearer\s+[A-Za-z0-9._-]{20,}/,
];
const destructivePatterns = [
  /(?:^|\s)rm\s+-rf(?:\s|$)/,
  /git\s+reset\s+--hard/,
  /git\s+checkout\s+--/,
  /curl[^\n|]*\|\s*(?:ba)?sh/,
  /(?:^|\s)sudo\s+/,
];
const failures = [];
const files = listed.stdout.split("\n").filter(Boolean);
for (const relativePath of files) {
  if (relativePath === ".git" || relativePath.startsWith(".git/")) continue;
  const absolutePath = path.join(repositoryRoot, relativePath);
  if (!fs.statSync(absolutePath).isFile()) continue;
  const contents = fs.readFileSync(absolutePath, "utf8");
  if (credentialPatterns.some((pattern) => pattern.test(contents))) failures.push(`credential-like value in ${relativePath}`);
  if (destructivePatterns.some((pattern) => pattern.test(contents))) failures.push(`destructive command in ${relativePath}`);
}

if (failures.length > 0) {
  console.error("PARITY_G10_SAFETY_FAIL");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(`PARITY_G10_SAFETY_OK files=${files.length}`);
