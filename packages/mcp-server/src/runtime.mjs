// Load the jco-transpiled components this server drives, and normalize the
// two-interface plugin exports into one object per plugin (same shape the VS
// Code extension's wasm-loader builds).

import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const componentsRoot = join(here, "..", "dist", "components");

/** Every bundled syntax plugin. Editability is discovered, not listed: a
 *  plugin is editable exactly when it exports `syntax-editor`. */
export const PLUGIN_IDS = ["ir-json", "raw", "ruby-like", "ts-like", "rust-like"];

/** The write surface. Explicit uids and structural bodies are what make a
 *  non-parser write path possible, so this is the one the write tools use. */
export const WRITE_SYNTAX = "ir-json";

function importComponent(id, file) {
  return import(pathToFileURL(join(componentsRoot, id, file)).href);
}

let cached;

export async function loadRuntime() {
  if (cached) return cached;

  const plugins = {};
  for (const id of PLUGIN_IDS) {
    const mod = await importComponent(id, `${id.replace(/-/g, "_")}.js`);
    plugins[id] = {
      toText: (component) => mod.syntaxRenderer.toText(component),
      ...(mod.syntaxEditor
        ? { fromText: (text, existing) => mod.syntaxEditor.fromText(text, existing) }
        : {}),
    };
  }

  const partialManager = (await importComponent("partial-manager", "partial_manager.js"))
    .partialManager;
  const codec = (await importComponent("codec", "codec.js")).codec;
  const compiler = (await importComponent("compiler", "compiler.js")).compiler;

  cached = { plugins, partialManager, codec, compiler };
  return cached;
}

/**
 * Flatten whatever a jco rejection carries into the coded `wast-error`
 * messages underneath.
 *
 * Those codes (`unknown_local:`, `signature_mismatch:`, `call_arity_mismatch:`
 * …) are the contract callers act on, so they have to survive the trip out.
 */
export function describeError(thrown) {
  const payload = thrown?.payload ?? thrown?.cause ?? thrown?.value;
  const list = Array.isArray(payload) ? payload : Array.isArray(thrown) ? thrown : null;
  if (list) {
    return list
      .map((e) => `${e.message ?? e}${e?.location ? ` [${e.location}]` : ""}`)
      .join("\n");
  }
  if (payload && typeof payload === "object" && "message" in payload) {
    return `${payload.message}${payload.location ? ` [${payload.location}]` : ""}`;
  }
  return thrown?.message ?? String(thrown);
}
