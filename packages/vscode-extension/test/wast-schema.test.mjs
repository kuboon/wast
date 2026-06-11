// Unit tests for the wast.json schema validation (src/wast-schema.ts).
// The module is deliberately vscode-free, so we can import the compiled
// dist/wast-schema.js straight into plain Node.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const pkgRoot = join(here, "..");
const { validateWastDb, WAST_DB_CURRENT_VERSION } = await import(
  pathToFileURL(join(pkgRoot, "dist", "wast-schema.js")).href
);

const sampleWastJson = join(pkgRoot, "..", "sample-wast", "wast.json");

test("the committed sample-wast fixture validates", async () => {
  const parsed = JSON.parse(await readFile(sampleWastJson, "utf-8"));
  const result = validateWastDb(parsed);
  assert.deepEqual(result, { ok: true });
  assert.equal(parsed.version, WAST_DB_CURRENT_VERSION);
});

test("missing version is rejected", () => {
  const result = validateWastDb({ funcs: [], types: [] });
  assert.equal(result.ok, false);
  assert.match(result.error, /version/);
});

test("unknown future version is rejected with a helpful message", () => {
  const result = validateWastDb({ version: 99, funcs: [], types: [] });
  assert.equal(result.ok, false);
  assert.match(result.error, /unsupported wast\.json version 99/);
  assert.match(result.error, /Update the WAST extension/);
});

test("non-object root and malformed rows are rejected", () => {
  assert.equal(validateWastDb(null).ok, false);
  assert.equal(validateWastDb([]).ok, false);
  assert.equal(validateWastDb("hi").ok, false);

  // funcs must be an array
  assert.equal(validateWastDb({ version: 1, funcs: {}, types: [] }).ok, false);

  // row without a uid
  const noUid = validateWastDb({
    version: 1,
    funcs: [{ source: { Internal: "f" }, params: [], result: null, body: null }],
    types: [],
  });
  assert.equal(noUid.ok, false);
  assert.match(noUid.error, /uid/);

  // source must be exactly one of Internal/Imported/Exported
  const badSource = validateWastDb({
    version: 1,
    funcs: [{ uid: "f", source: { Bogus: "f" }, params: [], result: null, body: null }],
    types: [],
  });
  assert.equal(badSource.ok, false);
  assert.match(badSource.error, /source/);
});

test("a well-formed minimal db validates", () => {
  const result = validateWastDb({
    version: 1,
    funcs: [
      {
        uid: "f",
        source: { Exported: "f" },
        params: [["x", "u32"]],
        result: "u32",
        body: [1, 2, 3],
      },
    ],
    types: [{ uid: "t", source: { Internal: "t" }, definition: { Primitive: "U32" } }],
  });
  assert.deepEqual(result, { ok: true });
});
