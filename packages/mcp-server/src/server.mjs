#!/usr/bin/env node
// MCP framing for the wast structured write path.
//
//   wast-mcp [root]        # root defaults to $WAST_ROOT, then cwd
//
// Everything the tools do is confined to `root`.

import { resolve } from "node:path";

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";

import { PLUGIN_IDS, loadRuntime } from "./runtime.mjs";
import {
  ToolError,
  wastCompile,
  wastListComponents,
  wastListFuncs,
  wastRead,
  wastRender,
  wastWrite,
} from "./tools.mjs";
import { WorkspaceError } from "./workspace.mjs";

const COMPONENT_ARG = {
  type: "string",
  description:
    "Component directory, relative to the server root (the directory holding wast.json). Use wast_list_components to discover these.",
};

const FUNCS_ARG = {
  type: "array",
  items: { type: "string" },
  description:
    "Func uids to narrow to. Omit for the whole component. Narrowing pulls in the signatures of anything the selected funcs call.",
};

const INCLUDE_CALLERS_ARG = {
  type: "boolean",
  description:
    "Also pull in the funcs that call the selected ones, with their bodies. Required if the edit changes a signature — otherwise merge refuses it (caller_not_included) because the hidden call sites were never revalidated.",
};

const LANG_ARG = {
  type: "string",
  description: "Display-name language suffix, i.e. syms.<lang>.yaml. Defaults to \"en\".",
};

export const TOOLS = [
  {
    name: "wast_list_components",
    description:
      "List the wast components under the server root. Start here when you don't know what's in the workspace.",
    inputSchema: { type: "object", properties: {} },
    handler: wastListComponents,
  },
  {
    name: "wast_list_funcs",
    description:
      "List a component's funcs and types with signatures and display names, without bodies. Cheap orientation before reading a func in full.",
    inputSchema: {
      type: "object",
      properties: { component: COMPONENT_ARG, lang: LANG_ARG },
      required: ["component"],
    },
    handler: wastListFuncs,
  },
  {
    name: "wast_read",
    description:
      "Read funcs as an editable JSON document: the wast IR itself, with explicit uids and bodies as instruction trees. Edit this document and pass it to wast_write. Instructions are externally tagged — {\"LocalGet\": {\"uid\": \"x\"}}, or a bare \"Return\"/\"Nop\"/\"None\" for payload-less ones; a call's args are [param_uid, value] pairs; string literals are plain strings.",
    inputSchema: {
      type: "object",
      properties: {
        component: COMPONENT_ARG,
        funcs: FUNCS_ARG,
        include_callers: INCLUDE_CALLERS_ARG,
        lang: LANG_ARG,
      },
      required: ["component"],
    },
    handler: wastRead,
  },
  {
    name: "wast_write",
    description:
      "Apply an edited wast_read document. Validated before anything is written: signatures, uid conflicts, and bodies (calls must name the callee's params exactly; locals must be params, assignment targets, or match bindings). Errors start with a machine-readable code. Omitting a field means \"leave it alone\": drop `body` to edit only a signature, drop `name` to keep a display name, drop a func entirely to leave it untouched. Renaming is a `name` edit — never change a uid to rename something.",
    inputSchema: {
      type: "object",
      properties: {
        component: COMPONENT_ARG,
        document: {
          type: "object",
          description:
            "The edited document from wast_read. May also be passed as a JSON string.",
        },
        verify: {
          type: "boolean",
          description:
            "Compile after writing to catch what merge can't see (default true). The write is not rolled back if compilation fails.",
        },
        lang: LANG_ARG,
      },
      required: ["component", "document"],
    },
    handler: wastWrite,
  },
  {
    name: "wast_compile",
    description:
      "Compile a component to a wasm Component under <component>/dist/. Use this to check the program builds without changing it.",
    inputSchema: {
      type: "object",
      properties: { component: COMPONENT_ARG, lang: LANG_ARG },
      required: ["component"],
    },
    handler: wastCompile,
  },
  {
    name: "wast_render",
    description: `Render funcs as human-readable source text for review. Read-only — edits go through wast_write. Available syntaxes: ${PLUGIN_IDS.join(", ")} (default ruby-like).`,
    inputSchema: {
      type: "object",
      properties: {
        component: COMPONENT_ARG,
        syntax: { type: "string", enum: PLUGIN_IDS, description: "Surface syntax to render." },
        funcs: FUNCS_ARG,
        include_callers: INCLUDE_CALLERS_ARG,
        lang: LANG_ARG,
      },
      required: ["component"],
    },
    handler: wastRender,
  },
];

/** Run one tool by name and wrap the outcome in MCP content. */
export async function callTool(ctx, name, args = {}) {
  const tool = TOOLS.find((t) => t.name === name);
  if (!tool) {
    return {
      isError: true,
      content: [{ type: "text", text: `unknown tool '${name}'` }],
    };
  }
  try {
    const result = await tool.handler(ctx, args);
    const text = typeof result === "string" ? result : JSON.stringify(result, null, 2);
    return { content: [{ type: "text", text }] };
  } catch (err) {
    // ToolError / WorkspaceError messages are written for the caller to act
    // on (they carry the coded wast errors); anything else is a bug, so let
    // its message through too rather than swallowing it.
    const text =
      err instanceof ToolError || err instanceof WorkspaceError
        ? err.message
        : `internal error: ${err?.stack ?? err}`;
    return { isError: true, content: [{ type: "text", text }] };
  }
}

async function main() {
  const root = resolve(process.argv[2] ?? process.env.WAST_ROOT ?? process.cwd());
  const runtime = await loadRuntime();
  const ctx = { root, runtime };

  const server = new Server(
    { name: "wast", version: "0.1.0" },
    { capabilities: { tools: {} } },
  );

  server.setRequestHandler(ListToolsRequestSchema, async () => ({
    tools: TOOLS.map(({ name, description, inputSchema }) => ({
      name,
      description,
      inputSchema,
    })),
  }));

  server.setRequestHandler(CallToolRequestSchema, (request) =>
    callTool(ctx, request.params.name, request.params.arguments ?? {}),
  );

  await server.connect(new StdioServerTransport());
  // stdout is the transport — anything human-facing has to go to stderr.
  process.stderr.write(`wast MCP server ready (root: ${root})\n`);
}

// Only start a transport when executed as a program; importing this module
// (the tests do) must not hijack stdio.
if (process.argv[1] && import.meta.url === `file://${process.argv[1]}`) {
  main().catch((err) => {
    process.stderr.write(`wast MCP server failed to start: ${err?.stack ?? err}\n`);
    process.exit(1);
  });
}
