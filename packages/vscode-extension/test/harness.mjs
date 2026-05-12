// Test harness: dynamically loads the bundled wasm components and reads
// the examples/basic fixture from disk. Returns the exact same surface
// the VS Code extension consumes at runtime, so tests can exercise the
// real plugin/codec/partial-manager/compiler code paths without
// launching an Extension Development Host.
//
// Usage:
//
//   import { loadHarness } from "./harness.mjs";
//   const { runtime, fixture } = await loadHarness();
//   const text = runtime.plugins["ruby-like"].toText(fixture.component);

import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const pkgRoot = join(here, "..");
const componentsRoot = join(pkgRoot, "dist", "components");

const PLUGIN_IDS = ["raw", "ruby-like", "ts-like", "rust-like"];

async function loadPlugins() {
  const out = {};
  for (const id of PLUGIN_IDS) {
    const mod = await import(join(componentsRoot, id, id.replace(/-/g, "_") + ".js"));
    out[id] = mod.syntaxPlugin;
  }
  return out;
}

export async function loadRuntime() {
  const plugins = await loadPlugins();
  const pm = (await import(join(componentsRoot, "partial-manager", "partial_manager.js"))).partialManager;
  const codec = (await import(join(componentsRoot, "codec", "codec.js"))).codec;
  const compiler = (await import(join(componentsRoot, "compiler", "compiler.js"))).compiler;
  return { plugins, partialManager: pm, codec, compiler };
}

export async function loadFixture(name = "basic") {
  const dir = join(pkgRoot, "examples", name);
  const worldWit = new Uint8Array(await readFile(join(dir, "world.wit")));
  const wastJson = new Uint8Array(await readFile(join(dir, "wast.json")));
  let symsEnYaml = null;
  try {
    symsEnYaml = new Uint8Array(await readFile(join(dir, "syms.en.yaml")));
  } catch {
    // syms.en.yaml is optional
  }
  return { dir, worldWit, wastJson, symsEnYaml };
}

export async function loadHarness(fixtureName = "basic") {
  const runtime = await loadRuntime();
  const fixture = await loadFixture(fixtureName);
  // Pre-decode the on-disk fixture into the WastComponent shape the
  // plugin / partial-manager / compiler all consume.
  fixture.component = runtime.codec.read(fixture.wastJson, fixture.symsEnYaml);
  return { runtime, fixture };
}

export function describeError(err) {
  if (Array.isArray(err?.payload)) {
    return err.payload
      .map((e) => `${e.message}${e.location ? ` [${e.location}]` : ""}`)
      .join("; ");
  }
  if (Array.isArray(err)) {
    return err
      .map((e) => `${e.message ?? e}${e.location ? ` [${e.location}]` : ""}`)
      .join("; ");
  }
  if (err && typeof err === "object") {
    try {
      return JSON.stringify(err);
    } catch {
      return String(err);
    }
  }
  return err?.message ?? String(err);
}

export function funcUids(component) {
  return component.funcs.map(([u]) => u).sort();
}

export function typeUids(component) {
  return component.types.map(([u]) => u).sort();
}
