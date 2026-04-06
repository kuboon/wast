#!/usr/bin/env node

import { existsSync } from "node:fs";
import { resolve } from "node:path";

import { bindgen } from "./commands/bindgen.js";
import { extract } from "./commands/extract.js";
import { merge } from "./commands/merge.js";
import { fmt } from "./commands/fmt.js";
import { diff } from "./commands/diff.js";
import { syms } from "./commands/syms.js";
import { setupGit } from "./commands/setup-git.js";

// ── Types ──

export interface GlobalOptions {
  json: boolean;
  help: boolean;
  includeCaller: boolean;
  dryRun: boolean;
  plugin: string;
  symsLang: string;
}

// ── Arg parsing ──

function parseArgs(argv: string[]): { command: string | null; positionals: string[]; options: GlobalOptions } {
  const options: GlobalOptions = {
    json: false,
    help: false,
    includeCaller: false,
    dryRun: false,
    plugin: process.env["WAST_PLUGIN"] ?? "ruby-like",
    symsLang: process.env["WAST_SYMS"] ?? "en",
  };

  const positionals: string[] = [];

  for (const arg of argv) {
    if (arg === "--help") {
      options.help = true;
    } else if (arg === "--json") {
      options.json = true;
    } else if (arg === "--include-caller") {
      options.includeCaller = true;
    } else if (arg === "--dry-run") {
      options.dryRun = true;
    } else if (arg.startsWith("--")) {
      errorExit(`Unknown option: ${arg}`, options.json);
    } else {
      positionals.push(arg);
    }
  }

  const command = positionals.length > 0 ? positionals[0] : null;
  const rest = positionals.slice(1);

  return { command, positionals: rest, options };
}

// ── Help ──

const HELP_TEXT = `wast - WAST component CLI

Usage: wast <command> [options]

Commands:
  bindgen <dir>                    Generate wast.db from world.wit
  extract <dir> <uid...>           Extract partial component as text
  merge <dir>                      Merge text from stdin into wast.db
  fmt                              Format/validate wast text from stdin
  diff <dir-a> <dir-b>             Diff two components
  syms <dir> <uid> <name>          Set display name in syms file
  setup-git                        Configure git diff driver

Options:
  --help                           Show help
  --json                           Machine-readable JSON output
  --include-caller                 Include callers (extract only)
  --dry-run                        Validate only (merge only)

Environment:
  WAST_PLUGIN                      Syntax plugin (default: ruby-like)
  WAST_SYMS                        Syms language (default: en)
`;

// ── Error helpers ──

export function errorExit(message: string, json: boolean): never {
  if (json) {
    console.log(JSON.stringify({ ok: false, errors: [{ code: "user_error", message }] }));
  } else {
    console.error(`error: ${message}`);
  }
  process.exit(1);
}

export function notImplemented(command: string, json: boolean): never {
  if (json) {
    console.log(JSON.stringify({ ok: false, command, errors: [{ code: "not_implemented" }] }));
  } else {
    console.error("not yet implemented");
  }
  process.exit(2);
}

export function requireArgs(positionals: string[], count: number, usage: string, json: boolean): void {
  if (positionals.length < count) {
    errorExit(`missing required arguments. Usage: ${usage}`, json);
  }
}

export function requireDir(dir: string, json: boolean): string {
  const resolved = resolve(dir);
  if (!existsSync(resolved)) {
    errorExit(`directory does not exist: ${dir}`, json);
  }
  return resolved;
}

export function requireFile(path: string, label: string, json: boolean): void {
  if (!existsSync(path)) {
    errorExit(`${label} not found: ${path}`, json);
  }
}

// ── Main ──

const { command, positionals, options } = parseArgs(process.argv.slice(2));

if (options.help || command === null) {
  if (options.json) {
    console.log(JSON.stringify({ ok: true, help: HELP_TEXT.trim() }));
  } else {
    console.log(HELP_TEXT.trim());
  }
  process.exit(0);
}

async function main() {
  switch (command) {
    case "bindgen":
      await bindgen(positionals, options);
      break;
    case "extract":
      extract(positionals, options);
      break;
    case "merge":
      merge(positionals, options);
      break;
    case "fmt":
      fmt(positionals, options);
      break;
    case "diff":
      diff(positionals, options);
      break;
    case "syms":
      syms(positionals, options);
      break;
    case "setup-git":
      setupGit(positionals, options);
      break;
    default:
      errorExit(`unknown command: ${command}`, options.json);
  }
}
main();
