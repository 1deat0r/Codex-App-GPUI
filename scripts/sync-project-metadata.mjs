#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRootResult = spawnSync("git", ["rev-parse", "--show-toplevel"], {
  encoding: "utf8",
});
const repoRoot = repoRootResult.status === 0
  ? repoRootResult.stdout.trim()
  : path.resolve(scriptDir, "..");

const read = (relativePath) => fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
const write = (relativePath, contents) => {
  const target = path.join(repoRoot, relativePath);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, contents.endsWith("\n") ? contents : `${contents}\n`, "utf8");
};

const metadata = JSON.parse(read("project-metadata.json"));
const parity = read("PARITY.md");
const gates = read("GATES.md");

const rows = parity
  .split("\n")
  .map((line) => line.match(/^\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|$/))
  .filter(Boolean)
  .filter((match) => /^[a-z0-9]+-[a-z0-9-]+$/i.test(match[1]))
  .map((match) => ({
    id: match[1],
    avenue: match[2],
    requirement: match[3],
    owner: match[4],
    status: match[5],
    evidence: match[6],
  }));

const statusCounts = Object.fromEntries(["planned", "implemented", "verified", "blocked"]
  .map((status) => [status, rows.filter((row) => row.status === status).length]));
const totalRows = rows.length;
const verifiedPercent = totalRows === 0 ? 0 : Math.round((statusCounts.verified / totalRows) * 100);
const gateRows = [...gates.matchAll(/^- \[([ x])\] (G\d+): (.+)$/gm)]
  .map((match) => ({ checked: match[1] === "x", id: match[2], title: match[3] }));
const checkedGates = gateRows.filter((gate) => gate.checked).length;

const paritySummary = `**Parity coverage:** ${statusCounts.verified}/${totalRows} avenues verified (${verifiedPercent}%). ${statusCounts.implemented} implemented, ${statusCounts.planned} planned, ${statusCounts.blocked} blocked.`;
const gateSummary = `**Acceptance gates:** ${checkedGates}/${gateRows.length} checked. See [GATES.md](GATES.md) for runnable and manual evidence.`;

const gridRows = rows.slice(0, 12).map((row) => `| ${row.id} | ${row.avenue} | ${row.status} | ${row.owner} |`).join("\n");
const featureGrid = [
  "| Axis | Reference avenue | Status | Owner |",
  "| --- | --- | --- | --- |",
  gridRows,
  "",
  `[View the complete ${totalRows}-avenue parity ledger](PARITY.md).`,
].join("\n");

const readmeTemplate = read("README.template.md");
const readme = readmeTemplate
  .replaceAll("{{PARITY_SUMMARY}}", paritySummary)
  .replaceAll("{{GATE_SUMMARY}}", gateSummary)
  .replaceAll("{{FEATURE_GRID}}", featureGrid)
  .replaceAll("{{LICENSE}}", metadata.license);
write("README.md", readme);

const statusDoc = [
  `# Parity status: ${metadata.name}`,
  "",
  `Reference: ${metadata.reference}`,
  "",
  paritySummary,
  "",
  gateSummary,
  "",
  "## Status counts",
  "",
  "| Status | Count |",
  "| --- | ---: |",
  ...Object.entries(statusCounts).map(([status, count]) => `| ${status} | ${count} |`),
  "",
  "## Ledger snapshot",
  "",
  "| ID | Avenue | Owner | Status | Evidence |",
  "| --- | --- | --- | --- | --- |",
  ...rows.map((row) => `| ${row.id} | ${row.avenue} | ${row.owner} | ${row.status} | ${row.evidence} |`),
  "",
  "This file is generated from [PARITY.md](../PARITY.md) by the pre-commit hook.",
].join("\n");
write("docs/PARITY-STATUS.md", statusDoc);

const stage = process.argv.includes("--stage");
if (stage) {
  const addResult = spawnSync("git", ["add", "README.md", "docs/PARITY-STATUS.md"], {
    cwd: repoRoot,
    stdio: "inherit",
  });
  if (addResult.status !== 0) process.exit(addResult.status ?? 1);
}

const remoteResult = spawnSync("git", ["remote", "get-url", "origin"], {
  cwd: repoRoot,
  encoding: "utf8",
});
if (remoteResult.status === 0) {
  const remote = remoteResult.stdout.trim();
  const match = remote.match(/github\.com[/:]([^/]+)\/([^/]+?)(?:\.git)?$/i);
  if (!match) {
    console.error(`Metadata sync cannot identify a GitHub owner from origin: ${remote}`);
    process.exit(1);
  }
  const repo = `${match[1]}/${match[2]}`;
  const editResult = spawnSync("gh", ["repo", "edit", repo, "--description", metadata.description], {
    cwd: repoRoot,
    stdio: "inherit",
  });
  if (editResult.status !== 0) {
    console.error("Metadata sync could not update the GitHub description; commit blocked.");
    process.exit(editResult.status ?? 1);
  }
  console.log(`GitHub metadata synchronized for ${repo}.`);
} else {
  console.log("No origin remote yet; local documentation synchronized.");
}

console.log("SYNC_METADATA_OK");
