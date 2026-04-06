# WAST Spec Quick Reference

## Core Concept

WAST = intermediate layer between text files and WASM Components.

```
text <──syntax plugin──> partial WastComponent <──partial manager──> full WastComponent <──file manager──> [wast.db, world.wit, syms]
```

## File Structure

```
component-name/
  wast.db          # Logic (SQLite format)
  world.wit        # Interface definition (source of truth)
  syms.ja.yaml     # Japanese display names
  syms.en.yaml     # English display names
```

## WastComponent (Central Type)

```
type WastComponent = { funcs, types, syms }
```

- Partial and full share the same type — partial is just a subset
- Defined in WIT so components can be wasm components themselves

## Identifier System

All identifiers are **meaningless UIDs**. No content addressing.

| Kind | Format | Example |
|---|---|---|
| Internal function | UID | `$f3a9` |
| WIT-derived | WIT path | `inventory/add-item` |
| Parameter/local | UID (random) | `$a7f2` |

## Syms (3 layers)

| Layer | Key | Content |
|---|---|---|
| `wit` | WIT path | Translation of WIT real names |
| `internal` | func UID | Function display name |
| `local` | param/local UID | Variable display name (cross-scope) |

syms are NOT needed for wasm generation.

## Error Codes

### Stage 1 (syntax plugin / from-text)
- `parse_error`, `unknown_uid`, `invalid_type_ref`, `duplicate_uid`, `missing_body`

### Stage 2 (partial manager / merge)
- `signature_mismatch`, `missing_dependency`, `uid_conflict`

### File/System
- `db_exists`, `db_not_found`, `wit_not_found`, `wit_inconsistency`, `io_error`, `plugin_error`

## CLI Commands

| Command | Description |
|---|---|
| `wast bindgen <dir>` | Generate wast.db from world.wit |
| `wast extract <dir> <uid...>` | Extract partial as text → stdout |
| `wast merge <dir>` | Merge text from stdin → wast.db |
| `wast fmt` | Format/validate wast text |
| `wast diff <dir-a> <dir-b>` | Diff via difftastic |
| `wast syms <dir> <uid> <name>` | Set display name |
| `wast setup-git` | Configure git diff driver |

Exit codes: 0 success, 1 user error, 2 system error. `--json` flag for all commands.

## Control Structures

WAT-inherited loops (loop + br_if). Syntax plugins detect patterns and convert to while/for/try.

WIT type operations are native in wast:
- `(some ...)`, `(none)`, `(match-option ...)`
- `(ok ...)`, `(err ...)`, `(match-result ...)`
- `(variant type/case ...)`, `(match-variant ...)`
- `(record type ($uid val)...)`, `(field $uid ...)`
- `(tuple ...)`, `(destructure ...)`
- `(list ...)`
