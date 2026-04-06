# CLI — wast Command Line Tool

## Purpose

TypeScript CLI for interacting with wast components. Designed for use by coding AIs — no interactive features.

## Commands

| Command | Description |
|---|---|
| `wast bindgen <dir>` | Generate wast.db from world.wit |
| `wast extract <dir> <uid...>` | Extract partial component as text to stdout |
| `wast merge <dir>` | Merge text from stdin into wast.db |
| `wast fmt` | Format/validate wast text from stdin |
| `wast diff <dir-a> <dir-b>` | Diff two components via difftastic |
| `wast syms <dir> <uid> <name>` | Set display name in syms file |
| `wast setup-git` | Configure git diff driver |

## Key Design

- `--json` flag for machine-readable output on all commands
- Exit codes: 0 success, 1 user error, 2 system error
- Environment variables: `WAST_PLUGIN`, `WAST_SYMS`
- stdin/stdout as pipe primitives

## Dependencies

- Wasm component runtime (for loading syntax-plugin, partial-manager, file-manager)
- difftastic (for diff command)

## Status

Not started.
