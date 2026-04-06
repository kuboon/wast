import { join } from "node:path";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import type { GlobalOptions } from "../index.js";
import { requireArgs, requireDir } from "../index.js";
import { parseSyms, serializeSyms, classifyUid } from "../syms-io.js";

export function syms(positionals: string[], options: GlobalOptions): void {
  requireArgs(positionals, 3, "wast syms <component-dir> <uid> <display-name>", options.json);
  const dir = requireDir(positionals[0], options.json);
  const uid = positionals[1];
  const displayName = positionals[2];

  const symsFile = join(dir, `syms.${options.symsLang}.yaml`);

  // Load existing or start fresh
  let data;
  if (existsSync(symsFile)) {
    data = parseSyms(readFileSync(symsFile, "utf-8"));
  } else {
    data = { wit: new Map<string, string>(), internal: new Map<string, string>(), local: new Map<string, string>() };
  }

  const section = classifyUid(uid);
  data[section].set(uid, displayName);

  writeFileSync(symsFile, serializeSyms(data));

  if (options.json) {
    console.log(JSON.stringify({ ok: true, command: "syms", uid, section, name: displayName }));
  } else {
    console.log(`${section}/${uid} = "${displayName}" -> ${symsFile}`);
  }
}
