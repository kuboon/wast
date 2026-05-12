// End-to-end test of every flow the VS Code extension drives against the
// bundled wasm components: read → render with each plugin → edit → parse
// → merge → write → re-read. Operates on examples/basic/ as fixture so
// changes to that workspace are caught.
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
  assert.deepEqual(funcUids(fixture.component), ["square", "sum-of-squares"]);
  // Even simple primitives become rows so the json maps 1:1 to SQLite later.
  assert.ok(typeUids(fixture.component).includes("u32"));
});

test("every syntax plugin renders the fixture without crashing", () => {
  for (const [id, plugin] of Object.entries(runtime.plugins)) {
    const text = plugin.toText(fixture.component);
    assert.ok(text.length > 0, `${id} produced empty output`);
    // Every plugin should mention both function names somewhere.
    assert.match(text, /square/, `${id} missing 'square'`);
    assert.match(text, /sum/, `${id} missing 'sum'`);
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

test("signature change is rejected at merge stage (not silently accepted)", () => {
  const plugin = runtime.plugins["ruby-like"];
  const text = plugin.toText(fixture.component);

  // Flip the first u32 param to bool.
  const corrupted = text.replace("(u32) -> u32", "(bool) -> u32");
  assert.notEqual(corrupted, text, "test setup: no '(u32) -> u32' substring found");

  // from_text may accept it (it doesn't cross-check against existing).
  const parsed = plugin.fromText(corrupted, fixture.component);

  // merge must reject — that's where signature/uid invariants are enforced.
  assert.throws(
    () => runtime.partialManager.merge(parsed, fixture.component),
    (err) => /signature_mismatch|signature|mismatch/i.test(describeError(err)),
    "merge should reject signature change",
  );
});

test("compiler.compile produces a non-empty wasm component (after body injection)", () => {
  // Empty bodies (the just-compiled-wit state) fail core validation. Use
  // ts-like to inject real bodies for both exports, then compile — this
  // mirrors the realistic "user edits → compile" flow.
  const ts = runtime.plugins["ts-like"];
  const rendered = ts.toText(fixture.component);
  const withBodies = rendered
    .replace(/\/\/ \[no body\]/, "return x * x;")
    .replace(/\/\/ \[no body\]/, "return square(a) + square(b);");

  // Sanity check: if the placeholder wasn't found, the rest of the test
  // is a lie — surface that loudly with the actual rendered text.
  assert.notEqual(
    withBodies,
    rendered,
    `ts-like placeholder '// [no body]' not found in rendered output:\n${rendered}`,
  );

  const parsed = ts.fromText(withBodies, fixture.component);
  const merged = runtime.partialManager.merge(parsed, fixture.component);

  let wasm;
  try {
    wasm = runtime.compiler.compile(merged, fixture.worldWit);
  } catch (err) {
    assert.fail(
      `compile threw: ${describeError(err)}\n` +
        `--- ts-like.toText(merged) was:\n${ts.toText(merged)}`,
    );
  }
  assert.ok(wasm.byteLength > 0, "compiler produced empty output");
  // Wasm magic: 0x00 0x61 0x73 0x6D.
  assert.deepEqual(
    [wasm[0], wasm[1], wasm[2], wasm[3]],
    [0x00, 0x61, 0x73, 0x6d],
    "output is not a wasm module",
  );
});

test("codec.compileWit reproduces a wast.json matching the committed fixture", () => {
  const out = runtime.codec.compileWit(fixture.worldWit);
  assert.ok(out.wastJson.byteLength > 0);

  // compileWit's output should be byte-identical to what's committed in
  // examples/basic — if not, either world.wit was edited without running
  // `pnpm seed-example`, or codec.compileWit behavior drifted.
  assert.equal(
    Buffer.from(out.wastJson).toString("utf-8"),
    Buffer.from(fixture.wastJson).toString("utf-8"),
    "examples/basic/wast.json out of sync with world.wit — run `pnpm seed-example`",
  );
});
