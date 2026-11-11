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
    'globalThis.document = globalThis.document || { documentElement: { dataset: {} }, querySelectorAll: () => [], createElement: () => ({}), body: { classList: { add: () => {} } } };',
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
  const classAdds = [];
  const sheetStyle = {};
  const sheetEl = { style: sheetStyle, scrollWidth: 1000 };
  const elements = new Map();
  return {
    closed,
    writes,
    classAdds,
    sheetStyle,
    sheetEl,
    elements,
    document: {
      open: () => {},
      write: (html) => { writes.push(html); },
      close: () => {},
      createElement: () => ({ textContent: '' }),
      documentElement: { clientWidth: 800 },
      querySelector: (sel) => sel === '.sheet' ? sheetEl : null,
      getElementById: (id) => {
        if (!elements.has(id)) {
          const listeners = [];
          elements.set(id, { listeners, addEventListener: (evt, fn) => { if (evt === 'click') { listeners.push(fn); } } });
        }
        return elements.get(id);
      },
      body: { classList: { add: (cls) => { classAdds.push(cls); } } },
    },
    focus: () => {},
    __writes: () => writes,
    __classAdds: () => classAdds,
  };
}

describe('previewResidentCard single-window reuse', () => {
  it('opens exactly one window and reuses it across multiple previews', async () => {
    const appState = loadAppState();
    const win = fakeWindow(false);

    let openCount = 0;
    let openedUrl = null;
    globalThis.open = (url) => { openCount += 1; openedUrl = url; return win; };

    appState.cardHtmlForResident = async (id) => `<html>card:${id}</html>`;
    appState.showToast = () => {};
    appState.printFilledCard = () => {};

    await appState.previewResidentCard('R1');
    await appState.previewResidentCard('R2');

    assert.equal(openCount, 1, 'window.open called only once');
    assert.equal(openedUrl, 'about:blank', 'window.open uses about:blank target');
    assert.equal(appState.previewWindow, win, 'previewWindow cached and reused');
    assert.equal(win.__writes().length, 2, 'both cards rendered into the same window');
    assert.equal(win.__classAdds().length, 2, 'preview class added each time');
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

describe('__applyZoom', () => {
  it('resets transform to empty string for 100% mode', () => {
    const appState = loadAppState();
    const win = fakeWindow(false);

    appState.__applyZoom(win, 1);

    assert.equal(win.sheetStyle.transform, '', '100% mode clears the transform');
  });

  it('scales to fit width for fit mode', () => {
    const appState = loadAppState();
    const win = fakeWindow(false);

    appState.__applyZoom(win, 0);

    assert.ok(win.sheetStyle.transform.startsWith('scale('), 'fit mode applies a scale transform');
    const scale = parseFloat(win.sheetStyle.transform.replace('scale(', '').replace(')', ''));
    assert.ok(scale > 0 && scale <= 1, 'scale is between 0 and 1');
    assert.equal(win.sheetStyle.transformOrigin, 'top center', 'transform origin is top center');
  });

  it('returns early without error when no sheet element exists', () => {
    const appState = loadAppState();
    const win = fakeWindow(false);
    win.document.querySelector = () => null;

    appState.__applyZoom(win, 0);
    appState.__applyZoom(win, 1);
  });
});

describe('preview toolbar button wiring', () => {
  it('wires print, zoom, and close buttons to their actions', async () => {
    const appState = loadAppState();
    const win = fakeWindow(false);

    let openCount = 0;
    globalThis.open = () => { openCount += 1; return win; };

    let printed = false;
    let closed = false;
    let zoomed100 = false;
    let zoomedFit = false;

    win.print = () => { printed = true; };
    win.close = () => { closed = true; };
    const originalApplyZoom = appState.__applyZoom;
    appState.__applyZoom = (_w, mode) => {
      if (mode === 1) { zoomed100 = true; }
      if (mode === 0) { zoomedFit = true; }
    };

    appState.cardHtmlForResident = async () => '<html></html>';
    appState.showToast = () => {};
    appState.printFilledCard = () => {};

    await appState.previewResidentCard('R1');

    // Call listeners while the __applyZoom override is still in place
    const printEl = win.document.getElementById('toolbar-print');
    printEl.listeners.forEach((fn) => fn.call(appState));
    assert.equal(printed, true, 'print button triggers window.print');

    const zoom100El = win.document.getElementById('toolbar-zoom-100');
    zoom100El.listeners.forEach((fn) => fn.call(appState));
    assert.equal(zoomed100, true, '100% zoom button calls __applyZoom with mode 1');

    const zoomFitEl = win.document.getElementById('toolbar-zoom-fit');
    zoomFitEl.listeners.forEach((fn) => fn.call(appState));
    assert.equal(zoomedFit, true, 'fit width button calls __applyZoom with mode 0');

    const closeEl = win.document.getElementById('toolbar-close');
    closeEl.listeners.forEach((fn) => fn.call(appState));
    assert.equal(closed, true, 'close button calls window.close');

    appState.__applyZoom = originalApplyZoom;
  });
});
