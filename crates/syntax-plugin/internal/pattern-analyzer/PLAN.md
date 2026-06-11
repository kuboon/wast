# pattern-analyzer — Future Work

Current state (see AGENTS.md): pure-Rust rlib providing the `Instruction` IR,
versioned body (de)serialization, and `analyze()` pattern detection
(while / for / for-in / try).

## Remaining

- **Promote to a WIT interface** (maybe) — make pattern analyzers swappable
  components instead of an internal Rust library, if non-Rust plugins ever
  need shared pattern detection.
