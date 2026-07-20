# partial-manager — Future Work

Current extract/merge semantics are described in AGENTS.md
("partial-manager semantics"). With the renderer/editor split,
extract/merge is the project's **structured write path**: display
syntaxes are read-only, so IR edits that don't go through a write syntax
(raw, ts-like) land here.

## Remaining

- **First-class agent write interface** — expose extract → modify → merge
  as a tool surface for LLM agents (e.g. an MCP server wrapping the
  component): `extract` hands the agent a partial (target funcs + callee
  signatures), the agent returns a modified partial with bodies as an
  `Instruction` tree in a structured format (JSON), and `merge` validates
  signatures/uids before committing. Needs: a JSON (de)serialization of
  `Instruction` bodies (today they're opaque postcard bytes at this
  boundary) and merge error messages written for machine consumption.
- **Body-level validation at merge** — merge currently checks signatures
  and uid conflicts; deserialize and type-check bodies too, so a bad
  structured edit is rejected at the boundary instead of at compile time.
