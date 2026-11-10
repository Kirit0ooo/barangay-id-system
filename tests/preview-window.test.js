// Single card-preview window reuse — regression for previewResidentCard.
// Before the fix, every call opened a new popup via window.open('', '_blank'),
// even when re-previewing the same resident. The fix caches this.previewWindow
// and reuses it unless it has been closed.
//
// Loads the real app source (helpers → app.core → app.detail) in a Function
// scope with browser globals stubbed, so tests run against the actual source
// and stay dependency-free (no jsdom, no network, no Tauri).
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');

// Evaluate helpers.js → app.core.js → app.detail.js together so module-scope
// dependencies (PUROK_OPTIONS, window.appState) resolve in order. Returns
// window.appState with previewResidentCard bound to it.
function loadAppState() {
  const helpers = readFileSync(join(root, 'frontend/src/js/shared/helpers.js'), 'utf8');
  const appCore = readFileSync(join(root, 'frontend/src/js/app/app.core.js'), 'utf8');
  const appDetail = readFileSync(join(root, 'frontend/src/js/app/residents/app.detail.js'), 'utf8');

  const body = [
    'const window = globalThis;',
    // Minimal DOM stub — only read at call time inside detail methods.
    'globalThis.document = globalThis.document || { documentElement: { dataset: {} }, querySelectorAll: () => [], createElement: () => ({}) };',
    'const Logger = { info() {}, warn() {}, error() {} };',
    'const API = {};',
    helpers,
    appCore,
    appDetail,
    'return window.appState;',
  ].join('\n');

  return new Function(body)();
}

function fakeWindow(closed = false) {
  const writes = [];
  const styleAppends = [];
  return {
    closed,
    writes,
    styleAppends,
    document: {
      open: () => {},
      write: (html) => { writes.push(html); },
      close: () => {},
      createElement: () => ({ textContent: '' }),
      head: { appendChild: (el) => { styleAppends.push(el.textContent); } },
    },
    focus: () => {},
    __writes: () => writes,
    __styleAppends: () => styleAppends,
  };
}

describe('previewResidentCard single-window reuse', () => {
  it('opens exactly one window and reuses it across multiple previews', async () => {
    const appState = loadAppState();
    const win = fakeWindow(false);

    let openCount = 0;
    globalThis.open = () => { openCount += 1; return win; };

    appState.cardHtmlForResident = async (id) => `<html>card:${id}</html>`;
    appState.showToast = () => {};
    appState.printFilledCard = () => {};

    await appState.previewResidentCard('R1');
    await appState.previewResidentCard('R2');

    assert.equal(openCount, 1, 'window.open called only once');
    assert.equal(appState.previewWindow, win, 'previewWindow cached and reused');
    assert.equal(win.__writes().length, 2, 'both cards rendered into the same window');
    assert.equal(win.__styleAppends().length, 2, 'print-matching style injected each time');
  });

  it('reopens a fresh window when the cached one has been closed', async () => {
    const appState = loadAppState();
    const win = fakeWindow(true); // already closed

    let openCount = 0;
    globalThis.open = () => { openCount += 1; return win; };

    appState.cardHtmlForResident = async (id) => `<html>card:${id}</html>`;
    appState.showToast = () => {};
    appState.printFilledCard = () => {};

    await appState.previewResidentCard('R1');
    await appState.previewResidentCard('R2');

    assert.equal(openCount, 2, 'closed window reopened on next preview');
    assert.equal(appState.previewWindow, win, 'previewWindow refreshed');
  });

  it('falls back to direct print when the popup is blocked', async () => {
    const appState = loadAppState();

    let openCount = 0;
    globalThis.open = () => { openCount += 1; return null; }; // popup blocked

    let printed = null;
    appState.cardHtmlForResident = async (id) => `<html>card:${id}</html>`;
    appState.showToast = () => {};
    appState.printFilledCard = (html) => { printed = html; };

    await appState.previewResidentCard('R1');

    assert.equal(openCount, 1, 'attempted to open once');
    assert.equal(appState.previewWindow, null, 'no window cached when blocked');
    assert.equal(printed, '<html>card:R1</html>', 'fell back to printFilledCard with the filled HTML');
  });
});
