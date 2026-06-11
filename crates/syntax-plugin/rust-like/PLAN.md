# rust-like — Future Work

Current state: signatures fully parse in `from_text`; function bodies are
preserved verbatim from the `existing` component via a brace-depth-aware body
skip. See AGENTS.md.

## Remaining

- **Recursive-descent body parser** — replace the preservation-based body skip
  with a real parser so body edits in Rust-like text round-trip (parse to
  `Vec<Instruction>`, serialize via `wast-pattern-analyzer`), matching what
  ts-like and raw already do.
