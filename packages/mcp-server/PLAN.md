# mcp-server — Future Work

Current state (six tools over stdio, path containment, write-then-verify) is
described in AGENTS.md ("MCP server status") and
[README.md](README.md).

## Remaining

- **Diff output** — `wast_write` reports which files changed but not *what*
  changed. Rendering before/after through a read-only syntax and returning a
  unified diff would let an agent show its work, and is the review-facing half
  of the multi-syntax projection.
- **Create a component** — there's no tool for `codec.compile-wit`, so an
  agent can't start a new component from a `world.wit`. Only wire it once the
  write path's guardrails apply to file creation too.
- **Concurrent-edit detection** — the VS Code provider compares source mtimes
  before saving; this server doesn't. Two agents (or an agent and an editor)
  writing the same component would silently clobber each other.
- **MCP resources** — the tools are all verbs. Exposing components as
  resources would let a client browse them without a tool call, but only
  matters once a client wants that.
