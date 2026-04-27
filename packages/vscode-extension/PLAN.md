# VS Code Extension — WAST Editor Integration

## Purpose

VS Code extension providing TreeView navigation, virtual document editing, LSP diagnostics, and save-merge workflow for wast components.
Should be work on vscode.dev web.

## Key Features

- TreeView panel listing wast.json components and their functions
- Virtual documents (`wast://` scheme) for editing partial components
- Real-time LSP diagnostics via syntax-plugin from-text
- Save flow: from-text → merge → write to wast.json
- fs.watch for external wast.json change detection
- Session conflict handling

## User Settings

- `wast.symsLanguage` — syms language suffix (e.g., "ja")
- `wast.syntaxPlugin` — syntax plugin variant (e.g., "ruby-like")

## Dependencies

- Wasm component runtime
- All wasm components (syntax-plugin, partial-manager, wast-codec)

## Status

Core features implemented (direct file access, no wasm runtime):
- TreeView panel listing wast.json components and their functions
- Virtual documents (`wast://` scheme) for viewing component functions
- fs.watch for external wast.json change detection with TreeView refresh
- Settings for symsLanguage and syntaxPlugin

Not yet implemented (requires wasm component runtime):
- Real-time LSP diagnostics via syntax-plugin from-text
- Save flow: from-text -> merge -> write to wast.json
- Session conflict handling
