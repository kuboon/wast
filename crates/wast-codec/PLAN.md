# wast-codec — Future Work

Current state (content-based read/write/merge/compile-wit, row-oriented
`wast.json` with required `version` field) is described in AGENTS.md.

## Remaining

- **Populate `calls: Vec<String>` on each func at write time** via
  `pattern-analyzer::deserialize_body` — a caller→callee edge index for the
  future SQLite indexing.
- **Migrate storage to SQLite (`wast.db`)** once the JSON compiler path
  stabilizes. Partial r/w is impossible in vscode-web (workspace fs is
  whole-file only), so the codec stays byte-oriented and the host (or a future
  host-imported page-level VFS) decides chunking.
