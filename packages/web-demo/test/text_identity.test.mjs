//! Text-level identity: to_text → from_text → to_text should yield the
//! same text. Body byte representation may normalize (e.g. ts-like drops
//! Call arg parameter names since its surface syntax doesn't render
//! them), but the rendered text round-trips.

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

const PLUGINS = [
  { id: "ir-json", path: "ir-json/ir_json.js", editable: true },
  { id: "raw", path: "raw/raw.js", editable: true },
  { id: "ruby-like", path: "ruby-like/ruby_like.js", editable: false },
  { id: "ts-like", path: "ts-like/ts_like.js", editable: true },
  { id: "rust-like", path: "rust-like/rust_like.js", editable: false },
];

let failures = 0;
for (const p of PLUGINS) {
  const m = await import(`${root}/public/plugins/${p.path}`);
  try {
    if (!p.editable) {
      // Renderer-only: two renders of the same component must agree.
      const t1 = m.syntaxRenderer.toText(wastComponent);
      const t2 = m.syntaxRenderer.toText(wastComponent);
      assert.equal(t1, t2, `${p.id}: render is not deterministic`);
      console.log(`✓ ${p.id}: deterministic render (read-only)`);
      continue;
    }
    const t1 = m.syntaxRenderer.toText(wastComponent);
    const parsed = m.syntaxEditor.fromText(t1, wastComponent);
    const t2 = m.syntaxRenderer.toText(parsed);
    assert.equal(t1, t2, `${p.id}: text differs after no-op sync`);
    console.log(`✓ ${p.id}: text identity round-trip`);
  } catch (err) {
    console.error(`✗ ${p.id}: ${err.message}`);
    failures++;
  }
}

if (failures > 0) {
  process.exit(1);
}
