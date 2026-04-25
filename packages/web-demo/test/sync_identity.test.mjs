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

const showcase = JSON.parse(
  await readFile(`${root}/public/components/plugin_showcase.json`, "utf8"),
);

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
  { id: "ruby-like", path: "ruby-like/ruby_like.js", supportsFromText: true },
  { id: "ts-like", path: "ts-like/ts_like.js", supportsFromText: true },
  { id: "rust-like", path: "rust-like/rust_like.js", supportsFromText: true },
];

let failures = 0;

for (const p of PLUGINS) {
  const m = await import(`${root}/public/plugins/${p.path}`);
  const plugin = m.syntaxPlugin;

  const before = summarize(showcase.wastComponent);
  const text = plugin.toText(showcase.wastComponent);

  let after;
  try {
    const parsed = plugin.fromText(text, showcase.wastComponent);
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
  console.log("\nall plugins identity-roundtrip");
}
