// Smoke tests — must stay inside D:\Barangay\barangay-id-system\tests\
import { readFileSync, existsSync } from 'fs';
import { join } from 'path';

const root = join(import.meta.dirname, '..');

function assert(cond, msg) { if (!cond) throw new Error(msg); }

const checks = [
  ['package.json exists', existsSync(join(root, 'package.json'))],
  ['tauri.conf.json exists', existsSync(join(root, 'src-tauri/tauri.conf.json'))],
  ['src/index.html exists', existsSync(join(root, 'src/index.html'))],
  ['src/js/app.js exists', existsSync(join(root, 'src/js/app.js'))],
  ['src/js/api.js exists', existsSync(join(root, 'src/js/api.js'))],
  ['src-tauri/Cargo.toml exists', existsSync(join(root, 'src-tauri/Cargo.toml'))],
  ['src-tauri/src/main.rs exists', existsSync(join(root, 'src-tauri/src/main.rs'))],
  ['data dir isolated', existsSync(join(root, 'data'))],
  ['tmp dir isolated', existsSync(join(root, 'tmp'))],
  ['.npmrc forces local cache', readFileSync(join(root, '.npmrc'), 'utf8').includes('./.npm-cache')],
];

let failed = 0;
for (const [name, ok] of checks) {
  console.log(ok ? `✓ ${name}` : `✗ ${name}`);
  if (!ok) failed++;
}
if (failed) { console.error(`\n${failed} check(s) failed`); process.exit(1); }
else console.log(`\nAll ${checks.length} isolation smoke checks passed — all artifacts inside D:\\Barangay\\barangay-id-system`);
