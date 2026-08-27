#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const run = (command, args) => spawnSync(command, args, { cwd: repositoryRoot, encoding: "utf8" });
const local = run("git", ["rev-parse", "HEAD"]).stdout.trim();
const remote = run("git", ["ls-remote", "origin", "refs/heads/main"]).stdout.trim().split(/\s+/)[0];
const status = run("git", ["status", "--porcelain"]).stdout.trim();
const view = run("gh", ["repo", "view", "1deat0r/Codex-App-GPUI", "--json", "visibility,description,defaultBranchRef"]).stdout;
const failures = [];
if (!local || local !== remote) failures.push(`local=${local || "missing"} remote=${remote || "missing"}`);
if (status) failures.push("working tree is dirty");
if (!view.includes('"visibility":"PUBLIC"')) failures.push("repository is not public");
if (!view.includes("Native GPU-accelerated Codex desktop client built with Rust and GPUI.")) failures.push("GitHub description is stale");
if (failures.length > 0) {
  console.error("PARITY_100_REMOTE_FAIL");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log(`PARITY_100_REMOTE_OK commit=${local}`);
