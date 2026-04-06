# pattern-analyzer — Internal Library Crate

## Purpose

Shared library for syntax plugins. Analyzes WastComponent bodies to detect high-level control flow patterns and tag them for syntax plugin text generation.

## Pattern Detection

| wast pattern | Text display |
|---|---|
| `loop + br_if` (head condition) | `while` |
| `loop + br_if` (counter variable) | `for` |
| `loop + br_if` (list index) | `for in` |
| `if (is_err) + return` (result unwrap) | `?` / `try` |

Unrecognized patterns fall back to raw wast syntax.

## Design Notes

- Pure Rust library crate (not a wasm component)
- Internal implementation detail — not part of public WIT spec
- Future: may be promoted to WIT interface for swappable pattern analyzers

## Dependencies

None (pure Rust).

## Testing Strategy

- Unit tests for each pattern type with sample wast bodies
- Fallback behavior tests for unrecognized patterns

## Status

Not started.
