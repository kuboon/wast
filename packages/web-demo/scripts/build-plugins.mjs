#!/usr/bin/env node
// Build the 4 syntax plugin WASM components, then transpile each with jco
// so the browser can load them alongside the v0.x function demos.

import { mkdir, rm, copyFile, cp } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..", "..");
const outRoot = join(here, "..", "public", "plugins");

function haveMise() {
  return spawnSync("mise", ["--version"], { stdio: "ignore" }).status === 0;
}

const plugins = [
  { crate: "wast-syntax-raw", artifact: "wast_syntax_raw.wasm", id: "raw" },
  { crate: "wast-syntax-ruby-like", artifact: "wast_syntax_ruby_like.wasm", id: "ruby-like" },
  { crate: "wast-syntax-ts-like", artifact: "wast_syntax_ts_like.wasm", id: "ts-like" },
  { crate: "wast-syntax-rust-like", artifact: "wast_syntax_rust_like.wasm", id: "rust-like" },
];

await rm(outRoot, { recursive: true, force: true });
await mkdir(outRoot, { recursive: true });

for (const p of plugins) {
  console.log(`\n== ${p.id} ==`);
  // Prefer `mise x --` so CI container finds cargo-component via its
  // installed tool path. Falls back to direct cargo if mise isn't around.
  const cmd = process.env.MISE_BIN || (haveMise() ? "mise" : null);
  const [prog, prefix] = cmd
    ? [cmd, ["x", "--", "cargo", "component"]]
    : ["cargo", ["component"]];
  const build = spawnSync(
    prog,
    [...prefix, "build", "-p", p.crate, "--release"],
    { cwd: root, stdio: "inherit" },
  );
  if (build.status !== 0) {
    console.error(`cargo component build failed for ${p.crate}`);
    process.exit(build.status ?? 1);
  }

  const wasm = join(root, "target", "wasm32-wasip1", "release", p.artifact);
  const dest = join(outRoot, p.id);
  await mkdir(dest, { recursive: true });

  const t = spawnSync(
    "npx",
    [
      "jco",
      "transpile",
      wasm,
      "-o",
      dest,
      "--name",
      p.id.replace(/-/g, "_"),
      "--no-typescript",
    ],
    { stdio: "inherit" },
  );
  if (t.status !== 0) {
    console.error(`jco transpile failed for ${p.id}`);
    process.exit(t.status ?? 1);
  }
}

console.log(`\nBuilt ${plugins.length} syntax plugins into ${outRoot}`);

// preview2-shim: jco-transpiled plugins use bare specifiers like
// `@bytecodealliance/preview2-shim/cli`. Copy the browser-flavor ES modules
// into public/vendor/preview2-shim/ so an import map can resolve them.
const shimSrc = join(
  root,
  "node_modules",
  "@bytecodealliance",
  "preview2-shim",
  "lib",
  "browser",
);
const shimDest = join(here, "..", "public", "vendor", "preview2-shim");
await rm(shimDest, { recursive: true, force: true });
await mkdir(shimDest, { recursive: true });
await cp(shimSrc, shimDest, { recursive: true });
console.log(`Copied preview2-shim browser build → ${shimDest}`);
