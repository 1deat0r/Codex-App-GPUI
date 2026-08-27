#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const checks = [
  ["format", ["scripts/run-cargo.mjs", "fmt", "--all", "--", "--check"]],
  ["build", ["scripts/run-cargo.mjs", "check", "--locked"]],
  ["rust tests", ["scripts/run-cargo.mjs", "test", "--locked"]],
  ["parity ledger", ["scripts/verify-parity.mjs"]],
  ["protocol fixture", ["scripts/verify-protocol.mjs"]],
  ["persistence fixture", ["scripts/verify-persistence.mjs"]],
  ["safety scan", ["scripts/check-safety.mjs"]],
];
for (const [label, args] of checks) {
  const result = spawnSync(process.execPath, args, { cwd: repositoryRoot, stdio: "inherit" });
  if (result.status !== 0) {
    console.error(`PARITY_100_TESTS_FAIL step=${label}`);
    process.exit(result.status ?? 1);
  }
}
console.log("PARITY_100_TESTS_OK");
