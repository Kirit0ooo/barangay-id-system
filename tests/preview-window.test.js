// In-app card preview modal — regression for previewResidentCard.
// Before the fix, every call opened a new popup via window.open('', '_blank').
// The fix replaces the popup with an in-app modal with an iframe.
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

function fakeIframe() {
  const writes = [];
  return {
    contentDocument: {
      open: () => {},
      write: (html) => { writes.push(html); },
      close: () => {},
    },
    contentWindow: {
      print: () => {},
    },
    __writes: () => writes,
  };
}

describe('previewResidentCard in-app modal', () => {
  it('opens the modal and renders card into iframe', async () => {
    const appState = loadAppState();
    const iframe = fakeIframe();

    // Stub $refs and $nextTick
    appState.$refs = { previewIframe: iframe };
    appState.$nextTick = (fn) => fn();

    let toasted = null;
    appState.cardHtmlForResident = async (id) => `<html>card:${id}</html>`;
    appState.showToast = (msg) => { toasted = msg; };

    await appState.previewResidentCard('R1');

    assert.equal(appState.previewOpen, true, 'previewOpen is true');
    assert.equal(appState.previewResident, undefined, 'previewResident is undefined when resident not found');
    assert.equal(appState.previewZoom, 100, 'previewZoom reset to 100');
    assert.equal(iframe.__writes().length, 1, 'card HTML written to iframe');
    assert.ok(iframe.__writes()[0].includes('card:R1'), 'correct card content in iframe');
    assert.equal(toasted, null, 'no toast shown for successful preview');
  });

  it('sets previewResident when resident found in searchResults', async () => {
    const appState = loadAppState();
    const iframe = fakeIframe();

    appState.$refs = { previewIframe: iframe };
    appState.$nextTick = (fn) => fn();
    appState.searchResults = [{ id: 'R1', resident_id: 'BRGY-2026-00001', last_name: 'Doe', first_name: 'John', suffix: '' }];

    appState.cardHtmlForResident = async (id) => `<html>card:${id}</html>`;
    appState.showToast = () => {};

    await appState.previewResidentCard('R1');

    assert.equal(appState.previewResident && appState.previewResident.id, 'R1', 'previewResident set from searchResults');
  });

  it('shows error toast when cardHtmlForResident fails', async () => {
    const appState = loadAppState();

    appState.cardHtmlForResident = async () => { throw new Error('fetch failed'); };
    let toasted = null;
    appState.showToast = (msg, type) => { toasted = { msg, type }; };

    await appState.previewResidentCard('R1');

    assert.ok(toasted, 'toast shown on error');
    assert.equal(toasted.type, 'error', 'toast type is error');
    assert.ok(toasted.msg.includes('Card preview failed'), 'toast message includes error');
  });
});

describe('preview modal controls', () => {
  it('closePreview resets all preview state', () => {
    const appState = loadAppState();
    appState.previewOpen = true;
    appState.previewResident = { id: 'R1' };
    appState.previewZoom = 150;

    appState.closePreview();

    assert.equal(appState.previewOpen, false, 'previewOpen is false');
    assert.equal(appState.previewResident, null, 'previewResident is null');
    assert.equal(appState.previewZoom, 100, 'previewZoom reset to 100');
  });

  it('previewZoomIn increases zoom up to 200', () => {
    const appState = loadAppState();
    appState.previewZoom = 100;
    appState.previewZoomIn();
    assert.equal(appState.previewZoom, 125, 'zoom in by 25');

    appState.previewZoom = 200;
    appState.previewZoomIn();
    assert.equal(appState.previewZoom, 200, 'capped at 200');
  });

  it('previewZoomOut decreases zoom down to 50', () => {
    const appState = loadAppState();
    appState.previewZoom = 100;
    appState.previewZoomOut();
    assert.equal(appState.previewZoom, 75, 'zoom out by 25');

    appState.previewZoom = 50;
    appState.previewZoomOut();
    assert.equal(appState.previewZoom, 50, 'capped at 50');
  });

  it('previewZoomReset sets zoom to 100', () => {
    const appState = loadAppState();
    appState.previewZoom = 175;
    appState.previewZoomReset();
    assert.equal(appState.previewZoom, 100, 'zoom reset to 100');
  });

  it('printPreviewCard calls iframe.contentWindow.print', () => {
    const appState = loadAppState();
    let printed = false;
    const iframe = {
      contentWindow: {
        print: () => { printed = true; },
      },
    };
    appState.$refs = { previewIframe: iframe };

    appState.printPreviewCard();

    assert.equal(printed, true, 'iframe.contentWindow.print called');
  });

  it('printPreviewCard does nothing when iframe not ready', () => {
    const appState = loadAppState();
    appState.$refs = { previewIframe: null };

    // Should not throw
    appState.printPreviewCard();
    assert.ok(true, 'no error when iframe not ready');
  });
});

describe('__applyZoom (kept for backward compatibility)', () => {
  it('resets transform to empty string for 100% mode', () => {
    const appState = loadAppState();
    const win = {
      sheetStyle: {},
      sheetEl: { style: {}, scrollWidth: 1000 },
      document: {
        querySelector: (sel) => sel === '.sheet' ? { style: win.sheetStyle, scrollWidth: 1000 } : null,
        documentElement: { clientWidth: 800 },
      },
    };

    appState.__applyZoom(win, 1);

    assert.equal(win.sheetStyle.transform, '', '100% mode clears the transform');
  });

  it('scales to fit width for fit mode', () => {
    const appState = loadAppState();
    const sheetStyle = {};
    const win = {
      sheetStyle,
      sheetEl: { style: sheetStyle, scrollWidth: 1000 },
      document: {
        querySelector: (sel) => sel === '.sheet' ? { style: sheetStyle, scrollWidth: 1000 } : null,
        documentElement: { clientWidth: 800 },
      },
    };

    appState.__applyZoom(win, 0);

    assert.ok(sheetStyle.transform.startsWith('scale('), 'fit mode applies a scale transform');
    const scale = parseFloat(sheetStyle.transform.replace('scale(', '').replace(')', ''));
    assert.ok(scale > 0 && scale <= 1, 'scale is between 0 and 1');
    assert.equal(sheetStyle.transformOrigin, 'top center', 'transform origin is top center');
  });

  it('returns early without error when no sheet element exists', () => {
    const appState = loadAppState();
    const win = {
      document: {
        querySelector: () => null,
      },
    };

    appState.__applyZoom(win, 0);
    appState.__applyZoom(win, 1);
    assert.ok(true, 'no error when sheet not found');
  });
});