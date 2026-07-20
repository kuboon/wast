# VS Code Extension — Future Work

Current state (TreeView, `wast://` virtual docs — editable for
editor-capable plugins with the `from_text` → `merge` → `codec.write` save
flow, read-only for renderer-only plugins — fs.watch refresh, compile
command) is described in AGENTS.md ("VS Code extension status").

## Remaining

- **Phase 4: vscode-web compatibility** — resolve jco's bare-specifier imports
  (`@bytecodealliance/preview2-shim/*`) under the web extension host
  (currently relies on Node's `node_modules` resolution), so the extension
  works on vscode.dev.
- **LSP diagnostics** — real-time `from_text` validation while editing
  (editor-capable syntaxes only).
- **Rename from read-only panes** — renaming is a syms-only edit that needs
  no parser; offer a rename action on identifiers in renderer-only panes
  (write path: update `syms.<lang>.yaml` via codec, not `from_text`).
- **Session conflict handling** — detect/resolve concurrent edits to the same
  `wast.json` (e.g. external change while a virtual doc is dirty).
