// Structural DOM test — view roots must be siblings under <main> inside #app.
// Regression for the invisible-detail bug: an unclosed register-hero div
// nested search/settings/detail inside the register view (and a stray close
// ejected detail+toast outside #app), so detail rendered blank while the
// backend happily returned the row. Uses a tiny stack parser (void elements
// + Alpine <template> aware); no external deps so `npm test` stays offline.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const html = readFileSync(join(root, 'src/index.html'), 'utf8');
const styleCss = readFileSync(join(root, 'src/css/style.css'), 'utf8');

const VOID = new Set(['meta', 'img', 'input', 'br', 'hr', 'link', 'source', 'circle', 'path', 'rect', 'use', 'polygon']);

// Minimal HTML5-ish stack parse: track open elements with attributes so we
// can query ancestor chains like a browser DOM would nest them.
function parseStack() {
  const stack = []; // {tag, attrs}
  const views = []; // {view, depth, ancestors}
  const re = /<(!--|\/?)([a-zA-Z0-9]+)?([^>]*?)>/g;
  let m;
  while ((m = re.exec(html))) {
    const [full, bang, rawTag, rest] = m;
    if (bang === '!--') continue; // comment
    const tag = (rawTag || '').toLowerCase();
    if (!tag) continue;
    if (bang === '/') {
      if (tag === 'template') {
        const i = stack.map(e => e.tag).lastIndexOf('template');
        if (i !== -1) stack.splice(i, 1);
        continue;
      }
      if (VOID.has(tag)) continue;
      const i = stack.map(e => e.tag).lastIndexOf(tag);
      if (i !== -1) stack.splice(i, 1); // close merges like browsers for balanced trees
      continue;
    }
    const selfClose = /\/\s*$/.test(rest);
    if (tag === 'template' || (!VOID.has(tag) && !selfClose)) {
      const attrs = {};
      const am = /x-show\s*=\s*"([^"]*)"/.exec(rest);
      if (am) attrs['x-show'] = am[1];
      const cm = /class\s*=\s*"([^"]*)"/.exec(rest);
      if (cm) attrs['class'] = cm[1];
      const im = /\bid\s*=\s*"([^"]*)"/.exec(rest);
      if (im) attrs['id'] = im[1];
      stack.push({ tag, attrs });
      const xs = attrs['x-show'] || '';
      const vm = /currentView === '(\w+)'/.exec(xs);
      if (tag === 'div' && vm && !/ml-auto/.test(attrs['class'] || '')) {
        views.push({ view: vm[1], ancestors: stack.slice(0, -1).map(e => e.tag + (e.attrs.id ? '#' + e.attrs.id : '')) });
      }
    }
  }
  return { stack, views };
}

describe('view-root nesting', () => {
  it('all five views are direct children of main inside #app', () => {
    const { stack, views } = parseStack();
    assert.equal(stack.length, 0, `unbalanced tags remain: ${JSON.stringify(stack.slice(-3))}`);
    const byView = {};
    for (const v of views) {
      if (!byView[v.view]) byView[v.view] = v.ancestors;
    }
    for (const name of ['dashboard', 'register', 'search', 'settings', 'detail']) {
      const chain = byView[name];
      assert.ok(chain, `${name} view root not found`);
      assert.deepEqual(
        chain.slice(-2),
        ['div#app', 'main#main-content'],
        `${name} must sit directly under main#main-content (got ${chain.join('>')})`
      );
      assert.ok(chain.includes('div#app'), `${name} must be inside #app (Alpine scope)`);
      assert.ok(chain.includes('main#main-content'), `${name} must be inside <main>`);
    }
  });

  it('toast lives inside #app so Alpine bindings evaluate', () => {
    const { views } = parseStack();
    assert.ok(views.length >= 5, 'expected at least 5 view roots');
    // toast checked via raw nesting: find toast div ancestors
    const stack = [];
    const re = /<(!--|\/?)([a-zA-Z0-9]+)?([^>]*?)>/g;
    let m, toastChain = null;
    while ((m = re.exec(html))) {
      const [full, bang, rawTag, rest] = m;
      if (bang === '!--') continue;
      const tag = (rawTag || '').toLowerCase();
      if (!tag) continue;
      if (bang === '/') {
        if (VOID.has(tag)) continue;
        const i = stack.map(e => e.tag).lastIndexOf(tag);
        if (i !== -1) stack.splice(i, 1);
        continue;
      }
      if (/\/\s*$/.test(rest) || VOID.has(tag)) continue;
      const am = /x-show\s*=\s*"([^"]*)"/.exec(rest);
      const cm = /class\s*=\s*"([^"]*)"/.exec(rest);
      const im = /\bid\s*=\s*"([^"]*)"/.exec(rest);
      stack.push({ tag, attrs: { 'x-show': am?.[1] || '', class: cm?.[1] || '', id: im?.[1] || '' } });
      if (tag === 'div' && /\btoast\b/.test(cm?.[1] || '')) {
        toastChain = stack.slice(0, -1).map(e => e.tag + (e.attrs.id ? '#' + e.attrs.id : ''));
      }
    }
    assert.ok(toastChain, 'toast div not found');
    assert.ok(toastChain.includes('div#app'), 'toast must be inside #app');
  });

  it('app backdrop is paper token, not amber-neon red (C1)', () => {
    const bodies = [...styleCss.matchAll(/^body\s*\{([^}]*)\}/gm)].map(m => m[1]);
    assert.ok(bodies.length >= 1, 'no body rule found in style.css');
    const last = bodies[bodies.length - 1];
    assert.match(last, /var\(--paper\)/, 'final body background must use --paper token');
    assert.doesNotMatch(last, /--amber-neon/, 'final body background must not use --amber-neon red');
  });
});


describe('theme toggle invariants', () => {
  it('theme has exactly one writer (applyTheme)', () => {
    // App modules are split across app.core/app.register/app.search/app.settings/app.detail/app.dashboard/app.js.
    // Concatenate them so path-based string checks still cover all view logic.
    const appModuleFiles = ['app.core','app.register','app.search','app.settings','app.detail','app.dashboard','app'];
    const appJs = appModuleFiles.map(f => readFileSync(join(root, 'src/js/' + f + '.js'), 'utf8')).join('\n');
    const writes = appJs.split('this.theme =').length - appJs.split('this.theme ===').length;
    assert.equal(writes, 1, 'this.theme must have exactly one writer (applyTheme)');
  });

  it('applyTheme() runs before the backend await in init()', () => {
    // App modules are split across app.core/app.register/app.search/app.settings/app.detail/app.dashboard/app.js.
    const appModuleFiles = ['app.core','app.register','app.search','app.settings','app.detail','app.dashboard','app'];
    const appJs = appModuleFiles.map(f => readFileSync(join(root, 'src/js/' + f + '.js'), 'utf8')).join('\n');
    const iTheme = appJs.indexOf('this.applyTheme();');
    const iCivic = appJs.indexOf('await this.loadCivicConfig();');
    assert.ok(iTheme !== -1 && iCivic !== -1 && iTheme < iCivic,
      'applyTheme() must run before the first backend await in init()');
  });

  it('toggle checkbox uses :checked comparison not x-model', () => {
    const lbl = html.indexOf('aria-label="Red gradient theme"');
    const tagStart = html.lastIndexOf('<input', lbl);
    const tag = html.slice(tagStart, html.indexOf('>', lbl));
    assert.ok(tag.includes(`:checked="theme === 'red-gradient'"`),
      'toggle checkbox must read theme via :checked comparison');

    assert.ok(!tag.includes('x-model'),
      'toggle checkbox must not bind theme via x-model (boolean coercion)');
  });

  it('applyTheme clears data-theme so paper stays reachable', () => {
    // App modules are split across app.core/app.register/app.search/app.settings/app.detail/app.dashboard/app.js.
    const appModuleFiles = ['app.core','app.register','app.search','app.settings','app.detail','app.dashboard','app'];
    const appJs = appModuleFiles.map(f => readFileSync(join(root, 'src/js/' + f + '.js'), 'utf8')).join('\n');
    assert.ok(appJs.includes('delete document.documentElement.dataset.theme'),
      'applyTheme must clear data-theme, otherwise the theme toggle is one-way');
  });

  it('toggle row uses items-center not items-end', () => {
    assert.ok(html.includes('<div class="flex gap-2 items-center">'),
      'toggle row must use backed items-center class');
    const rowStart = html.indexOf('<div class="flex gap-2 items-center">');
    const rowEnd = html.indexOf('aria-label="Red gradient theme"');
    const row = html.slice(rowStart, rowEnd);
    assert.ok(!row.includes('items-end'), 'toggle row must not use items-end');
  });

  it('style.css ships opt-in red-gradient body rule with font bump', () => {
    assert.ok(styleCss.includes('html[data-theme="red-gradient"]'),
      'style.css must declare the red-gradient theme selector');
    assert.ok(styleCss.includes('background-attachment:fixed'),
      'red-gradient rule must pin background-attachment:fixed');
    assert.ok(styleCss.includes('font-size: 12px'),
      'help text must be bumped 11->12px for legibility');
  });
});