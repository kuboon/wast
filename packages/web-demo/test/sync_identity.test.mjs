//! No-op `Sync from this pane` test: to_text → from_text without any
//! intermediate edits should be a structural identity, not just text-equal.
//! Regression: when syms.internal is empty (the showcase's initial state),
//! every plugin's `from_text` was generating fresh UIDs for funcs whose
//! rendered name matched their existing source-name, severing the link
//! between body Calls and their target funcs.

import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { strict as assert } from "node:assert";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");
const sampleDir = resolve(root, "..", "sample-wast");

const codec = (await import(`${root}/public/tools/codec/codec.js`)).codec;
const wastJsonBytes = new Uint8Array(await readFile(`${sampleDir}/wast.json`));
let symsBytes = null;
try {
  symsBytes = new Uint8Array(await readFile(`${sampleDir}/syms.en.yaml`));
} catch {}
const wastComponent = codec.read(wastJsonBytes, symsBytes);

/** Stable summary of a wast-component for structural comparison.
 *  Sorted by uid so the order doesn't matter, and serialised so we get a
 *  helpful diff on mismatch.
 */
function summarize(wc) {
  const funcs = [...wc.funcs]
    .map(([uid, row]) => ({
      uid,
      sourceTag: row.source.tag,
      sourceVal: row.source.val,
      params: row.params.map(([n, t]) => `${n}:${t}`),
      result: row.result,
      bodyLen: row.body?.length ?? 0,
    }))
    .sort((a, b) => a.uid.localeCompare(b.uid));
  const types = [...wc.types]
    .map(([uid]) => uid)
    .sort();
  return { funcs, types };
}

const PLUGINS = [
  { id: "raw", path: "raw/raw.js", editable: true },
  { id: "ruby-like", path: "ruby-like/ruby_like.js", editable: false },
  { id: "ts-like", path: "ts-like/ts_like.js", editable: true },
  { id: "rust-like", path: "rust-like/rust_like.js", editable: false },
];

let failures = 0;

for (const p of PLUGINS) {
  const m = await import(`${root}/public/plugins/${p.path}`);

  if (!p.editable) {
    // Renderer-only plugin: must render, must NOT export syntax-editor.
    try {
      assert.ok(m.syntaxRenderer, `${p.id}: missing syntax-renderer export`);
      assert.equal(
        m.syntaxEditor,
        undefined,
        `${p.id}: renderer-only plugin unexpectedly exports syntax-editor`,
      );
      const text = m.syntaxRenderer.toText(wastComponent);
      assert.ok(text.length > 0, `${p.id}: empty render`);
      console.log(`✓ ${p.id}: renderer-only (renders, no editor export)`);
    } catch (err) {
      console.error(`✗ ${p.id}: ${err.message}`);
      failures++;
    }
    continue;
  }

  const plugin = {
    toText: (c) => m.syntaxRenderer.toText(c),
    fromText: (t, e) => m.syntaxEditor.fromText(t, e),
  };

  const before = summarize(wastComponent);
  const text = plugin.toText(wastComponent);

  let after;
  try {
    const parsed = plugin.fromText(text, wastComponent);
    after = summarize(parsed);
  } catch (err) {
    console.error(`✗ ${p.id}: from_text threw`, err);
    failures++;
    continue;
  }

  try {
    assert.deepEqual(after.funcs, before.funcs, `${p.id}: funcs structure changed`);
    assert.deepEqual(after.types, before.types, `${p.id}: types changed`);
    console.log(`✓ ${p.id}: no-op sync preserves structure`);
  } catch (err) {
    console.error(`✗ ${p.id}: ${err.message}`);
    console.error("  before funcs:", JSON.stringify(before.funcs.map(f => f.uid)));
    console.error("  after  funcs:", JSON.stringify(after.funcs.map(f => f.uid)));
    failures++;
  }
}

if (failures > 0) {
  console.error(`\n${failures} plugin(s) failed`);
  process.exit(1);
} else {
  console.log("\nall editable plugins identity-roundtrip; renderers render");
}
