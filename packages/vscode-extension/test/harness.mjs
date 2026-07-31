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
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const pkgRoot = join(here, "..");
const componentsRoot = join(pkgRoot, "dist", "components");
const sampleWastDir = join(pkgRoot, "..", "sample-wast");

const PLUGIN_IDS = ["ir-json", "raw", "ruby-like", "ts-like", "rust-like"];

// import() takes URLs, not raw paths — a join()ed Windows path would be
// mis-parsed (drive letter / backslashes), so go through pathToFileURL.
function importPath(...segments) {
  return import(pathToFileURL(join(...segments)).href);
}

async function loadPlugins() {
  const out = {};
  for (const id of PLUGIN_IDS) {
    const mod = await importPath(componentsRoot, id, id.replace(/-/g, "_") + ".js");
    // Every plugin exports syntax-renderer (toText); write-capable plugins
    // also export syntax-editor (fromText). Renderer-only plugins get no
    // fromText — same shape the extension's wasm-loader builds.
    out[id] = {
      toText: (component) => mod.syntaxRenderer.toText(component),
      ...(mod.syntaxEditor
        ? { fromText: (text, existing) => mod.syntaxEditor.fromText(text, existing) }
        : {}),
    };
  }
  return out;
}

export async function loadRuntime() {
  const plugins = await loadPlugins();
  const pm = (await importPath(componentsRoot, "partial-manager", "partial_manager.js")).partialManager;
  const codec = (await importPath(componentsRoot, "codec", "codec.js")).codec;
  const compiler = (await importPath(componentsRoot, "compiler", "compiler.js")).compiler;
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
