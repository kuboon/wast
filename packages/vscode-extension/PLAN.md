# VS Code Extension — WAST Editor Integration

## Purpose

VS Code extension providing TreeView navigation, virtual document editing, LSP diagnostics, and save-merge workflow for wast components.

## Key Features

- TreeView panel listing wast.db components and their functions
- Virtual documents (`wast://` scheme) for editing partial components
- Real-time LSP diagnostics via syntax-plugin from-text
- Save flow: from-text → merge → write to wast.db
- fs.watch for external wast.db change detection
- Session conflict handling

## User Settings

- `wast.symsLanguage` — syms language suffix (e.g., "ja")
- `wast.syntaxPlugin` — syntax plugin variant (e.g., "ruby-like")

## Dependencies

- Wasm component runtime
- All wasm components (syntax-plugin, partial-manager, file-manager)

## Status

Not started.
