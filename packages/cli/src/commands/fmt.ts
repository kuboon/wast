import type { GlobalOptions } from "../index.js";
import { errorExit } from "../index.js";

function readStdin(): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    process.stdin.on("data", (chunk) => chunks.push(chunk));
    process.stdin.on("end", () => resolve(Buffer.concat(chunks).toString("utf-8")));
    process.stdin.on("error", reject);
  });
}

/**
 * Basic validation that the text looks like wast text format.
 * Returns list of warnings (empty = valid).
 */
function validate(text: string): string[] {
  const warnings: string[] = [];
  const lines = text.split("\n");

  // Check for at least one func or type or comment
  const hasFuncOrType = lines.some((l) => /^\s*(func|type)\s/.test(l));
  const hasComment = lines.some((l) => /^\s*#/.test(l));
  const isEmpty = text.trim().length === 0;

  if (isEmpty) {
    // Empty input is valid (no-op)
    return [];
  }

  if (!hasFuncOrType && !hasComment) {
    warnings.push("input does not contain any func or type definitions");
  }

  return warnings;
}

export async function fmt(_positionals: string[], options: GlobalOptions): Promise<void> {
  const input = await readStdin();

  if (input.trim().length === 0) {
    // Empty input -> empty output
    if (options.json) {
      console.log(JSON.stringify({ ok: true, command: "fmt", text: "" }));
    }
    return;
  }

  const warnings = validate(input);

  if (warnings.length > 0 && !options.json) {
    for (const w of warnings) {
      console.error(`warning: ${w}`);
    }
  }

  // Passthrough for now — normalize trailing newline
  const formatted = input.trimEnd() + "\n";

  if (options.json) {
    console.log(
      JSON.stringify({
        ok: true,
        command: "fmt",
        warnings,
        text: formatted,
      }),
    );
  } else {
    process.stdout.write(formatted);
  }
}
