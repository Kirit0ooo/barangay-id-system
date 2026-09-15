#!/usr/bin/env node
// CSS freshness guard: fails when the Alpine/JS markup changes but the
// prebuilt static CSS was not reviewed. Replace for the deleted
// tmp/gen_tailwind.py (P2-10). --update re-stamps the fingerprint.
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const stamp = 'tools/css-fingerprint.txt';
const hash = createHash('sha256');
for (const f of ['src/index.html', 'src/js/app.js']) {
  hash.update(readFileSync(join(root, f)));
}
const current = hash.digest('hex');

if (process.argv.includes('--update')) {
  writeFileSync(join(root, stamp), current + '\n');
  console.log('css-fingerprint updated — remember to review src/css/tailwind.css');
} else if (current === readFileSync(join(root, stamp), 'utf8').trim()) {
  console.log('CSS fresh: markup unchanged since last stamp');
} else {
  console.error('STALE CSS: src/index.html or src/js/app.js changed since last stamp.');
  console.error('Review src/css/tailwind.css + src/css/style.css for missing utility classes,');
  console.error('then re-stamp with: node tools/check-css.mjs --update');
  process.exit(1);
}
