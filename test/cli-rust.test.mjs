import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const cliBin = process.env.WAST_CLI_BIN ?? resolve(rootDir, 'target/debug/wast-rs');

function runCli(args, options = {}) {
  return spawnSync(cliBin, args, {
    encoding: 'utf8',
    ...options,
  });
}

function assertSuccess(result) {
  assert.equal(result.status, 0, result.stderr || result.stdout);
}

test('wast-rs help prints available commands', () => {
  const result = runCli(['--help']);

  assertSuccess(result);
  assert.match(result.stdout, /bindgen/);
  assert.match(result.stdout, /setup-git/);
});

test('wast-rs syms writes syms.en.yaml in target directory', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'wast-cli-syms-'));
  const result = runCli(['syms', dir, 'func_123', 'DisplayName']);

  assertSuccess(result);
  const symsText = readFileSync(resolve(dir, 'syms.en.yaml'), 'utf8');
  assert.match(symsText, /func_123 = "DisplayName"/);
});

test('wast-rs setup-git configures temp repository', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'wast-cli-git-'));
  mkdirSync(resolve(dir, '.git'));

  const init = spawnSync('git', ['init'], { cwd: dir, encoding: 'utf8' });
  assert.equal(init.status, 0, init.stderr || init.stdout);

  const result = runCli(['setup-git'], { cwd: dir });

  assertSuccess(result);
  const attrs = readFileSync(resolve(dir, '.gitattributes'), 'utf8');
  assert.match(attrs, /wast\.db diff=wast/);

  const config = spawnSync('git', ['config', '--get', 'diff.wast.command'], {
    cwd: dir,
    encoding: 'utf8',
  });
  assert.equal(config.status, 0, config.stderr || config.stdout);
  assert.equal(config.stdout.trim(), 'wast diff');
});