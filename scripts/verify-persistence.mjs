#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const repositoryRoot = path.resolve(scriptDirectory, "..");
const result = spawnSync(process.execPath, ["scripts/run-cargo.mjs", "test", "--locked", "snapshot_round_trip_uses_atomic_file", "--", "--nocapture"], {
  cwd: repositoryRoot,
  stdio: "inherit",
});
if (result.status !== 0) process.exit(result.status ?? 1);
console.log("PARITY_G5_PERSISTENCE_OK");
