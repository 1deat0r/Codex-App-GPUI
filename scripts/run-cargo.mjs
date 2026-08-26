#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const repositoryRoot = path.resolve(scriptDirectory, "..");
const userHome = os.homedir();
const stableToolchain = path.join(userHome, ".rustup", "toolchains", "stable-x86_64-unknown-linux-gnu", "bin");
const candidates = [
  process.env.CARGO_BIN,
  path.join(stableToolchain, "cargo"),
  path.join(userHome, ".cargo", "bin", "cargo"),
  "cargo",
].filter(Boolean);

function environmentFor(candidate) {
  const environment = { ...process.env };
  environment.RUST_MIN_STACK ??= "16777216";
  const candidateDirectory = path.dirname(candidate);
  const pairedRustc = path.join(candidateDirectory, "rustc");
  const pairedRustdoc = path.join(candidateDirectory, "rustdoc");
  if (fs.existsSync(pairedRustc)) environment.RUSTC = pairedRustc;
  if (fs.existsSync(pairedRustdoc)) environment.RUSTDOC = pairedRustdoc;
  if (!environment.RUSTC && fs.existsSync(path.join(stableToolchain, "rustc"))) {
    environment.RUSTC = path.join(stableToolchain, "rustc");
  }
  if (!environment.RUSTDOC && fs.existsSync(path.join(stableToolchain, "rustdoc"))) {
    environment.RUSTDOC = path.join(stableToolchain, "rustdoc");
  }
  return environment;
}

function findCargo() {
  for (const candidate of candidates) {
    const probe = spawnSync(candidate, ["--version"], {
      cwd: repositoryRoot,
      env: environmentFor(candidate),
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
    if (probe.status === 0) return { candidate, environment: environmentFor(candidate) };
  }
  console.error("No working Cargo executable was found. Set CARGO_BIN to an explicit toolchain path.");
  process.exit(127);
}

const args = process.argv.slice(2);
if (args.length === 0) {
  console.error("Usage: node scripts/run-cargo.mjs <cargo arguments>");
  process.exit(2);
}

const { candidate, environment } = findCargo();
const result = spawnSync(candidate, args, {
  cwd: repositoryRoot,
  env: environment,
  stdio: "inherit",
});

if (result.error) {
  console.error(`Cargo could not start: ${result.error.message}`);
  process.exit(1);
}

if (result.status !== 0) process.exit(result.status ?? 1);

const command = args[0];
if (command === "check") console.log("PARITY_G1_BUILD_OK");
if (command === "test") console.log("PARITY_G2_TESTS_OK");
