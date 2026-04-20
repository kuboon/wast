# file-manager — WASI-based File Manager

## Purpose

Handles persistence of `WastComponent` to/from the filesystem. Manages `wast.json` (current) / `wast.db` (future SQLite), `world.wit` parsing, and `syms.*.yaml` files.

## Storage format roadmap

| Format | Status | Notes |
|---|---|---|
| `wast.json` | **Current** | Row-oriented JSON. Each func/type is a standalone record with inline `uid` — designed to map 1:1 to SQLite rows when migration lands |
| `wast.db` (SQLite) | **Future** | Same logical schema plus indexed `call_edges` table for caller/callee traversal |

## JSON format (current)

Row-oriented so each record maps to a SQLite row without restructuring:

```json
{
  "funcs": [
    {
      "uid": "...",
      "source": {...},
      "params": [["param_uid", "type_uid"], ...],
      "result": "type_uid_or_null",
      "body": [/* bytes */],
      "calls": ["callee_uid_1", "callee_uid_2"]
    }
  ],
  "types": [
    { "uid": "...", "source": {...}, "definition": {...} }
  ]
}
```

- `calls` is a derived index (populated by `file-manager` via `pattern-analyzer::deserialize_body` at write time)
- `calls` is **not** part of the WIT `wast-func` record; it is storage-layer only
- `syms` remain in separate per-language YAML files (`syms.ja.yaml`, `syms.en.yaml`)

## SQLite schema (future target)

```sql
CREATE TABLE funcs (
  uid    TEXT PRIMARY KEY,
  source TEXT NOT NULL,   -- JSON-encoded FuncSource
  params BLOB NOT NULL,   -- JSON or postcard list
  result BLOB,
  body   BLOB
);

CREATE TABLE types (
  uid        TEXT PRIMARY KEY,
  source     TEXT NOT NULL,
  definition BLOB NOT NULL
);

-- Caller/callee edges (populated from body at write time)
CREATE TABLE call_edges (
  caller_uid TEXT NOT NULL,
  callee_uid TEXT NOT NULL,
  ordinal    INTEGER NOT NULL,
  PRIMARY KEY (caller_uid, callee_uid, ordinal),
  FOREIGN KEY (caller_uid) REFERENCES funcs(uid),
  FOREIGN KEY (callee_uid) REFERENCES funcs(uid)
);
CREATE INDEX idx_callee ON call_edges(callee_uid, caller_uid);

-- syms tables stay in per-language YAML for now; may move into DB later
```

Why a dedicated `call_edges` table: both directions are cheap.
- callees of X: `WHERE caller_uid = ?` (PK prefix match)
- callers of X: `WHERE callee_uid = ?` (uses `idx_callee`)

## Interfaces

Exports: `file-manager` (bindgen, read, write, merge) via WIT.

## Key Responsibilities

- Read/write `wast.json` (SQLite `wast.db` in the future)
- Parse `world.wit` and validate consistency with wast data
- Read/write `syms.*.yaml` per language
- Validate on write/merge that func signatures match `world.wit`
- Populate `calls` edge index using `pattern-analyzer` (TODO)

## Dependencies

- `wast-types` (TODO — shared serde types crate to be extracted)
- `wast-pattern-analyzer` (for `calls` index; TODO)
- `wit/wast-core.wit`
- WASI filesystem interfaces

## Testing Strategy

- Unit tests for JSON roundtrip (current)
- Tests for `world.wit` consistency validation
- Error case tests: `db_exists`, `db_not_found`, `wit_not_found`, `wit_inconsistency`
- Future: SQLite serialization roundtrip tests

## Status

- JSON format: **Done** (row-oriented)
- SQLite migration: **Not started**
- `calls` index: **Not started**
