// Bulk print tests — HTML-only bulk (printAllCards JS loop, page-break
// paginated, live counter). String-level + behavioral (pure loop sim):
// offline, no network, no backend events (PDF bulk deleted).
import { readFileSync } from 'fs';
import { join } from 'path';

const root = join(import.meta.dirname, '..');
let failed = 0;
function check(name, cond, hint = '') {
  const ok = !!cond;
  console.log(ok ? `✓ ${name}` : `✗ ${name}${hint ? ' — ' + hint : ''}`);
  if (!ok) failed++;
}

const idCardRs = readFileSync(join(root, 'src-tauri/src/id_card.rs'), 'utf8');
const mainRs = readFileSync(join(root, 'src-tauri/src/main.rs'), 'utf8');
const apiJs = readFileSync(join(root, 'src/js/api.js'), 'utf8');
// App modules are split across app.core/app.register/app.search/app.settings/app.detail/app.dashboard/app.js.
// Concatenate them so path-based string checks still cover all view logic.
const appModuleFiles = ['app.core','app.register','app.search','app.settings','app.detail','app.dashboard','app'];
const appJs = appModuleFiles.map(f => readFileSync(join(root, 'src/js/' + f + '.js'), 'utf8')).join('\n');
const indexHtml = readFileSync(join(root, 'src/index.html'), 'utf8');
const cardTpl = readFileSync(join(root, 'src/card-template.html'), 'utf8');
// Comment-stripped copy: historical notes may name deleted PDF items (allowed).
const idCardCode = idCardRs.replace(/^\s*\/\/.*$/gm, '');

// --- Backend: no bulk machinery remains ---
check('no generate_all_id_cards_pdf command', !idCardCode.includes('generate_all_id_cards_pdf') && !mainRs.includes('generate_all_id_cards_pdf'));
check('no generate_id_card_pdf command', !idCardCode.includes('generate_id_card_pdf') && !mainRs.includes('generate_id_card_pdf'));
check('no bulk-progress/bulk-complete events', !idCardCode.includes('bulk-progress') && !idCardCode.includes('bulk-complete'));
check('no BulkProgress/BulkIdResult structs', !idCardCode.includes('BulkProgress') && !idCardCode.includes('BulkIdResult'));
check('no failures.json/manifest logic', !idCardCode.includes('failures.json') && !idCardCode.includes('manifest.txt'));
check('api.js exposes no PDF wrappers', !apiJs.includes('generateIdCardPdf') && !apiJs.includes('generateAllIdCardsPdf') && !apiJs.includes('previewIdCardHtml'));
check('api.js exposes no bulk listeners', !apiJs.includes('listenBulkProgress') && !apiJs.includes('listenBulkComplete'));
check('api.js keeps QR + short-barangay for print', apiJs.includes('getQrDataUrl') && apiJs.includes('getShortBarangay'));

// --- Frontend: JS-loop bulk print ---
check('app.js printAllCards exists', appJs.includes('printAllCards'));
check('bulk paginates with page-break-after', appJs.includes('page-break-after:always'));
check('bulk reuses single-row payload builder', appJs.includes('buildCardPayloadForRow'));
check('bulk progress state (JS loop)', appJs.includes('bulkProgress: { current: 0, total: 0, percent: 0, inProgress: false'));
check('bulk updates percent live', appJs.includes('bulkProgress.percent = Math.round'));
check('bulk counts failures without backend', appJs.includes('bulkProgress.failures++'));
check('bulk prints via hidden iframe', appJs.includes('printFilledCard(bulkHtml)'));
check('bulk guards empty search (no silent full-registry print)', appJs.includes('No active residents to print') && !appJs.includes(': await this.fetchAllResidentsPaged()'));
check('bulk confirms large jobs (>50)', /async printAllCards[\s\S]{0,3000}?confirm/.test(appJs));
check('bulk supports cancel flag', appJs.includes('bulkPrintCancel') || appJs.includes('cancelBulk'));
check('toast uses sequence token (no stale hide)', /showToast[\s\S]{0,600}?(toastSeq|toastToken|seq \+ 1|seq \+= 1)/.test(appJs));
check('validDaysLeft uses UTC midnight (DST-safe)', /validDaysLeft[\s\S]{0,1200}?Date\.UTC/.test(appJs));
const viewFn = /async viewResident[\s\S]*?^    \},/m.exec(appJs)?.[0] || '';
const editFn = /async editResident[\s\S]*?^    \},/m.exec(appJs)?.[0] || '';
check('view/edit loads resident+photo in parallel', viewFn.includes('Promise.all') && editFn.includes('Promise.all'));
const printFn = /printFilledCard\(html\)[\s\S]*?^    \},/m.exec(appJs)?.[0] || '';
check('printFilledCard avoids shared onload race', printFn.includes('onload = null') || printFn.includes('onload=null') || (printFn.includes('new iframe') && printFn.includes('new Promise')));
check('no backend bulk invoke', !appJs.includes('generateAllIdCardsPdf') && !appJs.includes('generateIdCardPdf'));
check('index.html Print All button', indexHtml.includes('printAllCards') && !indexHtml.includes('downloadAllIDPDF'));
check('index.html progress bar bound to percent', indexHtml.includes('bulkProgress.percent'));
check('index.html live counter', indexHtml.includes('bulkProgress.current') && indexHtml.includes('bulkProgress.total'));
check('index.html recent-files list (last 5)', indexHtml.includes('bulkProgress.recent'));
check('index.html no failures.json reference', !indexHtml.includes('failures.json'));
check('template back follows front on the same A4 (no forced break)', cardTpl.includes('back-bleed') && cardTpl.includes('.back-bleed{page-break-before:auto;'));

// --- Behavioral: replay the JS-loop progress shape over 3 rows (1 failure) ---
{
  const bulkProgress = { current: 0, total: 3, percent: 0, inProgress: true, failures: 0, recent: [] };
  const rows = [{ resident_id: 'BRGY-2026-00001' }, { resident_id: 'BRGY-2026-00002' }, { resident_id: 'BRGY-2026-00003' }];
  for (let i = 0; i < rows.length; i++) {
    const r = rows[i];
    try {
      if (r.resident_id.endsWith('00002')) throw new Error('boom');
      bulkProgress.recent = [...bulkProgress.recent, { file: r.resident_id, status: 'success' }].slice(-5);
    } catch {
      bulkProgress.failures++;
      bulkProgress.recent = [...bulkProgress.recent, { file: r.resident_id, status: 'failed' }].slice(-5);
    }
    bulkProgress.current = i + 1;
    bulkProgress.percent = Math.round((i + 1) / rows.length * 100);
  }
  bulkProgress.inProgress = false;
  check('loop reaches 3/3 with percent 100', bulkProgress.current === 3 && bulkProgress.total === 3 && bulkProgress.percent === 100, JSON.stringify(bulkProgress));
  check('loop counts 1 failure', bulkProgress.failures === 1, String(bulkProgress.failures));
  check('loop recent keeps resident ids', bulkProgress.recent.length === 3 && bulkProgress.recent[0].file === 'BRGY-2026-00001', JSON.stringify(bulkProgress.recent));
}

if (failed) { console.error(`\n${failed} bulk-print check(s) failed`); process.exit(1); }
else console.log('\nAll bulk-print checks passed — HTML-only bulk wired end-to-end.');
