# VS Code Extension — Future Work

Current state (TreeView, editable `wast://` virtual docs with the
`from_text` → `merge` → `codec.write` save flow, fs.watch refresh, compile
command) is described in AGENTS.md ("VS Code extension status").

## Remaining

- **Phase 4: vscode-web compatibility** — resolve jco's bare-specifier imports
  (`@bytecodealliance/preview2-shim/*`) under the web extension host
  (currently relies on Node's `node_modules` resolution), so the extension
  works on vscode.dev.
- **LSP diagnostics** — real-time `from_text` validation while editing.
- **Session conflict handling** — detect/resolve concurrent edits to the same
  `wast.json` (e.g. external change while a virtual doc is dirty).
