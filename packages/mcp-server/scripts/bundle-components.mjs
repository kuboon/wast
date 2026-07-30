#!/usr/bin/env node
// Build the wasm components the MCP server drives and jco-transpile each
// into dist/components/<id>/. Same set as the VS Code extension: the write
// path needs ir-json + partial-manager + codec + compiler, and the
// read-only renderers back the `wast_render` tool.

import { mkdir, rm } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { bundleComponent } from "../../../scripts/lib/components.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..", "..");
const out = join(here, "..", "dist", "components");

const targets = [
  { crate: "wast-syntax-ir-json", artifact: "wast_syntax_ir_json.wasm", id: "ir-json" },
  { crate: "wast-syntax-raw", artifact: "wast_syntax_raw.wasm", id: "raw" },
  { crate: "wast-syntax-ruby-like", artifact: "wast_syntax_ruby_like.wasm", id: "ruby-like" },
  { crate: "wast-syntax-ts-like", artifact: "wast_syntax_ts_like.wasm", id: "ts-like" },
  { crate: "wast-syntax-rust-like", artifact: "wast_syntax_rust_like.wasm", id: "rust-like" },
  { crate: "wast-partial-manager", artifact: "wast_partial_manager.wasm", id: "partial-manager" },
  { crate: "wast-codec", artifact: "wast_codec.wasm", id: "codec" },
  { crate: "wast-compiler-component", artifact: "wast_compiler_component.wasm", id: "compiler" },
];

await rm(out, { recursive: true, force: true });
await mkdir(out, { recursive: true });

for (const target of targets) {
  await bundleComponent({ root, outDir: out, nodeHost: true, ...target });
}

console.log(`\nBuilt ${targets.length} components into ${out}`);
