import type { GlobalOptions } from "../index.js";
import { loadTsLikePlugin, loadFileManager } from "../wasm-plugin.js";

function readStdin(): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    process.stdin.on("data", (chunk) => chunks.push(chunk));
    process.stdin.on("end", () => resolve(Buffer.concat(chunks).toString("utf-8")));
    process.stdin.on("error", reject);
  });
}

export async function fmt(_positionals: string[], options: GlobalOptions): Promise<void> {
  const input = await readStdin();

  if (input.trim().length === 0) {
    if (options.json) {
      console.log(JSON.stringify({ ok: true, command: "fmt", text: "" }));
    }
    return;
  }

  const plugin = await loadTsLikePlugin();
  // Use file-manager to get an empty component as the "existing" base
  // so fromText can generate new UIDs for unknown names.
  const emptyDb = { funcs: [], types: [] };
  const emptySyms = { wit: [] as [string, string][], internal: [] as [string, string][], local: [] as [string, string][] };

  let formatted: string;
  const errors: string[] = [];

  try {
    // Parse text → WastComponent → render back to text (normalized form)
    const result = plugin.fromText(input, emptyDb, emptySyms);
    formatted = plugin.toText(result.db, result.syms);
  } catch (err: any) {
    // fromText might throw on invalid syntax — report errors
    const msg = err?.message ?? String(err);
    if (options.json) {
      console.log(JSON.stringify({ ok: false, command: "fmt", errors: [{ message: msg }] }));
    } else {
      console.error(`error: ${msg}`);
    }
    process.exit(1);
  }

  if (!formatted.endsWith("\n")) {
    formatted += "\n";
  }

  if (options.json) {
    console.log(JSON.stringify({ ok: true, command: "fmt", errors, text: formatted }));
  } else {
    process.stdout.write(formatted);
  }
}
