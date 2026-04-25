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

const showcase = JSON.parse(
  await readFile(`${root}/public/components/plugin_showcase.json`, "utf8"),
);

const PLUGINS = [
  { id: "raw", path: "raw/raw.js" },
  { id: "ruby-like", path: "ruby-like/ruby_like.js" },
  { id: "ts-like", path: "ts-like/ts_like.js" },
  { id: "rust-like", path: "rust-like/rust_like.js" },
];

let failures = 0;
for (const p of PLUGINS) {
  const m = await import(`${root}/public/plugins/${p.path}`);
  const plugin = m.syntaxPlugin;
  const t1 = plugin.toText(showcase.wastComponent);
  const parsed = plugin.fromText(t1, showcase.wastComponent);
  const t2 = plugin.toText(parsed);
  try {
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
