// Test harness: dynamically loads the bundled wasm components and reads
// the shared packages/sample-wast fixture from disk. Returns the exact
// same surface the VS Code extension consumes at runtime, so tests can
// exercise the real plugin/codec/partial-manager/compiler code paths
// without launching an Extension Development Host.
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
const sampleWastDir = join(pkgRoot, "..", "sample-wast");

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

export async function loadFixture() {
  const dir = sampleWastDir;
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

export async function loadHarness() {
  const runtime = await loadRuntime();
  const fixture = await loadFixture();
  // Pre-decode the on-disk fixture into the WastComponent shape the
  // plugin / partial-manager / compiler all consume.
  fixture.component = runtime.codec.read(fixture.wastJson, fixture.symsEnYaml);
  return { runtime, fixture };
}

export function describeError(err) {
  if (err == null) return String(err);

  // jco often wraps wasm traps so the payload is hidden behind one of
  // these property names. Probe each shape explicitly.
  const payload =
    err.payload ?? err.cause ?? err.value ?? err.error ?? err.inner;
  if (Array.isArray(payload)) {
    return payload
      .map((e) => `${e.message ?? e}${e?.location ? ` [${e.location}]` : ""}`)
      .join("; ");
  }
  if (payload && typeof payload === "object" && "message" in payload) {
    const loc = payload.location ? ` [${payload.location}]` : "";
    return `${payload.message}${loc}`;
  }

  if (Array.isArray(err)) {
    return err
      .map((e) => `${e.message ?? e}${e?.location ? ` [${e.location}]` : ""}`)
      .join("; ");
  }

  if (err instanceof Error) {
    return err.stack ?? err.message;
  }

  if (typeof err === "object") {
    // Some thrown values (especially component-model traps) carry their
    // info on non-enumerable properties, so JSON.stringify returns "{}".
    // Dump everything getOwnPropertyNames can see, including symbols.
    const own = {};
    for (const k of Object.getOwnPropertyNames(err)) {
      try {
        own[k] = err[k];
      } catch {
        own[k] = "<unreadable>";
      }
    }
    const json = JSON.stringify(own);
    if (json !== "{}") return json;
    return `${err.constructor?.name ?? "Object"} ${err.toString()}`;
  }

  return String(err);
}

export function funcUids(component) {
  return component.funcs.map(([u]) => u).sort();
}

export function typeUids(component) {
  return component.types.map(([u]) => u).sort();
}
