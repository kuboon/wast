// End-to-end test of every flow the VS Code extension drives against the
// bundled wasm components: read → render with each plugin → edit → parse
// → merge → write → re-read. Operates on packages/sample-wast/ as fixture
// (the shared on-disk source also used by the web-demo) so changes to
// that workspace are caught.
//
//   pnpm --filter @wast/vscode-extension test
//
// Add a new scenario by appending another `test(...)` block — the
// harness handles plumbing.

import { test } from "node:test";
import assert from "node:assert/strict";
import { loadHarness, describeError, funcUids, typeUids } from "./harness.mjs";

const { runtime, fixture } = await loadHarness();

test("codec.read returns the funcs and types we expect", () => {
  // sample-wast ships 12 funcs (5 internal helpers + 7 exports).
  const uids = funcUids(fixture.component);
  for (const expected of ["square", "sum_of_squares", "max3", "greeting"]) {
    assert.ok(uids.includes(expected), `missing ${expected}`);
  }
  assert.ok(typeUids(fixture.component).includes("point"));
});

test("every syntax plugin renders the fixture without crashing", () => {
  for (const [id, plugin] of Object.entries(runtime.plugins)) {
    const text = plugin.toText(fixture.component);
    assert.ok(text.length > 0, `${id} produced empty output`);
    assert.match(text, /square/, `${id} missing 'square'`);
    assert.match(text, /greeting/, `${id} missing 'greeting'`);
  }
});

test("ruby-like round-trip: toText → fromText → merge preserves all funcs/types", () => {
  const plugin = runtime.plugins["ruby-like"];
  const text = plugin.toText(fixture.component);

  let parsed;
  try {
    parsed = plugin.fromText(text, fixture.component);
  } catch (err) {
    assert.fail(`fromText threw: ${describeError(err)}`);
  }

  let merged;
  try {
    merged = runtime.partialManager.merge(parsed, fixture.component);
  } catch (err) {
    assert.fail(`merge threw: ${describeError(err)}`);
  }

  assert.deepEqual(funcUids(merged), funcUids(fixture.component));
  assert.deepEqual(typeUids(merged), typeUids(fixture.component));
});

test("full save flow round-trip: merged → codec.write → codec.read is identity", () => {
  const plugin = runtime.plugins["ruby-like"];
  const text = plugin.toText(fixture.component);
  const parsed = plugin.fromText(text, fixture.component);
  const merged = runtime.partialManager.merge(parsed, fixture.component);

  const written = runtime.codec.write(fixture.worldWit, merged);
  assert.ok(written.wastJson.byteLength > 0, "codec.write produced empty wast.json");

  const reread = runtime.codec.read(written.wastJson, written.symsEnYaml);
  assert.deepEqual(funcUids(reread), funcUids(fixture.component));
  assert.deepEqual(typeUids(reread), typeUids(fixture.component));
});

test("partial-manager.extract narrows the view to one func", () => {
  const partial = runtime.partialManager.extract(fixture.component, [
    { sym: "square", includeCaller: false },
  ]);
  assert.deepEqual(funcUids(partial), ["square"]);

  // Re-merging the partial into the full component restores everything.
  const merged = runtime.partialManager.merge(partial, fixture.component);
  assert.deepEqual(funcUids(merged), funcUids(fixture.component));
});

test("compiler.compile produces a wasm component from the committed sample", () => {
  // sample-wast ships real bodies, so we can hand the fixture straight to
  // the compiler — no body injection needed.
  let wasm;
  try {
    wasm = runtime.compiler.compile(fixture.component, fixture.worldWit);
  } catch (err) {
    assert.fail(`compile threw: ${describeError(err)}`);
  }
  assert.ok(wasm.byteLength > 0, "compiler produced empty output");
  // Wasm magic: 0x00 0x61 0x73 0x6D.
  assert.deepEqual(
    [wasm[0], wasm[1], wasm[2], wasm[3]],
    [0x00, 0x61, 0x73, 0x6d],
    "output is not a wasm module",
  );
});

test("signature change is rejected at merge stage (not silently accepted)", () => {
  const plugin = runtime.plugins["ruby-like"];
  // Narrow to a single internal helper so the rendered text has a known
  // signature substring we can corrupt.
  const partial = runtime.partialManager.extract(fixture.component, [
    { sym: "square", includeCaller: false },
  ]);
  const text = plugin.toText(partial);

  const corrupted = text.replace("(u32) -> u32", "(bool) -> u32");
  assert.notEqual(corrupted, text, "test setup: no '(u32) -> u32' substring found");

  const parsed = plugin.fromText(corrupted, partial);

  assert.throws(
    () => runtime.partialManager.merge(parsed, fixture.component),
    (err) => /signature_mismatch|signature|mismatch/i.test(describeError(err)),
    "merge should reject signature change",
  );
});
