# @wast/mcp-server

An MCP server that exposes wast's **structured write path**: an agent reads
funcs as JSON, edits the IR directly, and writes it back through validation —
no surface syntax, no parser, no guessing about identity.

```
wast_list_components → wast_list_funcs → wast_read → (edit) → wast_write → wast_compile
                                                                  ↓
                                                             wast_render (human view)
```

## Why not just edit text

Display syntaxes (`ruby-like`, `rust-like`) are read-only projections, and
even the editable ones (`raw`, `ts-like`) put a parser between the edit and
the IR. This server skips that: `wast_read` hands back the IR itself with
**uids explicit and bodies as instruction trees**, so

- a rename is a `name` field edit — the uid never moves, and nothing has to
  infer "rename" from "the text changed";
- a body edit is a tree edit, so there is no syntax to get wrong;
- every write is validated against the rest of the program before it lands.

## Running it

```bash
pnpm --filter @wast/mcp-server build     # bundles the wasm components it drives
node packages/mcp-server/src/server.mjs /path/to/workspace
```

The single argument is the **root**: every tool argument is resolved inside
it, and paths that escape are refused. It also reads `$WAST_ROOT`, and falls
back to the current directory.

Registering it with Claude Code:

```bash
claude mcp add wast -- node /abs/path/to/packages/mcp-server/src/server.mjs /abs/path/to/workspace
```

Any MCP client works — the transport is stdio.

## Tools

| Tool | What it does |
|---|---|
| `wast_list_components` | Component directories (those holding a `wast.json`) under the root |
| `wast_list_funcs` | Funcs and types with signatures and display names, no bodies — cheap orientation |
| `wast_read` | Funcs as an editable JSON document (uids + instruction trees), narrowed with `funcs` |
| `wast_write` | Apply an edited document: validate → persist → compile |
| `wast_compile` | Compile to a wasm Component under `<component>/dist/` |
| `wast_render` | Project the IR through any syntax plugin, for a human-readable view |

`wast_read`/`wast_render` take `funcs` to narrow the view. Narrowing runs
`partial-manager.extract`, which pulls in the signatures of everything the
selected funcs call, so an edit can be type-checked without loading the whole
program. Pass `include_callers: true` when the edit changes a signature —
otherwise the merge refuses it, because the call sites that live outside the
view were never revalidated.

## The document

```json
{
  "version": 1,
  "funcs": [
    {
      "uid": "square",
      "source": "internal",
      "wit_name": "square",
      "name": "square",
      "params": [{ "uid": "x", "type": "u32", "name": "x" }],
      "result": "u32",
      "body": [
        {
          "Arithmetic": {
            "op": "Mul",
            "lhs": { "LocalGet": { "uid": "x" } },
            "rhs": { "LocalGet": { "uid": "x" } }
          }
        }
      ]
    }
  ]
}
```

Instructions are serde's externally-tagged form: `{"Variant": {…}}`, or a bare
`"Nop"` / `"Return"` / `"None"` for the payload-less ones. A call's args are
`[param_uid, value]` pairs, and string literals are plain strings. The shape
comes straight from the one `Instruction` definition the compiler reads, so it
cannot drift from the real IR — see
[`crates/syntax-plugin/ir-json`](../../crates/syntax-plugin/ir-json/).

**Omitting a field means "leave it alone."** Drop `body` to edit only a
signature; drop `name` to keep a display name; drop a func from `funcs`
entirely and it is untouched. An explicit `""` clears a name, and `[]` clears
a body. Leave `uid` off a brand-new func and one is generated.

The signature fields — `source`, `wit_name`, `params`, `result` — are
**required**, and unknown fields are rejected. Because omission carries
meaning here, a typo'd or forgotten key would otherwise be indistinguishable
from a deliberate omission: the edit would vanish and the write would still
report success. Use `"result": null` for a func that returns nothing.

## What `wast_write` checks

Errors start with a stable machine-readable code, so a failed write tells you
what to fix rather than just that something broke:

| Code | Meaning |
|---|---|
| `parse_error` | the document isn't valid JSON, or an instruction shape is unknown |
| `unsupported_version` | `version` isn't one this build understands |
| `duplicate_uid` | the same uid appears twice in the document |
| `signature_mismatch` | a boundary signature disagrees with the rest of the program |
| `uid_conflict` | the uid exists elsewhere with an incompatible kind |
| `missing_dependency` | a body calls a func that exists nowhere |
| `caller_not_included` | a signature changed while a caller outside the view would break |
| `invalid_body` | body bytes don't deserialize |
| `unknown_local` | a body reads a local that is never a param, assignment, or binding |
| `call_arity_mismatch` | a call passes the wrong number of args |
| `call_arg_unknown` / `call_arg_duplicate` | a call names a param the callee doesn't have, or names one twice |

A rejected edit writes nothing. `verify` (default `true`) compiles after a
successful write so type errors the merge can't see surface right away; that
compile failure is reported as an error but the write is **not** rolled back —
the files on disk are the edit, and the message says what to fix next.
