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
  ["100% parity ledger", ["scripts/verify-parity-100.mjs"]],
  ["protocol fixture", ["scripts/verify-protocol.mjs"]],
  ["persistence fixture", ["scripts/verify-persistence.mjs"]],
  ["native fixture", ["scripts/verify-native-fixture.mjs"]],
  ["live app-server", ["scripts/verify-live-app-server.mjs"]],
  ["safety scan", ["scripts/check-safety.mjs"]],
  ["release contract", ["scripts/verify-release-contract.mjs"]],
  ["implementation review", ["scripts/verify-review-records.mjs", "implementation"]],
  ["evidence review", ["scripts/verify-review-records.mjs", "evidence"]],
  ["remote release", ["scripts/verify-remote-release.mjs"]],
];
for (const [label, args] of checks) {
  const result = spawnSync(process.execPath, args, { cwd: repositoryRoot, stdio: "inherit" });
  if (result.status !== 0) {
    console.error(`PARITY_100_TESTS_FAIL step=${label}`);
    process.exit(result.status ?? 1);
  }
}
console.log("PARITY_100_TESTS_OK");
