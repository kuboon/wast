import { execSync } from "node:child_process";
import { existsSync, readFileSync, appendFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import type { GlobalOptions } from "../index.js";
import { errorExit } from "../index.js";

export function setupGit(_positionals: string[], options: GlobalOptions): void {
  // Configure git diff driver
  try {
    execSync('git config diff.wast.command "wast diff"', { stdio: "pipe" });
  } catch {
    errorExit("failed to run git config — are you inside a git repository?", options.json);
  }

  // Append to .gitattributes
  const attribsPath = resolve(".gitattributes");
  const attribLine = "wast.db diff=wast";

  if (existsSync(attribsPath)) {
    const content = readFileSync(attribsPath, "utf-8");
    if (!content.includes(attribLine)) {
      const separator = content.endsWith("\n") ? "" : "\n";
      appendFileSync(attribsPath, `${separator}${attribLine}\n`);
    }
  } else {
    writeFileSync(attribsPath, `${attribLine}\n`);
  }

  if (options.json) {
    console.log(JSON.stringify({ ok: true, command: "setup-git" }));
  } else {
    console.log("git diff driver configured");
    console.log(`.gitattributes updated: ${attribsPath}`);
  }
}
