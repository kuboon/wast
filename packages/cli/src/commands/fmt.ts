import type { GlobalOptions } from "../index.js";

export function fmt(_positionals: string[], options: GlobalOptions): void {
  if (options.json) {
    console.log(JSON.stringify({ ok: false, command: "fmt", errors: [{ code: "not_ready", message: "fmt requires wasm runtime (syntax-plugin) which is not yet integrated" }] }));
  } else {
    console.error("error: fmt requires wasm runtime (syntax-plugin) which is not yet integrated");
  }
  process.exit(2);
}
