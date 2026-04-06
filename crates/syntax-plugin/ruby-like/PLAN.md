# ruby-like — Ruby-like Syntax Plugin

## Purpose

Wasm component implementing `syntax-plugin` interface with Ruby-like text syntax.

## Interfaces

Exports: `syntax-plugin` (to-text, from-text)

## Key Responsibilities

- Convert WastComponent to Ruby-like text (using syms for display names)
- Parse Ruby-like text back to WastComponent
- Generate new UIDs for new identifiers
- Stage 1 validation (parse errors, unknown UIDs, etc.)

## Dependencies

- `wit/wast-core.wit`
- `wast-pattern-analyzer` (internal library)

## Status

Not started.
