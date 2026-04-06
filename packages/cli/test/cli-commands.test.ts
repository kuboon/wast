/**
 * Integration tests for the CLI commands that use WASM components.
 * Tests extract, merge, fmt, and diff via their underlying WASM pipelines.
 */
import { describe, it, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, readFileSync, existsSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  loadFileManager,
  loadPartialManager,
  loadTsLikePlugin,
} from "../src/wasm-plugin.js";
import type { WastDb } from "../src/wast-db.js";
import type { SymsData } from "../src/wasm-plugin.js";

let tmpDir: string;

function makeTmpDir(): string {
  return mkdtempSync(join(tmpdir(), "wast-cli-test-"));
}
function cleanup(dir: string): void {
  rmSync(dir, { recursive: true, force: true });
}

const WORLD_WIT = `package test:example@0.1.0;

world example {
  import greet: func(name: string) -> string;
  export run: func() -> string;
}
`;

/** Set up a component dir with world.wit and wast.db via file-manager bindgen. */
async function setupComponent(dir: string): Promise<void> {
  writeFileSync(join(dir, "world.wit"), WORLD_WIT);
  const fm = await loadFileManager();
  fm.bindgen(dir);
}

// -----------------------------------------------------------------------
// fmt
// -----------------------------------------------------------------------

describe("CLI fmt: text → WastComponent → text", () => {
  it("normalizes function text through syntax-plugin roundtrip", async () => {
    const plugin = await loadTsLikePlugin();
    const emptyDb = { funcs: [], types: [] } as WastDb;
    const emptySyms: SymsData = { wit: [], internal: [], local: [] };

    // Input with extra whitespace — fmt should normalize
    const input = "function   my_func(  x : u32 ):u32 {\n  let x = 42;\n  return;\n}\n";
    const result = plugin.fromText(input, emptyDb, emptySyms);
    const formatted = plugin.toText(result.db, result.syms);

    assert.ok(formatted.includes("function my_func"), "should contain function");
    assert.ok(formatted.includes("let x = 42;"), "should preserve body");
    assert.ok(formatted.includes("return;"), "should preserve return");

    // Second roundtrip should be stable
    const result2 = plugin.fromText(formatted, result.db, result.syms);
    const formatted2 = plugin.toText(result2.db, result2.syms);
    assert.strictEqual(formatted, formatted2, "fmt should be idempotent");
  });

  it("reports error on invalid syntax", async () => {
    const plugin = await loadTsLikePlugin();
    const emptyDb = { funcs: [], types: [] } as WastDb;
    const emptySyms: SymsData = { wit: [], internal: [], local: [] };

    assert.throws(
      () => plugin.fromText("not a valid function definition", emptyDb, emptySyms),
      (err: any) => err !== undefined,
      "should throw on invalid input",
    );
  });
});

// -----------------------------------------------------------------------
// extract
// -----------------------------------------------------------------------

describe("CLI extract: FileManager.read → PartialManager.extract → SyntaxPlugin.toText", () => {
  beforeEach(() => { tmpDir = makeTmpDir(); });
  afterEach(() => { cleanup(tmpDir); });

  it("extracts a function and renders as ts-like text", async () => {
    await setupComponent(tmpDir);

    const fm = await loadFileManager();
    const pm = await loadPartialManager();
    const plugin = await loadTsLikePlugin();

    const { db, syms } = fm.read(tmpDir);

    // Extract the exported "run" function
    const result = pm.extract(db, syms, [{ sym: "run", includeCaller: false }]);
    const text = plugin.toText(result.db, result.syms);

    assert.ok(text.includes("export function run"), `should render export, got:\n${text}`);
    assert.ok(text.includes("string"), `should include return type, got:\n${text}`);
  });

  it("extracts import as declare function", async () => {
    await setupComponent(tmpDir);

    const fm = await loadFileManager();
    const pm = await loadPartialManager();
    const plugin = await loadTsLikePlugin();

    const { db, syms } = fm.read(tmpDir);

    const result = pm.extract(db, syms, [{ sym: "greet", includeCaller: false }]);
    const text = plugin.toText(result.db, result.syms);

    assert.ok(text.includes("declare function greet"), `should render import as declare, got:\n${text}`);
  });
});

// -----------------------------------------------------------------------
// merge
// -----------------------------------------------------------------------

describe("CLI merge: SyntaxPlugin.fromText → FileManager.merge", () => {
  beforeEach(() => { tmpDir = makeTmpDir(); });
  afterEach(() => { cleanup(tmpDir); });

  it("parses text and merges into existing component", async () => {
    await setupComponent(tmpDir);

    const fm = await loadFileManager();
    const plugin = await loadTsLikePlugin();

    // Read existing
    const { db: existingDb, syms: existingSyms } = fm.read(tmpDir);

    // Create text for an updated export function with body
    const text = "export function run(): string {\n  return;\n}\n";
    const parsed = plugin.fromText(text, existingDb, existingSyms);

    // Merge via file-manager
    fm.merge(tmpDir, parsed.db, parsed.syms);

    // Read back and verify the body was updated
    const { db: updatedDb } = fm.read(tmpDir);
    const runFunc = updatedDb.funcs.find(([uid]) => uid === "run");
    assert.ok(runFunc, "run should still exist");
    assert.ok(runFunc![1].body !== null, "run should now have a body");
  });
});

// -----------------------------------------------------------------------
// diff
// -----------------------------------------------------------------------

describe("CLI diff: FileManager.read → SyntaxPlugin.toText comparison", () => {
  let tmpDirA: string;
  let tmpDirB: string;

  beforeEach(() => {
    tmpDirA = makeTmpDir();
    tmpDirB = makeTmpDir();
  });
  afterEach(() => {
    cleanup(tmpDirA);
    cleanup(tmpDirB);
  });

  it("detects identical components", async () => {
    writeFileSync(join(tmpDirA, "world.wit"), WORLD_WIT);
    writeFileSync(join(tmpDirB, "world.wit"), WORLD_WIT);

    const fm = await loadFileManager();
    fm.bindgen(tmpDirA);
    fm.bindgen(tmpDirB);

    const plugin = await loadTsLikePlugin();

    const { db: dbA, syms: symsA } = fm.read(tmpDirA);
    const { db: dbB, syms: symsB } = fm.read(tmpDirB);

    const textA = plugin.toText(dbA, symsA);
    const textB = plugin.toText(dbB, symsB);

    assert.strictEqual(textA, textB, "identical components should produce identical text");
  });

  it("detects differences after merge", async () => {
    writeFileSync(join(tmpDirA, "world.wit"), WORLD_WIT);
    writeFileSync(join(tmpDirB, "world.wit"), WORLD_WIT);

    const fm = await loadFileManager();
    const plugin = await loadTsLikePlugin();

    fm.bindgen(tmpDirA);
    fm.bindgen(tmpDirB);

    // Merge a body into B but not A
    const { db: existingDb, syms: existingSyms } = fm.read(tmpDirB);
    const text = "export function run(): string {\n  return;\n}\n";
    const parsed = plugin.fromText(text, existingDb, existingSyms);
    fm.merge(tmpDirB, parsed.db, parsed.syms);

    // Now read both and compare
    const { db: dbA, syms: symsA } = fm.read(tmpDirA);
    const { db: dbB, syms: symsB } = fm.read(tmpDirB);

    const textA = plugin.toText(dbA, symsA);
    const textB = plugin.toText(dbB, symsB);

    assert.notStrictEqual(textA, textB, "should detect difference after merge");
  });
});
