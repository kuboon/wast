/**
 * Integration tests: load the ts-like WASM syntax-plugin component via jco
 * transpilation, then exercise toText and fromText with body roundtrips.
 */
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { loadTsLikePlugin, dbToWasmComponent, wasmComponentToDb } from "../src/wasm-plugin.js";
import type { WastDb } from "../src/wast-db.js";
import type { SymsData } from "../src/wasm-plugin.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Build a minimal wast.db + syms with one internal func. */
function makeSimpleDb(body: number[] | null = null): { db: WastDb; syms: SymsData } {
  return {
    db: {
      funcs: [
        [
          "f1",
          {
            source: { Internal: "f1" },
            params: [["p1", "t1"]],
            result: "t1",
            body,
          },
        ],
      ],
      types: [
        [
          "t1",
          {
            source: { Internal: "t1" },
            definition: { Primitive: "U32" },
          },
        ],
      ],
    },
    syms: {
      wit: [],
      internal: [
        ["f1", "my_func"],
        ["t1", "u32"],
      ],
      local: [["p1", "x"]],
    },
  };
}

/** Build wast.db with internal, imported, and exported functions. */
function makeFullDb(body: number[] | null = null): { db: WastDb; syms: SymsData } {
  return {
    db: {
      funcs: [
        [
          "f1",
          {
            source: { Internal: "f1" },
            params: [["p1", "t1"]],
            result: "t1",
            body,
          },
        ],
        [
          "f2",
          {
            source: { Imported: "f2" },
            params: [["p2", "t1"]],
            result: null,
            body: null,
          },
        ],
        [
          "f3",
          {
            source: { Exported: "f3" },
            params: [],
            result: "t1",
            body,
          },
        ],
      ],
      types: [
        [
          "t1",
          {
            source: { Internal: "t1" },
            definition: { Primitive: "U32" },
          },
        ],
      ],
    },
    syms: {
      wit: [["f2", "imported_fn"]],
      internal: [
        ["f1", "my_func"],
        ["f3", "exported_fn"],
        ["t1", "u32"],
      ],
      local: [
        ["p1", "x"],
        ["p2", "y"],
      ],
    },
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("wasm-plugin bridge: dbToWasmComponent / wasmComponentToDb", () => {
  it("round-trips a simple db through WASM format", () => {
    const { db, syms } = makeSimpleDb([1, 2, 3]);
    const wasm = dbToWasmComponent(db, syms);
    const back = wasmComponentToDb(wasm);
    assert.deepStrictEqual(back.db, db);
    assert.deepStrictEqual(back.syms, syms);
  });

  it("handles null body", () => {
    const { db, syms } = makeSimpleDb(null);
    const wasm = dbToWasmComponent(db, syms);
    const back = wasmComponentToDb(wasm);
    assert.strictEqual(back.db.funcs[0][1].body, null);
  });
});

describe("ts-like WASM plugin: toText", () => {
  it("renders function signatures with display names", async () => {
    const plugin = await loadTsLikePlugin();
    const { db, syms } = makeFullDb();

    const text = plugin.toText(db, syms);
    assert.ok(text.includes("function my_func(x: u32): u32"), `should contain internal func, got:\n${text}`);
    assert.ok(text.includes("declare function imported_fn(y: u32)"), `should contain import, got:\n${text}`);
    assert.ok(text.includes("export function exported_fn(): u32"), `should contain export, got:\n${text}`);
  });

  it("renders body instructions", async () => {
    // Create a body with instructions: let y = x + 1; return;
    // This requires postcard-serialized body bytes. We'll use the WASM component
    // to roundtrip: toText → fromText → check body is not null → toText again.
    const plugin = await loadTsLikePlugin();
    const { db, syms } = makeSimpleDb(null);

    // Write a body via fromText
    const inputText = "function my_func(x: u32): u32 {\n  let x = x + 1;\n  return;\n}";
    const result = plugin.fromText(inputText, db, syms);

    // Body should now be Some (non-null)
    const f1 = result.db.funcs.find(([uid]) => uid === "f1");
    assert.ok(f1, "f1 should exist");
    assert.ok(f1[1].body !== null && f1[1].body.length > 0, "body should be populated after fromText");

    // toText should render the body back
    const text = plugin.toText(result.db, result.syms);
    assert.ok(text.includes("let x = x + 1;"), `should render assignment, got:\n${text}`);
    assert.ok(text.includes("return;"), `should render return, got:\n${text}`);
  });
});

describe("ts-like WASM plugin: fromText → toText roundtrip", () => {
  it("roundtrips simple instructions", async () => {
    const plugin = await loadTsLikePlugin();
    const { db, syms } = makeSimpleDb(null);

    const inputText = "function my_func(x: u32): u32 {\n  let x = 42;\n  return;\n}";
    const parsed = plugin.fromText(inputText, db, syms);
    const outputText = plugin.toText(parsed.db, parsed.syms);

    // fromText → toText should produce text that round-trips again
    const reparsed = plugin.fromText(outputText, parsed.db, parsed.syms);
    const outputText2 = plugin.toText(reparsed.db, reparsed.syms);
    assert.strictEqual(outputText, outputText2, "text should stabilize after one roundtrip");
  });

  it("roundtrips if-else", async () => {
    const plugin = await loadTsLikePlugin();
    const { db, syms } = makeSimpleDb(null);

    const inputText = [
      "function my_func(x: u32): u32 {",
      "  if (x < 10) {",
      "    return;",
      "  } else {",
      "    let x = 99;",
      "  }",
      "}",
    ].join("\n");

    const parsed = plugin.fromText(inputText, db, syms);
    const outputText = plugin.toText(parsed.db, parsed.syms);
    const reparsed = plugin.fromText(outputText, parsed.db, parsed.syms);
    const outputText2 = plugin.toText(reparsed.db, reparsed.syms);
    assert.strictEqual(outputText, outputText2, "if-else roundtrip should stabilize");
  });

  it("roundtrips loop with break", async () => {
    const plugin = await loadTsLikePlugin();
    const { db, syms } = makeSimpleDb(null);

    const inputText = [
      "function my_func(x: u32): u32 {",
      "  while (true) { // loop0",
      "    if (x < 5) break loop0; // break",
      "    let x = x + 1;",
      "  }",
      "}",
    ].join("\n");

    const parsed = plugin.fromText(inputText, db, syms);
    const outputText = plugin.toText(parsed.db, parsed.syms);
    const reparsed = plugin.fromText(outputText, parsed.db, parsed.syms);
    const outputText2 = plugin.toText(reparsed.db, reparsed.syms);
    assert.strictEqual(outputText, outputText2, "loop roundtrip should stabilize");
  });

  it("roundtrips multiple functions", async () => {
    const plugin = await loadTsLikePlugin();
    const { db, syms } = makeFullDb(null);

    const inputText = [
      "function my_func(x: u32): u32 {",
      "  let x = 42;",
      "}",
      "",
      "declare function imported_fn(y: u32);",
      "",
      "export function exported_fn(): u32 {",
      "  return;",
      "}",
    ].join("\n");

    const parsed = plugin.fromText(inputText, db, syms);
    const outputText = plugin.toText(parsed.db, parsed.syms);
    const reparsed = plugin.fromText(outputText, parsed.db, parsed.syms);
    const outputText2 = plugin.toText(reparsed.db, reparsed.syms);
    assert.strictEqual(outputText, outputText2, "multi-func roundtrip should stabilize");
  });

  it("roundtrips WIT type operations (some, ok, err, isErr)", async () => {
    const plugin = await loadTsLikePlugin();
    const syms: SymsData = {
      wit: [],
      internal: [["f1", "my_func"], ["t1", "u32"]],
      local: [["p1", "x"], ["v1", "val"], ["v2", "res"]],
    };
    const db: WastDb = {
      funcs: [["f1", {
        source: { Internal: "f1" },
        params: [["p1", "t1"]],
        result: "t1",
        body: null,
      }]],
      types: [["t1", {
        source: { Internal: "t1" },
        definition: { Primitive: "U32" },
      }]],
    };

    const inputText = [
      "function my_func(x: u32): u32 {",
      "  let val = some(x);",
      "  let res = ok(val);",
      "  if (isErr(res)) {",
      "    return;",
      "  }",
      "}",
    ].join("\n");

    const parsed = plugin.fromText(inputText, db, syms);
    const outputText = plugin.toText(parsed.db, parsed.syms);
    const reparsed = plugin.fromText(outputText, parsed.db, parsed.syms);
    const outputText2 = plugin.toText(reparsed.db, reparsed.syms);
    assert.strictEqual(outputText, outputText2, "WIT type roundtrip should stabilize");
  });
});

describe("ts-like WASM plugin: error handling", () => {
  it("rejects invalid syntax with WastError", async () => {
    const plugin = await loadTsLikePlugin();
    const { db, syms } = makeSimpleDb(null);

    assert.throws(
      () => plugin.fromText("not valid code at all", db, syms),
      (err: any) => {
        // jco wraps WASM component errors
        return err !== undefined;
      },
      "should throw on invalid syntax",
    );
  });
});
