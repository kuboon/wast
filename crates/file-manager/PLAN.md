# file-manager — WASI-based File Manager

## Purpose

Handles persistence of WastComponent to/from the filesystem. Manages `wast.db` (SQLite), `world.wit` parsing, and `syms.*.yaml` files.

## Interfaces

Exports: `file-manager` (read, write, merge)

## Key Responsibilities

- Read/write wast.db (SQLite format, see spec section 3)
- Parse world.wit and validate consistency with wast.db
- Read/write syms files (YAML, per-language)
- Validate on write/merge that wast.db content conforms to world.wit

## Database Schema

```sql
CREATE TABLE funcs (uid TEXT PRIMARY KEY, source TEXT NOT NULL, params BLOB NOT NULL, result BLOB, body BLOB);
CREATE TABLE types (uid TEXT PRIMARY KEY, source TEXT NOT NULL, definition BLOB NOT NULL);
CREATE TABLE syms_wit (path TEXT PRIMARY KEY, display_name TEXT NOT NULL);
CREATE TABLE syms_internal (uid TEXT PRIMARY KEY, display_name TEXT NOT NULL);
CREATE TABLE syms_local (uid TEXT PRIMARY KEY, display_name TEXT NOT NULL);
```

## Dependencies

- `wit/wast-core.wit`
- WASI filesystem interfaces

## Testing Strategy

- Unit tests for SQLite serialization/deserialization roundtrips
- Tests for world.wit consistency validation
- Error case tests: db_exists, db_not_found, wit_not_found, wit_inconsistency

## Status

Not started.
