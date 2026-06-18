// Unit tests for src/format.js — in particular the BigInt (u64/s64)
// handling: results must render exactly (no Number precision loss, no
// JSON.stringify TypeError) and 64-bit inputs must parse to BigInt as the
// jco-transpiled components expect.

import { test } from "node:test";
import assert from "node:assert/strict";
import { formatResult, parseInputField, jsonStringify } from "../src/format.js";

test("formatResult renders a bare u64 BigInt exactly", () => {
  assert.equal(formatResult("u64", 18446744073709551615n), "18446744073709551615");
  // Number(18446744073709551615n) would round to 18446744073709552000.
  assert.notEqual(formatResult("u64", 18446744073709551615n), "18446744073709552000");
  assert.equal(formatResult("s64", -9223372036854775808n), "-9223372036854775808");
});

test("formatResult handles BigInt inside option/list/record without throwing", () => {
  // option<u64>: the payload itself is a BigInt → bare digits.
  assert.equal(formatResult("option<u64>", 2n ** 60n), `some(${(2n ** 60n).toString()})`);
  assert.equal(formatResult("option<u64>", null), "none");
  // Nested BigInts render as quoted digit strings (lossless; plain
  // JSON.stringify would throw a TypeError here).
  assert.equal(formatResult("list<u64>", [1n, 2n ** 63n]), '["1","9223372036854775808"]');
  assert.equal(
    formatResult("record", { a: 2n ** 53n + 1n, b: "x" }),
    '{"a":"9007199254740993","b":"x"}',
  );
});

test("formatResult still handles the non-BigInt shapes", () => {
  assert.equal(formatResult("u32", 42), "42");
  assert.equal(formatResult("string", "hi"), '"hi"');
  assert.equal(formatResult("option<u32>", 3), "some(3)");
  assert.equal(formatResult("list<u32>", [1, 2]), "[1,2]");
  assert.equal(formatResult("anything", undefined), "(void)");
});

test("parseInputField parses 64-bit params as BigInt (no precision loss)", () => {
  const u64 = parseInputField({ kind: "u64" }, "18446744073709551615");
  assert.equal(typeof u64, "bigint");
  assert.equal(u64, 18446744073709551615n);

  const i64 = parseInputField({ kind: "i64" }, " -9223372036854775808 ");
  assert.equal(i64, -9223372036854775808n);
});

test("parseInputField keeps 32-bit/float/bool/JSON behaviour", () => {
  assert.equal(parseInputField({ kind: "u32" }, "7"), 7);
  assert.equal(parseInputField({ kind: "f64" }, "1.5"), 1.5);
  assert.equal(parseInputField({ kind: "bool" }, "true"), true);
  assert.deepEqual(parseInputField({ kind: "list" }, "[1,2]"), [1, 2]);
});

test("jsonStringify never throws on BigInt anywhere in the value", () => {
  assert.equal(jsonStringify(5n), "5");
  assert.equal(jsonStringify({ v: 5n }), '{"v":"5"}');
  assert.equal(jsonStringify([{ v: [5n] }]), '[{"v":["5"]}]');
});
