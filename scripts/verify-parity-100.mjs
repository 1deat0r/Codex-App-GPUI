#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const parityPath = path.join(repositoryRoot, "PARITY.md");
const parity = fs.readFileSync(parityPath, "utf8");
const expectedIds = [
  "shell-01", "shell-02", "shell-03", "shell-04",
  "thread-01", "thread-02", "thread-03", "thread-04",
  "composer-01", "composer-02", "composer-03",
  "exec-01", "exec-02", "exec-03", "exec-04",
  "data-01", "data-02", "collab-01", "collab-02",
  "nav-01", "nav-02", "runtime-01", "runtime-02", "runtime-03",
];
const rows = parity.split("\n")
  .map((line) => line.match(/^\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|$/))
  .filter(Boolean)
  .filter((match) => /^[a-z0-9]+-[a-z0-9-]+$/i.test(match[1]))
  .map((match) => ({
    id: match[1], avenue: match[2], requirement: match[3], owner: match[4], status: match[5], evidence: match[6],
  }));

const failures = [];
if (rows.length !== expectedIds.length) failures.push(`expected ${expectedIds.length} rows, found ${rows.length}`);
if (JSON.stringify(rows.map((row) => row.id)) !== JSON.stringify(expectedIds)) failures.push("ledger row order does not match the required inventory");
for (const [index, row] of rows.entries()) {
  if (row.status !== "verified") failures.push(`${row.id} is ${row.status}, not verified`);
  if (!row.evidence || /pending|unavailable|incomplete|remains|blocked/i.test(row.evidence)) failures.push(`${row.id} has non-final evidence`);
  const owners = [...row.owner.matchAll(/`([^`]+)`/g)].map((match) => match[1]);
  if (owners.length === 0) failures.push(`${row.id} has no owner`);
  for (const owner of owners) {
    if (owner.endsWith("/**")) continue;
    if (!fs.existsSync(path.join(repositoryRoot, owner))) failures.push(`${row.id} owner is missing: ${owner}`);
  }
  if (index >= expectedIds.length) failures.push(`unexpected row: ${row.id}`);
}

if (failures.length > 0) {
  console.error("PARITY_100_LEDGER_FAIL");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log(`PARITY_100_LEDGER_OK rows=${rows.length} verified=${rows.filter((row) => row.status === "verified").length}`);
