# wit/ — Shared WIT Contract

## Purpose

Defines the shared WebAssembly Interface Types (WIT) for all wasm components in the project. `wast-core.wit` is the single source of truth for all inter-component contracts.

## Key Interfaces

- **types** — Core data types: WastComponent, WastFunc, WastTypeDef, Syms, etc.
- **syntax-plugin** — to-text / from-text conversion
- **partial-manager** — extract / merge operations
- **file-manager** — read / write / merge to filesystem

## Worlds

- `syntax-plugin-world` — exports syntax-plugin
- `partial-manager-world` — exports partial-manager
- `file-manager-world` — exports file-manager

## Status

Skeleton — initial types defined, may need refinement as implementation progresses.

## Dependencies

None (this is the root contract).
