#!/usr/bin/env node
// Build the 5 syntax plugin WASM components plus the partial-manager and
// codec components, transpile each with jco so the browser can load them
// alongside the v0.x function demos.

import { mkdir, rm, cp } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { bundleComponent } from "../../../scripts/lib/components.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..", "..");
const pluginsRoot = join(here, "..", "public", "plugins");
const toolsRoot = join(here, "..", "public", "tools");

const targets = [
  { crate: "wast-syntax-ir-json", artifact: "wast_syntax_ir_json.wasm", id: "ir-json", outDir: pluginsRoot },
  { crate: "wast-syntax-raw", artifact: "wast_syntax_raw.wasm", id: "raw", outDir: pluginsRoot },
  { crate: "wast-syntax-ruby-like", artifact: "wast_syntax_ruby_like.wasm", id: "ruby-like", outDir: pluginsRoot },
  { crate: "wast-syntax-ts-like", artifact: "wast_syntax_ts_like.wasm", id: "ts-like", outDir: pluginsRoot },
  { crate: "wast-syntax-rust-like", artifact: "wast_syntax_rust_like.wasm", id: "rust-like", outDir: pluginsRoot },
  { crate: "wast-partial-manager", artifact: "wast_partial_manager.wasm", id: "partial-manager", outDir: toolsRoot },
  { crate: "wast-codec", artifact: "wast_codec.wasm", id: "codec", outDir: toolsRoot },
];

await rm(pluginsRoot, { recursive: true, force: true });
await mkdir(pluginsRoot, { recursive: true });
await rm(toolsRoot, { recursive: true, force: true });
await mkdir(toolsRoot, { recursive: true });

for (const target of targets) {
  await bundleComponent({ root, ...target });
}

console.log(`\nBuilt ${targets.length} components (plugins → ${pluginsRoot}, tools → ${toolsRoot})`);

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
