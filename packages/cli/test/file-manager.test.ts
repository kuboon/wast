/**
 * Integration tests: file-manager WASM component.
 * Exercises bindgen (world.wit → wast.db), read, write, and merge.
 */
import { describe, it, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, readFileSync, existsSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { loadFileManager } from "../src/wasm-plugin.js";
import type { WastDb } from "../src/wast-db.js";

let tmpDir: string;

function makeTmpDir(): string {
  return mkdtempSync(join(tmpdir(), "wast-fm-test-"));
}

function cleanup(dir: string): void {
  rmSync(dir, { recursive: true, force: true });
}

const SIMPLE_WORLD_WIT = `package test:example@0.1.0;

world example {
  import greet: func(name: string) -> string;
  export run: func() -> string;
}
`;

describe("file-manager WASM: bindgen", () => {
  beforeEach(() => { tmpDir = makeTmpDir(); });
  afterEach(() => { cleanup(tmpDir); });

  it("creates wast.db from world.wit with imports and exports", async () => {
    writeFileSync(join(tmpDir, "world.wit"), SIMPLE_WORLD_WIT);

    const fm = await loadFileManager();
    fm.bindgen(tmpDir);

    // wast.db should now exist
    const dbPath = join(tmpDir, "wast.db");
    assert.ok(existsSync(dbPath), "wast.db should be created");

    // Read it back
    const raw = readFileSync(dbPath, "utf-8");
    const db = JSON.parse(raw) as WastDb;

    // Should have funcs for import greet and export run
    assert.strictEqual(db.funcs.length, 2, `should have 2 funcs, got ${db.funcs.length}`);

    const funcNames = db.funcs.map(([uid]) => uid);
    // The import func should have wit_path like "greet/greet"
    const hasImport = db.funcs.some(([, f]) => "Imported" in f.source);
    const hasExport = db.funcs.some(([, f]) => "Exported" in f.source);
    assert.ok(hasImport, `should have imported func, funcs: ${JSON.stringify(funcNames)}`);
    assert.ok(hasExport, `should have exported func, funcs: ${JSON.stringify(funcNames)}`);

    // Should have types for 'string'
    assert.ok(db.types.length >= 1, `should have at least 1 type, got ${db.types.length}`);
  });

  it("fails when world.wit does not exist", async () => {
    // Don't create world.wit
    const fm = await loadFileManager();
    assert.throws(
      () => fm.bindgen(tmpDir),
      (err: any) => err !== undefined,
      "should throw when world.wit missing",
    );
  });

  it("fails when wast.db already exists", async () => {
    writeFileSync(join(tmpDir, "world.wit"), SIMPLE_WORLD_WIT);
    writeFileSync(join(tmpDir, "wast.db"), "{}");

    const fm = await loadFileManager();
    assert.throws(
      () => fm.bindgen(tmpDir),
      (err: any) => err !== undefined,
      "should throw when wast.db exists",
    );
  });
});

describe("file-manager WASM: read", () => {
  beforeEach(() => { tmpDir = makeTmpDir(); });
  afterEach(() => { cleanup(tmpDir); });

  it("reads back a bindgen-created component", async () => {
    writeFileSync(join(tmpDir, "world.wit"), SIMPLE_WORLD_WIT);

    const fm = await loadFileManager();
    fm.bindgen(tmpDir);

    const { db, syms } = fm.read(tmpDir);
    assert.strictEqual(db.funcs.length, 2, "should read 2 funcs");
    assert.strictEqual(syms.wit.length, 2, "should have 2 wit syms");
  });
});

describe("file-manager WASM: write + validate", () => {
  beforeEach(() => { tmpDir = makeTmpDir(); });
  afterEach(() => { cleanup(tmpDir); });

  it("writes a valid component to disk", async () => {
    writeFileSync(join(tmpDir, "world.wit"), SIMPLE_WORLD_WIT);

    const fm = await loadFileManager();
    fm.bindgen(tmpDir);

    // Read, then write back (should succeed — same data)
    const { db, syms } = fm.read(tmpDir);
    fm.write(tmpDir, db, syms);

    // Read again and verify
    const { db: db2 } = fm.read(tmpDir);
    assert.strictEqual(db2.funcs.length, db.funcs.length, "func count should match");
  });
});
