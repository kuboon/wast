// Every playground sample, taken through the exact chain the page uses:
//
//   lisp source → raw.from_text → compiler.compile_core → WebAssembly → call
//
// This is the compiler's coverage record. A sample that stops compiling, or
// an export that stops returning what it should, fails here rather than
// silently degrading the page.

import { test } from "node:test";
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { SAMPLES, DEFAULT_SAMPLE } from "../src/samples.js";

const here = dirname(fileURLToPath(import.meta.url));
const plugins = join(here, "..", "public", "plugins");
const tools = join(here, "..", "public", "tools");

const raw = await import(pathToFileURL(join(plugins, "raw", "raw.js")).href);
const { compiler } = await import(
  pathToFileURL(join(tools, "compiler", "compiler.js")).href
);

const EMPTY = { funcs: [], types: [], syms: { witSyms: [], internal: [], local: [] } };

function describeError(thrown) {
  const payload = thrown?.payload ?? thrown?.cause ?? thrown?.value;
  const list = Array.isArray(payload) ? payload : Array.isArray(thrown) ? thrown : null;
  if (list) return list.map((e) => e.message ?? e).join("; ");
  return payload?.message ?? thrown?.message ?? String(thrown);
}

/** Compile a sample the way the page does, returning its live instance. */
async function build(sample) {
  let component;
  try {
    component = raw.syntaxEditor.fromText(sample.source, EMPTY);
  } catch (err) {
    assert.fail(`${sample.id}: parse failed — ${describeError(err)}`);
  }

  let bytes;
  try {
    bytes = compiler.compileCore(component);
  } catch (err) {
    assert.fail(`${sample.id}: compile failed — ${describeError(err)}`);
  }

  // The whole point: no import object. An import-free program carries its
  // own memory and defines cabi_realloc, so the browser needs nothing.
  const { instance } = await WebAssembly.instantiate(bytes);
  return { component, bytes, instance };
}

/** Read a Canonical-ABI string return out of the module's own memory. */
function readString(instance, ret) {
  const view = new DataView(instance.exports.memory.buffer);
  const bytes = new Uint8Array(instance.exports.memory.buffer);
  const ptr = view.getInt32(ret, true);
  const len = view.getInt32(ret + 4, true);
  return new TextDecoder().decode(bytes.subarray(ptr, ptr + len));
}

test("the default sample exists", () => {
  assert.ok(SAMPLES.some((s) => s.id === DEFAULT_SAMPLE));
});

test("sample ids and labels are unique", () => {
  assert.equal(new Set(SAMPLES.map((s) => s.id)).size, SAMPLES.length);
  assert.equal(new Set(SAMPLES.map((s) => s.label)).size, SAMPLES.length);
});

for (const sample of SAMPLES) {
  test(`sample '${sample.id}' compiles to a core module and instantiates`, async () => {
    const { bytes, instance } = await build(sample);

    // Core module, not a Component: version 1, not 0x0d.
    assert.deepEqual(
      [...bytes.slice(0, 8)],
      [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
      "must be a core wasm module — a Component would not instantiate here",
    );
    assert.ok(instance.exports.memory instanceof WebAssembly.Memory);

    for (const call of sample.calls ?? []) {
      assert.equal(
        typeof instance.exports[call.func],
        "function",
        `${sample.id}: '${call.func}' is not exported (got: ${Object.keys(instance.exports).join(", ")})`,
      );
    }
  });

  if (sample.calls?.length) {
    test(`sample '${sample.id}' runs its listed calls`, async () => {
      const { instance } = await build(sample);
      for (const call of sample.calls) {
        const fn = instance.exports[call.func];
        assert.equal(
          fn.length,
          call.args.length,
          `${sample.id}: '${call.func}' takes ${fn.length} core value(s), sample passes ${call.args.length}`,
        );
        const result = fn(...call.args);
        assert.equal(
          typeof result,
          "number",
          `${sample.id}: '${call.func}' should return a core value`,
        );
      }
    });
  }
}

test("arithmetic sample computes the expected values", async () => {
  const { instance } = await build(SAMPLES.find((s) => s.id === "arithmetic"));
  assert.equal(instance.exports.cube(3), 27);
  assert.equal(instance.exports.poly(4), 4 * 4 + 4 * 4 * 4);
});

test("branching sample picks the larger value", async () => {
  const { instance } = await build(SAMPLES.find((s) => s.id === "branching"));
  assert.equal(instance.exports.max2(11, 4), 11);
  assert.equal(instance.exports.max3(3, 9, 6), 9);
  assert.equal(instance.exports["abs-diff"](4, 10), 6);
});

test("loop sample iterates", async () => {
  const { instance } = await build(SAMPLES.find((s) => s.id === "loop"));
  assert.equal(instance.exports["sum-to"](10), 55, "1+…+10");
  assert.equal(instance.exports.factorial(5), 120);
});

test("option sample destructures both cases", async () => {
  // option<u32> flattens to (discriminant, payload) across the ABI.
  const { instance } = await build(SAMPLES.find((s) => s.id === "option"));
  assert.equal(instance.exports["unwrap-or"](1, 42, 7), 42, "some(42)");
  assert.equal(instance.exports["unwrap-or"](0, 0, 7), 7, "none");
});

test("record sample reads a field of a flattened record param", async () => {
  const { instance } = await build(SAMPLES.find((s) => s.id === "record"));
  assert.equal(instance.exports["get-x"](3, 4), 3);
});

test("string sample returns text readable from the module's memory", async () => {
  const { instance } = await build(SAMPLES.find((s) => s.id === "string"));
  assert.equal(readString(instance, instance.exports.greeting()), "hello, wast!");
  assert.equal(readString(instance, instance.exports.shout()), "COMPILED IN YOUR BROWSER");
});

test("emit-wat exposes the generated core module text", async () => {
  const sample = SAMPLES.find((s) => s.id === "arithmetic");
  const component = raw.syntaxEditor.fromText(sample.source, EMPTY);
  const wat = compiler.emitWat(component);
  assert.match(wat, /^\(module/, "should be a core module, not a component");
  assert.match(wat, /\(memory \(export "memory"\)/);
  assert.match(wat, /cabi_realloc/, "realloc is defined, not imported");
});
