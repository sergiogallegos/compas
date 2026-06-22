#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(new URL("..", import.meta.url)));

function read(path) {
  return readFileSync(join(root, path), "utf8");
}

function cargoWorkspaceVersion() {
  const cargo = read("Cargo.toml");
  const workspacePackage = cargo.match(/\[workspace\.package\]([\s\S]*?)(?:\n\[|$)/);
  if (!workspacePackage) {
    throw new Error("Cargo.toml is missing [workspace.package]");
  }
  const version = workspacePackage[1].match(/^\s*version\s*=\s*"([^"]+)"/m);
  if (!version) {
    throw new Error("Cargo.toml [workspace.package] is missing version");
  }
  return version[1];
}

const versions = {
  "Cargo.toml [workspace.package]": cargoWorkspaceVersion(),
  "src-tauri/tauri.conf.json": JSON.parse(read("src-tauri/tauri.conf.json")).version,
  "frontend/package.json": JSON.parse(read("frontend/package.json")).version,
};

const unique = new Set(Object.values(versions));
if (unique.size === 1) {
  console.log(`Versions match: ${[...unique][0]}`);
  process.exit(0);
}

console.error("Version mismatch:");
for (const [file, version] of Object.entries(versions)) {
  console.error(`  ${file}: ${version}`);
}
process.exit(1);
