#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const kind = process.argv[2];
if (!["implementation", "evidence"].includes(kind)) {
  console.error("usage: verify-review-records.mjs implementation|evidence");
  process.exit(2);
}
const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const reviewPath = path.join(repositoryRoot, "reviews", `${kind}.md`);
const text = fs.existsSync(reviewPath) ? fs.readFileSync(reviewPath, "utf8") : "";
const required = ["STATUS: APPROVED", "REVIEWER:", "COMMIT:", "FINDINGS: NONE", "REMEDIATION:"];
const missing = required.filter((marker) => !text.includes(marker));
if (missing.length > 0) {
  console.error(`PARITY_100_${kind.toUpperCase()}_REVIEW_FAIL missing=${missing.join(",")}`);
  process.exit(1);
}
console.log(`PARITY_100_${kind.toUpperCase()}_REVIEW_OK`);
