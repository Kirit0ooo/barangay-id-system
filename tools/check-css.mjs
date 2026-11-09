#!/usr/bin/env node
// CSS freshness guard: fails when the Alpine/JS markup changes but the
// prebuilt static CSS was not reviewed. Replace for the deleted
// tmp/gen_tailwind.py (P2-10). --update re-stamps the fingerprint.
// JS class-emitting files are derived at runtime (src/js/*.js, vendored
// libs excluded) so a newly added module cannot silently fall out of the
// hash and leave check:css permanently green.
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, readdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const stamp = 'tools/css-fingerprint.txt';
const jsDir = join(root, 'src', 'js');
const vendorName = 'vendor';

// Collect JS sources: everything under src/js except vendored libs,
// plus index.html (the class consumer).
const sources = ['src/index.html'];
try {
  for (const name of readdirSync(jsDir).sort()) {
    if (name === vendorName || name.endsWith('.min.js')) continue;
    if (name.endsWith('.js')) sources.push(join('src', 'js', name));
  }
} catch {
  // fall back to the original pair if src/js is unavailable
  sources.push('src/js/app.js');
}

const hash = createHash('sha256');
for (const f of sources) {
  hash.update(readFileSync(join(root, f)));
}
const current = hash.digest('hex');

if (process.argv.includes('--update')) {
  writeFileSync(join(root, stamp), current + '\n');
  console.log('css-fingerprint updated — remember to review src/css/tailwind.css');
} else if (current === readFileSync(join(root, stamp), 'utf8').trim()) {
  console.log('CSS fresh: markup unchanged since last stamp');
} else {
  console.error('STALE CSS: src/index.html or a src/js module changed since last stamp.');
  console.error('Review src/css/tailwind.css + src/css/style.css for missing utility classes,');
  console.error('then re-stamp with: node tools/check-css.mjs --update');
  process.exit(1);
}
