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
const jsDir = join(root, 'frontend', 'src', 'js');
const vendorName = 'vendor';

// Collect JS sources: everything under frontend/src/js except vendored libs,
// plus index.html (the class consumer). Recurse so nested feature folders
// (app/, services/, shared/, bootstrap/) are included.
const collectJs = (dir, rel) => {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return;
  }
  for (const entry of entries) {
    const relPath = rel ? join(rel, entry.name) : entry.name;
    if (entry.isDirectory()) {
      if (entry.name === vendorName) {
        continue;
      }
      collectJs(join(dir, entry.name), relPath);
    } else if (entry.name.endsWith('.js') && !entry.name.endsWith('.min.js')) {
      sources.push(join('frontend', 'src', 'js', relPath));
    }
  }
};

const sources = ['frontend/src/index.html'];
collectJs(jsDir, '');
sources.sort();
if (sources.length === 1) {
  // fall back to the original pair if frontend/src/js is unavailable
  sources.push('frontend/src/js/app/app.js');
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
