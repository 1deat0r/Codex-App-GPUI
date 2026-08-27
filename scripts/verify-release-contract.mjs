#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const run = (command, args, options = {}) => spawnSync(command, args, { cwd: repositoryRoot, encoding: "utf8", ...options });
const metadata = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "project-metadata.json"), "utf8"));
const hook = path.join(repositoryRoot, ".githooks", "pre-commit");
const failures = [];
if (!fs.existsSync(hook) || (fs.statSync(hook).mode & 0o111) === 0) failures.push("pre-commit hook is missing or not executable");
if (run("git", ["config", "--get", "core.hooksPath"]).stdout.trim() !== ".githooks") failures.push("core.hooksPath is not .githooks");
const readme = fs.readFileSync(path.join(repositoryRoot, "README.md"), "utf8");
const status = fs.readFileSync(path.join(repositoryRoot, "docs/PARITY-STATUS.md"), "utf8");
if (!readme.includes("github.com/1deat0r/Codex-App-GPUI")) failures.push("README lacks public repository link");
if (!readme.includes("Parity coverage:")) failures.push("README lacks generated parity summary");
if (!status.includes("Parity coverage:")) failures.push("status document lacks generated parity summary");
if (!metadata.description || metadata.description.length < 12) failures.push("project description is missing");
const sync = run(process.execPath, ["scripts/sync-project-metadata.mjs"]);
if (sync.status !== 0 || !`${sync.stdout}${sync.stderr}`.includes("SYNC_METADATA_OK")) failures.push("metadata synchronization failed");
const safety = run(process.execPath, ["scripts/check-safety.mjs"]);
if (safety.status !== 0 || !`${safety.stdout}${safety.stderr}`.includes("PARITY_G10_SAFETY_OK")) failures.push("safety scan failed");
if (failures.length > 0) {
  console.error("PARITY_100_RELEASE_FAIL");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("PARITY_100_RELEASE_OK");
