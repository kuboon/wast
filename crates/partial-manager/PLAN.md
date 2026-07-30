# partial-manager — Future Work

Current extract/merge semantics (including body validation and the error-code
contract) are described in AGENTS.md ("partial-manager semantics"). With the
renderer/editor split, extract/merge is the project's **structured write
path**: display syntaxes are read-only, and edits arrive as `ir-json`
documents from `packages/mcp-server/` or an editor-capable syntax.

## Remaining

- **Type-level body validation** — merge now checks structure (bodies
  deserialize, calls name the callee's params, locals are defined). It does
  not check *types*: a `RecordGet` on a non-record, an arg whose type doesn't
  match the param, an `Arithmetic` on a string. Those still surface only at
  compile time. Doing it here needs the type-resolution logic the compiler's
  `resolve_type` already has, so the honest options are to share that rlib or
  to accept the split.
- **Deletion in a partial** — merge only adds and replaces, so a func can't
  be removed through the write path (dropping it from a document means
  "unchanged", which is what makes minimal edits possible). Removing a func
  needs an explicit signal plus a check that nothing still calls it.
- **Extract by type** — `extract` targets funcs. An agent working on a type
  has to know which funcs mention it; targeting a type uid and pulling in its
  users would be the natural counterpart.
