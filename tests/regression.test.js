// Regression tests for repaired HIGH findings — must stay inside D:\Barangay\barangay-id-system\tests\
import { readFileSync, existsSync } from 'fs';
import { join } from 'path';

const root = join(import.meta.dirname, '..');
let failed = 0;
function check(name, cond, hint='') {
  const ok = !!cond;
  console.log(ok ? `✓ ${name}` : `✗ ${name}${hint ? ' — '+hint : ''}`);
  if (!ok) failed++;
}

const indexHtml = readFileSync(join(root, 'src/index.html'), 'utf8');
const tauriConf = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf8'));
const cargoToml = readFileSync(join(root, 'src-tauri/Cargo.toml'), 'utf8');
const mainRs = readFileSync(join(root, 'src-tauri/src/main.rs'), 'utf8');
const residentsRs = readFileSync(join(root, 'src-tauri/src/residents.rs'), 'utf8');
const backupRs = readFileSync(join(root, 'src-tauri/src/backup.rs'), 'utf8');
const idCardRs = readFileSync(join(root, 'src-tauri/src/id_card.rs'), 'utf8');
const exportRs = readFileSync(join(root, 'src-tauri/src/export.rs'), 'utf8');
const configRs = readFileSync(join(root, 'src-tauri/src/config.rs'), 'utf8');
const scheduledRs = readFileSync(join(root, 'src-tauri/src/scheduled.rs'), 'utf8');
const loggerRs = readFileSync(join(root, 'src-tauri/src/logger.rs'), 'utf8');
const pkgJson = readFileSync(join(root, 'package.json'), 'utf8');
let cssGuard = '';
try {
  cssGuard = readFileSync(join(root, 'tools/check-css.mjs'), 'utf8');
} catch {
  // guard file moved/renamed — check below will fail gracefully instead of
  // aborting the whole module and masking every check below it.
}
const cardTpl = readFileSync(join(root, 'src/card-template.html'), 'utf8');
const buildPs1 = readFileSync(join(root, 'build.ps1'), 'utf8');
const runPs1 = readFileSync(join(root, 'run.ps1'), 'utf8');
const runBat = readFileSync(join(root, 'run.bat'), 'utf8');
const buildBat = readFileSync(join(root, 'build.bat'), 'utf8');
const viewLogsBat = readFileSync(join(root, 'view-logs.bat'), 'utf8');
// Whitespace-collapsed copy: cargo fmt may wrap chained calls across lines.
const idCardFlat = idCardRs.replace(/\s+/g, ' ');
// Comment-stripped copy: historical notes may name deleted PDF items (allowed).
const idCardCode = idCardRs.replace(/^\s*\/\/.*$/gm, '');
const cargoCode = cargoToml.replace(/^\s*#.*$/gm, '');

// 1. Detail view null guard — must use template x-if, routed view replaces modal
check('detail modal uses template x-if guard', indexHtml.includes('<template x-if="detailResident">'), 'expected <template x-if="detailResident">');
check('no remaining inner x-show selectedResident', !indexHtml.includes('<div class="p-6" x-show="selectedResident">'), 'still has x-show null hazard');

// 2. Purok filter alignment — search and register must share same options AVOCADO etc
const registerPuroks = ['AVOCADO','RIMAS','PASAYAN','BUBUNAO','ASLUM'];
for (const p of registerPuroks) {
  check(`register purok ${p} present`, indexHtml.includes(`<option>${p}</option>`));
}
check('search purok no longer uses Purok 1', !indexHtml.includes('<option>Purok 1</option>'), 'still has Purok 1');
for (const p of registerPuroks) {
  // search section should contain same set after fix
  check(`search purok ${p} present`, indexHtml.includes(`<option>${p}</option>`));
}

// 3. Tauri allowlist hardening — isolation: scope must be tight, no DOWNLOAD/DOCUMENT/DESKTOP leak
check('fs scope not wildcard **', !JSON.stringify(tauriConf.tauri.allowlist.fs.scope).includes('"**"'), JSON.stringify(tauriConf.tauri.allowlist.fs.scope));
check('fs.all is false', tauriConf.tauri.allowlist.fs.all === false, String(tauriConf.tauri.allowlist.fs.all));
check('csp not null', tauriConf.tauri.security.csp !== null && tauriConf.tauri.security.csp.length > 10, String(tauriConf.tauri.security.csp));
check('fs scope tight ($APP/$RESOURCE only)', JSON.stringify(tauriConf.tauri.allowlist.fs.scope).includes('$APP') && JSON.stringify(tauriConf.tauri.allowlist.fs.scope).includes('$RESOURCE'), JSON.stringify(tauriConf.tauri.allowlist.fs.scope));
check('fs scope does not leak DOWNLOAD/DOCUMENT/DESKTOP/TEMP', !JSON.stringify(tauriConf.tauri.allowlist.fs.scope).includes('$DOWNLOAD') && !JSON.stringify(tauriConf.tauri.allowlist.fs.scope).includes('$DOCUMENT') && !JSON.stringify(tauriConf.tauri.allowlist.fs.scope).includes('$DESKTOP'), JSON.stringify(tauriConf.tauri.allowlist.fs.scope));

// 4. ID generation — must use MAX suffix, not COUNT
check('generate_next_id uses MAX suffix', residentsRs.includes('MAX(CAST(substr(resident_id, -5) AS INTEGER))'), 'still uses COUNT or missing MAX');
check('COUNT(*) no longer used for ID generation', !residentsRs.includes("SELECT COUNT(*) FROM residents WHERE strftime"), 'COUNT pattern still present');
check('validate_resident_input exists', residentsRs.includes('fn validate_resident_input'), 'validation not added');
check('orphan photo cleanup on insert failure', residentsRs.includes('Clean up orphan photo'), 'missing orphan cleanup');
check('photo size guard present', residentsRs.includes('Photo too large'), 'missing size guard');
check('explicit resident_id collision errors instead of silent reassign (C2)', residentsRs.includes('explicit_id'), 'explicit ID still silently cleared on collision');

// 5. Backup — must use wal_checkpoint + isolated validation, no string-interpolated VACUUM INTO
check('backup no longer uses string-interpolated VACUUM INTO', !backupRs.includes("VACUUM INTO '"), 'still has VACUUM INTO string interpolation (injection risk)');
check('backup uses wal_checkpoint', backupRs.includes('wal_checkpoint'), 'missing wal_checkpoint');
check('backup validates isolated write path', backupRs.includes('validate_isolated_write_path'), 'missing isolation validation');

// 6. ID card QR hardening (HTML-only print: no PDF draw fns remain)
check('qr_png_base64 no longer unwraps', !idCardRs.includes('QrCode::new(data.as_bytes()).unwrap()'), 'still unwraps QrCode');
check('qr_png fallback present', idCardRs.includes('1x1 transparent PNG placeholder') || idCardRs.includes('iVBORw0KGgo'), 'fallback missing');
check('no PDF draw fns remain', !idCardCode.includes('fn draw_front') && !idCardCode.includes('fn draw_back') && !idCardCode.includes('fn generate_single_pdf'), 'PDF render path still present');
check('no printpdf dep', !cargoCode.includes('printpdf'), 'printpdf still in Cargo.toml');
check('no generate_*_pdf commands', !idCardCode.includes('generate_id_card_pdf') && !idCardCode.includes('generate_all_id_cards_pdf') && !idCardCode.includes('preview_id_card_html'), 'PDF commands still present');
check('main unregisters PDF commands', !mainRs.includes('generate_id_card_pdf') && !mainRs.includes('generate_all_id_cards_pdf') && !mainRs.includes('preview_id_card_html'), 'main still registers PDF commands');

// 7. Smoke test path corrected
const smoke = readFileSync(join(root, 'tests/smoke.test.js'), 'utf8');
check('smoke test checks src-tauri/tauri.conf.json', smoke.includes("src-tauri/tauri.conf.json"), 'smoke still checks root path');

// 8. Audit P0 — gender stats case-insensitive, search hides soft-deleted, QR command
check('dashboard stats use UPPER(gender)', residentsRs.includes("UPPER(gender) = 'MALE'") && residentsRs.includes("UPPER(gender) = 'FEMALE'"), 'stats still query Male/Female literals');
check('search filters soft-deleted by default', /\? = 1 OR is_active = 1/.test(residentsRs), 'search still leaks is_active=0');
check('search matches digit-stripped contacts (P1-2)', residentsRs.includes("REPLACE(contact_number, '-', '')") && residentsRs.includes('digits_search'), 'formatted-number search missing');
check('get_qr_data_url command exists', idCardRs.includes('pub async fn get_qr_data_url'), 'QR still regex-scraped only');
check('main registers get_qr_data_url', readFileSync(join(root, 'src-tauri/src/main.rs'), 'utf8').includes('id_card::get_qr_data_url'), 'command not registered');
const apiJs = readFileSync(join(root, 'src/js/api.js'), 'utf8');
check('api.js wraps getQrDataUrl', apiJs.includes('getQrDataUrl'), 'frontend still scrapes QR');
// App modules are split across app.core/app.register/app.search/app.settings/app.detail/app.dashboard/app.js.
// Concatenate them so path-based string checks still cover all view logic.
const appModuleFiles = ['app.core','app.register','app.search','app.settings','app.detail','app.dashboard','app'];
const appJs = appModuleFiles.map(f => readFileSync(join(root, 'src/js/' + f + '.js'), 'utf8')).join('\n');
check('buildCardPayload calls getQrDataUrl', appJs.includes('API.getQrDataUrl(residentId)'), 'buildCardPayload still regex-first');
check('shortBarangay BALUD-substring hack gone', !appJs.includes("if (s.includes('BALUD')) return 'BALUD'"), 'hack still present');
check('update deletes old photo only after commit', residentsRs.includes('commit won'), 'photo-loss reorder missing');
check('update reads old photo from DB', residentsRs.includes('SELECT photo_filename FROM residents WHERE id = ?'), 'old photo still from stale input');

// 9. Audit P1 — ISO valid_until storage, server short_barangay, bulk dir reuse
check('valid_until stored ISO', residentsRs.includes('vu.format("%Y-%m-%d")'), 'still stores DD-MM-YYYY');
check('normalize_valid_until exists', residentsRs.includes('fn normalize_valid_until'), 'no validity normalizer');
check('db migrates legacy DD-MM-YYYY validity', readFileSync(join(root, 'src-tauri/src/db.rs'), 'utf8').includes("GLOB '??-??-????'"), 'migration missing');
check('get_short_barangay command exists', readFileSync(join(root, 'src-tauri/src/config.rs'), 'utf8').includes('pub async fn get_short_barangay'), 'no server short_barangay');
check('no bulk PDF machinery', !idCardCode.includes('BulkIdResult') && !idCardCode.includes('bulk-progress') && !idCardCode.includes('failures.json') && !idCardCode.includes('BulkProgress'), 'bulk PDF structs/events still present');
check('bulk print is JS-loop (printAllCards)', appJs.includes('printAllCards') && appJs.includes('page-break-after'), 'no JS bulk-print loop');
check('no duplicate FRONT/BACK template dupes', !existsSync(join(root, 'FRONT.png')) && !existsSync(join(root, 'src/FRONT.png')) && !existsSync(join(root, 'src/assets/FRONT.png')), 'dupes still present');
check('no _cspNote in tauri.conf.json', !JSON.stringify(tauriConf).includes('_cspNote'), 'custom _cspNote field present');
check('birthdate input keeps only :max binding', indexHtml.includes(':max="todayISO()"') && !indexHtml.includes('max="9999-12-31"'), 'redundant static max still present');
check('package.json test script wires smoke+regression', JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).scripts.test.includes('tests/smoke') && JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).scripts.test.includes('tests/regression'), 'npm test not wired');
check('CI workflow exists', existsSync(join(root, '.github/workflows/ci.yml')), 'ci.yml missing');
check('tmp/.gitignore ignores everything', readFileSync(join(root, 'tmp/.gitignore'), 'utf8').split('\n')[0].trim() === '*', 'tmp gitignore still lists node_modules/target');
check('saveResident captures savedId (no ReferenceError)', appJs.includes('const savedId = await API.createResident') && appJs.includes("id=' + savedId"), 'savedId not captured correctly');
check('generate_next_id has ROLLBACK on error', residentsRs.includes('ROLLBACK') && residentsRs.includes('BEGIN IMMEDIATE'), 'missing ROLLBACK guard');
check('export write_excel ensures parent dir', exportRs.includes('create_dir_all(parent)'), 'parent dir not ensured');
check('civic.json size limit exists', configRs.includes('MAX_CIVIC_JSON'), 'no size guard on civic.json');

// 10. barangay-impl-002 — M1 CSP tightening (prebuilt static CSS, no Play CDN)
const dbRs = readFileSync(join(root, 'src-tauri/src/db.rs'), 'utf8');
const tailwindCss = readFileSync(join(root, 'src/css/tailwind.css'), 'utf8');
check('Play CDN runtime unlinked', !indexHtml.includes('vendor/tailwindcss.js'), 'tailwindcss.js script still linked');
check('prebuilt static CSS linked', indexHtml.includes('css/tailwind.css'), 'tailwind.css link missing');
check('prebuilt CSS has utilities', tailwindCss.includes('.bg-white') && tailwindCss.includes('@media(min-width:640px)'), 'tailwind.css looks empty');
check('CSP drops unsafe-eval', !tauriConf.tauri.security.csp.includes('unsafe-eval'), 'unsafe-eval still in CSP');
// Alpine.js evaluates x-attributes via `new Function` (1 site in alpine.min.js),
// so script-src keeps 'unsafe-inline'; style-src keeps 'unsafe-inline' for the
// <style> blocks in index.html/card-template.html. No inline <script> exists.
check('CSP keeps unsafe-inline for Alpine', tauriConf.tauri.security.csp.includes("'unsafe-inline'") && !indexHtml.match(/<script(?![^>]*src=)/), 'unsafe-inline rationale broken');

// 11. barangay-impl-002 — M3 MSI perMachine robustness (no panic, writability probe)
check('writability probe exists', dbRs.includes('fn dir_is_writable'), 'no dir_is_writable');
check('exe-dir fallback gated on writability', dbRs.includes('if dir_is_writable(dir)'), 'exe dir not gated');
check('no isolation panic in db.rs', !dbRs.includes('panic!("Isolation violation'), 'panic still present');
check('init_db maps open errors user-facing', dbRs.includes('Cannot open database at'), 'no graceful DB error');

// 12. barangay-impl-002 — L4 address_house probe runs once (migration flag)
check('boot-flags gate exists', dbRs.includes('_boot_flags') && dbRs.includes('address_house_drop'), 'no migration flag');

// 13. barangay-impl-002 — L2 export-stem shim finalized (canonical name holds impl)
check('export stem canonical in db.rs', dbRs.includes('pub fn per_resident_export_stem') && /pub fn per_resident_export_stem[\s\S]*?sanitize_file_name_component/.test(dbRs), 'impl not in canonical name');
check('legacy pdf_filename delegates', /pub fn per_resident_pdf_filename[\s\S]*?per_resident_export_stem\(full_name/.test(dbRs), 'legacy name still holds impl');

// 14. barangay-impl-002 — L3 asset dedupe (canonical src/assets only)
check('root image dupes removed', !existsSync(join(root, 'BALUD.png')) && !existsSync(join(root, 'bagong_pilipinas.png')) && !existsSync(join(root, 'Boy.jpg')) && !existsSync(join(root, 'Girl.jpg')), 'root dupes still present');
check('unreferenced logos/ removed', !existsSync(join(root, 'src-tauri/assets/logos')), 'logos/ still present');
check('canonical src/assets kept', existsSync(join(root, 'src/assets/BALUD.png')) && existsSync(join(root, 'src/assets/bagong-pilipinas.png')) && existsSync(join(root, 'src/assets/Boy.jpg')) && existsSync(join(root, 'src/assets/Girl.jpg')), 'canonical assets missing');
check('backend fallback assets kept', existsSync(join(root, 'src-tauri/assets/fallback/Boy.jpg')) && existsSync(join(root, 'src-tauri/assets/fallback/Girl.jpg')), 'fallback assets missing');
check('project marker kept', existsSync(join(root, 'barangay.ico')), 'barangay.ico marker removed');

// 15. barangay-impl-002 — L1 validity single-sourced (backend truth)
check('backend valid_until_display is truth', readFileSync(join(root, 'src-tauri/src/id_card.rs'), 'utf8').includes('L1 single source of truth'), 'no single-source marker');
check('frontend mirrors backend validity', appJs.includes('L1') && appJs.includes('id_card.rs::valid_until_display'), 'frontend not marked as mirror');

// 16. repair — M-1 create path never holds the Mutex across its write txn
check('create clones handles before txn (M-1)', residentsRs.includes('never hold the') && residentsRs.includes('let (pool, app_data_dir) = {'), 'create still holds Mutex guard across txn');
check('create loop uses cloned pool, not state guard', !/let state = state\.lock\(\)\.await;\n\n    \/\/ R2/.test(residentsRs), 'whole-guard lock still present in create');

// 17. search tab auto-lists 20 alphabetical residents per page with pager
check('search tab auto-lists on open', indexHtml.includes("@click=\"searchResidents(); currentView='search'\"") && indexHtml.includes('x-for="resident in pagedResults()"'), 'search still waits for typing');
check('pager helpers exist (20 per page, reset on new search)', appJs.includes('searchPerPage: 20') && appJs.includes('pagedResults()') && appJs.includes('totalSearchPages()') && appJs.includes('this.searchPage = 1;'), 'pager logic missing');
check('pager bar rendered when many residents', indexHtml.includes('totalSearchPages() > 1') && indexHtml.includes('prevSearchPage()') && indexHtml.includes('nextSearchPage()') && indexHtml.includes('searchRangeText()'), 'pager markup missing');

// 18. emergency contact address — full chain (form → model → DB → card → Excel)
check('emergency address input on register form', indexHtml.includes('form.emergency_contact_address') && indexHtml.includes('sm:col-span-2'), 'form input missing');
check('emergency address in form model + card payload prefers explicit', appJs.includes('emergency_contact_address: \'\'') && appJs.includes('emerExplicit'), 'model/payload missing');
check('emergency address stored + migrated', residentsRs.includes('pub emergency_contact_address: Option<String>') && dbRs.includes('ADD COLUMN emergency_contact_address TEXT'), 'struct/migration missing');
check('emergency address in Excel round-trip (37 cols)', exportRs.includes('"Emergency Address"') && exportRs.includes('EXPECTED_COLS: usize = 37'), 'export/import missing');

// 19. failed save never wipes typed progress (snapshot + restore)
check('save snapshots form before mutating', appJs.includes('const formSnapshot = { ...this.form };'), 'no pre-save snapshot');
check('save error restores snapshot', appJs.includes('this.form = formSnapshot;'), 'error path does not restore form');

// 21. edit mode renders the stored photo; removal clears persisted filename too
check('edit loads stored photo outside payload', appJs.includes('editPhotoUrl') && appJs.includes('API.getResidentPhoto(id)'), 'edit photo not loaded');
check('preview prefers upload, falls back to stored photo', indexHtml.includes(':src="form.photoData || editPhotoUrl"'), 'preview binding wrong');
check('remove clears upload, stored name, and preview', indexHtml.includes("form.photoData = ''; form.photo_filename = ''; editPhotoUrl = ''"), 'remove leaves persisted photo');

// 20. resident detail modal polish (balanced columns, status, emergency block)
check('modal shows status pill beside purok', indexHtml.includes('statusPillClass(detailResident)') && indexHtml.includes('statusLabel(detailResident)'), 'detail status pill missing');
check('modal right column carries emergency contact block', indexHtml.includes('emergency_contact_name || \'N/A\'') && indexHtml.includes('Contact Address:') && indexHtml.includes('emergency_contact_address'), 'modal emergency block missing');

// 22. no fallback photo anywhere — only the resident's actual upload renders
check('backend photo fallback removed', !residentsRs.includes('fallback_photo_bytes') && !residentsRs.includes('Girl.jpg'), 'fallback helper still present');
check('frontend photo fallbacks removed', !appJs.includes('assets/Girl.jpg') && !appJs.includes('assets/Boy.jpg'), 'frontend still falls back');
check('card photo slot hides when empty', cardTpl.includes('data-field="photo" src=""') && cardTpl.includes('.photo-box img[src=""]{visibility:hidden;}'), 'photo default not neutralized');

// 23. edit never blanks selects: option values match UPPERCASE storage
check('gender/civil/suffix options carry storage values', indexHtml.includes('<option value="MALE">Male</option>') && indexHtml.includes('<option value="SINGLE">Single</option>') && indexHtml.includes('<option value="JR.">Jr.</option>'), 'select values still mismatch storage');
check('edit coerces legacy enum casing', appJs.includes("for (const k of ['gender', 'civil_status', 'suffix'])"), 'legacy rows can still render blank');
check('modal photo has single alt binding', indexHtml.includes(':alt="(detailResident ? detailResident.last_name') && !indexHtml.includes('alt="Resident photo" :alt='), 'duplicate alt on detail photo');

// 24. barangay-fix-001 — P1/P2 hardening pins
check('import normalizes before validate (P1-1)', exportRs.includes('normalize_resident_strings(&mut base)'), 'import bypasses normalization');
check('bulk import preloads residents (P2-7)', exportRs.includes('bulk_cache'), 'import still N+1 SELECTs');
check('bulk print pages full table (P1-3)', appJs.includes('fetchAllResidentsPaged'), 'bulk print still caps at 5000');
check('bulk print hoists civic + batches photo/QR (P1-4)', appJs.includes('getCardsBatch') && idCardRs.includes('pub async fn get_cards_batch') && apiJs.includes('getCardsBatch'), 'bulk print still O(N) IPC');
check('backup/safety/log retention exists (P2-6)', backupRs.includes('prune_oldest_files') && scheduledRs.includes('prune_oldest_files') && loggerRs.includes('prune_run_logs'), 'unbounded growth unpruned');
check('resolve_unique_path errors on exhaustion (P2-8)', dbRs.includes('refusing to overwrite'), 'collision fall-through still overwrites');
check('get_logs_dir returns Result (P2-9)', loggerRs.includes('pub fn get_logs_dir() -> Result<PathBuf, String>'), 'panic path still present');
check('CSS freshness check wired (P2-10)', pkgJson.includes('check:css'), 'no stale-CSS guard');
// Note: this pin matches local identifiers jsDir / vendorName / '.min.js' inside
// tools/check-css.mjs. A legitimate refactor that renames those locals trips this
// check as a false red — update the spelling here rather than restoring the old
// hardcoded sources = ['src/index.html','src/js/app.js'] pair.
check('check-css derives JS sources at runtime', cssGuard.includes('readdirSync(jsDir)') && cssGuard.includes('name === vendorName') && cssGuard.includes('.min.js'), 'guard missing or reverted to hardcoded sources');
check('update ignores echoed photo filename (P2-12)', residentsRs.includes('never trust the echoed client filename'), 'stale echo can still overwrite photo');
check('todayISO uses local time (P2-14)', appJs.includes('getFullYear') && !/function todayISO\(\)\s*\{[^}]*toISOString/.test(appJs), 'todayISO still UTC');

// 25. barangay-enhance-001 — duplicate detection, DB optimize, logic tests, photo budget
check('duplicate-person gate on create+update', residentsRs.includes('Possible duplicate') && residentsRs.includes('AND id != ?'), 'name+DOB dup check missing');
check('optimize_database fn exists (PRAGMA optimize + incremental vacuum)', dbRs.includes('pub async fn optimize_database') && dbRs.includes('PRAGMA optimize') && dbRs.includes('incremental_vacuum(100)'), 'optimize fn missing');
check('optimize sentinel tracks 7-day cadence', dbRs.includes('.last_optimize') && mainRs.includes('db::optimize_database_cmd'), 'sentinel/command not wired');
check('api.js wraps optimizeDatabase', apiJs.includes('optimizeDatabase'), 'frontend wrapper missing');
check('photo ingest has 10s timeout budget', residentsRs.includes('process_photo_bounded') && residentsRs.includes('from_secs(10)') && residentsRs.includes('timed out'), 'photo timeout missing');
check('helpers.js extracted + logic tests exist', existsSync(join(root, 'src/js/helpers.js')) && existsSync(join(root, 'tests/logic.test.js')) && indexHtml.includes('js/helpers.js'), 'helpers/logic tests missing');

// 26. barangay-fix-short-barangay-001 — paren-first shortBarangay + card favicon
check('frontend shortBarangay extracts parens first', appJs.includes('match(/\\(([^)]*)\\)/)') && appJs.includes("if (!tokens.length) return 'BALUD'"), 'paren-first extract missing in app.js');
check('frontend shortBarangay keeps last-token fallback after parens', (appJs.match(/return 'BALUD'/g) || []).length >= 2, 'fallback BARANGAY guard missing');
check('backend short_barangay extracts parens first', configRs.includes("find('(')") && configRs.includes("find(')')") && configRs.includes('take(40)'), 'paren-first extract missing in config.rs');
check('backend short_barangay keeps full-string fallback', configRs.includes("b.to_uppercase()") && configRs.includes('chars().take(40)'), 'fallback missing in config.rs');
check('card template has no dead favicon link (fetched-as-text, srcdoc-rendered)', !cardTpl.includes('<link rel="icon"'), 'dead favicon link still present in card-template.html');

// 27. repair — null guards on async-loaded template state (photoScanResult)
check('photoScanResult panel uses template x-if outer guard', indexHtml.includes('<template x-if="photoScanResult">'), 'outer should be template x-if, not x-show');
check('photoScanResult inner orphan guard null-safe', indexHtml.includes('photoScanResult && photoScanResult.orphan_count'), 'inner x-if must guard null before .orphan_count');
check('no bare photoScanResult deref outside template', !/x-show="photoScanResult"/.test(indexHtml) || indexHtml.includes('<template x-if="photoScanResult">'), 'x-show on nullable still present');

// 28. detail open/close logic — routed view (not modal/new window)
check('detail opens as exclusive routed view', indexHtml.includes(`x-show="currentView === 'detail'"`), 'detail must be exclusive currentView branch');
check('detail Edit goes to register (no closeDetail bounce)', !indexHtml.includes(`editResident(detailResident.id); closeDetail()`), 'Edit must not call closeDetail after editResident');
check('detail dismisses with Esc', indexHtml.includes(`@keydown.escape.window`), 'Esc-to-close missing on app root');

// 29. paper backdrop — titles stay readable, surfaces stay light (C1: red backdrop retired)
const styleCss = readFileSync(join(root, 'src/css/style.css'), 'utf8');
check('paper backdrop token in use', styleCss.includes('background:var(--paper);') && !/body\s*\{\s*background:var\(--amber-neon\)/.test(styleCss), 'backdrop must be paper, not amber-neon red');
check('all five view titles use view-title', (indexHtml.match(/view-title"/g) || []).length >= 5, 'every view h2 needs view-title');
check('content surfaces stay light', styleCss.includes('.stat-card{\n  background:var(--color-surface);') && styleCss.includes('.form-section{\n  background:var(--color-surface);'), 'cards/stats/forms must stay white surfaces');

// 30. contact auto-dash mask — 11 digits, 0990-123-4567, submit gate
const helpersJs = readFileSync(join(root, 'src/js/helpers.js'), 'utf8');
check('maskContactPH helper exists', helpersJs.includes('function maskContactPH('), 'mask helper missing');
check('contact inputs use mask handler', indexHtml.includes(`onContactInput($event, 'contact_number')`) && indexHtml.includes(`onContactInput($event, 'emergency_contact_number')`), 'inputs not wired to mask');
check('contact inputs capped by maxlength', indexHtml.includes('id="inp-contact"') && indexHtml.includes('maxlength="13"'), 'maxlength cap missing');
check('save enforces 11-digit mobiles', appJs.includes('mustBeMobile'), 'submit gate missing');

// 31. dashboard status cards + gender pie (male/female ratio cards retired)
check('status cards present (active/renew/lost/damage)', ['Active cards', 'Renew cards', 'Lost cards', 'Damage cards'].every(s => indexHtml.includes(s)), 'status card missing');

check('old ratio cards retired', !indexHtml.includes('stat-card--male') && !indexHtml.includes('stat-card--female'), 'old male/female cards still present');
check('backend serves status counts', residentsRs.includes('pub active: i64') && residentsRs.includes("UPPER(remarks) = 'RENEW'") && residentsRs.includes("UPPER(remarks) = 'LOST'") && residentsRs.includes("UPPER(remarks) = 'DAMAGE'"), 'status queries missing');

check('primary stat dominant (34px)', styleCss.includes('.stat-card--primary .stat-card__value{ color:var(--color-text); font-size:34px; }'), 'primary dominance missing');
check('root clips horizontal overflow (full-bleed sticky nav)', styleCss.includes('html,body{ overflow-x:clip; }'), 'nav can scroll sideways');
check('uniform 40px buttons, label-fit widths', styleCss.includes('min-height:40px;') && !indexHtml.includes('min-height: 100px') && indexHtml.includes('action-chip'), 'button heights vary');
check('edit mode hides Clear Form, shows Back (new keeps Clear)', indexHtml.includes('x-show="!editingResident" @click="clearForm"') && indexHtml.includes('x-show="editingResident" @click="goBackFromEdit()"'), 'edit/new buttons not gated');
check('edit mode has Back only (Cancel is new-only)', indexHtml.includes('x-show="!editingResident" @click="clearForm"') && !appJs.includes('async revertEdit()') && appJs.includes('async goBackFromEdit()'), 'edit-mode buttons wrong');
check('detail hero shows ID expiry below pills', indexHtml.includes('detail-view-validity') && indexHtml.includes('expiryCountdown(detailResident)') && appJs.includes('expiryCountdown('), 'expiry line missing');


// 36. balance-gauge replaces the donut (new visual language)


check('donut layers retired', !indexHtml.includes('pieGradMale') && !indexHtml.includes('url(#pieSoft)') && !indexHtml.includes('opacity="0.65"'), 'donut remnants still present');
check('donut helpers retired', !appJs.includes('pieSpan()') && !appJs.includes('pieOff()') && !appJs.includes('pieBoth()'), 'donut helpers still present');

// 32. per-user MSI (writable one-folder install + automatic desktop shortcut)
const wixTmpl = readFileSync(join(root, 'src-tauri/wix/main.wxs'), 'utf8');
check('MSI installs per-user (no admin, writable dir)', wixTmpl.includes('InstallScope="perUser"') && wixTmpl.includes('InstallPrivileges="limited"') && !wixTmpl.includes('InstallScope="perMachine"'), 'MSI still perMachine');
check('MSI data dir is per-user LocalAppData (not Program Files)', wixTmpl.includes('LocalAppDataFolder') && !wixTmpl.includes('$(var.PlatformProgramFilesFolder)" Name="PFiles"'), 'install dir still Program Files');
check('MSI creates desktop shortcut automatically', wixTmpl.includes('<Directory Id="DesktopFolder"') && wixTmpl.includes('Id="ApplicationShortcutDesktop"') && wixTmpl.includes('<ComponentRef Id="ApplicationShortcutDesktop"'), 'desktop shortcut missing');
check('tauri.conf wires custom WiX template', tauriConf.tauri.bundle.windows.wix.template === './wix/main.wxs', 'wix.template not wired');
check('WebView2 embedded offline (no runtime download)', tauriConf.tauri.bundle.windows.webviewInstallMode.type === 'offlineInstaller', 'still bootstrapper/download mode');

// 33. P0 fixes: single backupDB, truthful search-cap toast, shippable MSI, verify removed
const searchCap = Number((residentsRs.match(/pub const SEARCH_RESULT_LIMIT:\s*i64\s*=\s*(\d+)/) || [])[1] || 0);
check('backupDB defined once (scheduled-aware wins)', (appJs.match(/async backupDB\(\)/g) || []).length === 1, 'duplicate backupDB still present');
check('search cap centralized and matches backend', (() => { const m = appJs.match(/const SEARCH_RESULT_CAP\s*=\s*(\d+)/); const feCap = m ? Number(m[1]) : 0; return searchCap > 0 && feCap === searchCap && (appJs.match(/SEARCH_RESULT_CAP/g) || []).length >= 3; })(), 'frontend cap missing, duplicated, or != SEARCH_RESULT_LIMIT');
check('frontend contact gate mirrors backend (PH-strict + legacy-charset +63)', appJs.includes('+63') && appJs.includes('7') && appJs.includes('mirrors backend'), 'frontend still blanket-rejects legacy numbers the backend accepts');
check('bundle ships MSI (WiX per-user hardening is real)', tauriConf.tauri.bundle.targets.includes('msi'), 'targets still nsis-only');
check('verify module removed', !mainRs.includes('mod verify;'), 'mod verify still present');
check('verify commands unregistered', !mainRs.includes('verify::get_verify_qr_data_url') && !mainRs.includes('verify::get_verify_cards_batch') && !mainRs.includes('verify::export_verify_registry') && !mainRs.includes('publish_verify_trust'), 'verify commands still in invoke_handler');
check('verify key infra removed from config', !configRs.includes('VerifyTrust') && !configRs.includes('ensure_verify_keypair') && !configRs.includes('verify_base_url'), 'verify remnants in config.rs');
check('verify crypto deps pruned', !cargoToml.includes('ed25519-dalek') && !cargoToml.includes('sha2'), 'unused crypto deps remain');

// 34. Build dependency gate: every release-build tool probed fail-fast (future deps = one $Prereqs row)
check('build gate probes MSVC linker', buildPs1.includes('link.exe') && buildPs1.includes('vswhere'), 'cargo would fail at link after a long compile');
check('build gate probes NSIS', buildPs1.includes('makensis'), 'nsis leg unchecked');
check('build gate is table-driven for future deps', buildPs1.includes('$Prereqs') && buildPs1.includes("FUTURE deps are a one-row addition"), 'gate not extensible');
check('all launchers ASCII-only (PS 5.1 codepage-safe)', [runPs1, buildPs1, runBat, buildBat, viewLogsBat].every(s => !/[^\x00-\x7F]/.test(s)), 'non-ASCII broke PS 5.1 parsing once');

// 35. run.ps1 dev gate mirrors build gate (dev compiles Rust too; no bundling tools needed)
check('run.ps1 gate probes MSVC linker', runPs1.includes('link.exe'), 'tauri dev would fail at link after compiling');
check('Node floor aligned at 20+ in both launchers', runPs1.includes('LTS 20+') && buildPs1.includes('20'), 'run says 18+, build requires 20+');
check('bat wrappers forward flags to ps1', runBat.includes('%*') && buildBat.includes('%*'), 'flags silently dropped by wrapper');

// 38. Release hygiene: stale bundle outputs removed pre-build, patch version auto-increments
check('build.ps1 cleans stale bundle dir before build', buildPs1.includes('Remove-Item') && buildPs1.includes('release\\bundle') && buildPs1.includes('NoClean'), 'no bundle clean step');
check('build.ps1 auto-increments patch version across manifests', buildPs1.includes('NoBump') && buildPs1.includes('Auto-version') && buildPs1.includes('Cargo.lock') && buildPs1.includes('tauri.conf.json'), 'no auto-version step');
check('build.bat documents auto-version + clean (flags forwarded)', buildBat.includes('%*') && (buildBat.includes('NoBump') || buildBat.includes('auto-increment')), 'build.bat not updated');

// 39. Verify trust publish removed with the portal (no unreachable UI)
check('api.js has no publishVerifyTrust wrapper', !apiJs.includes('publishVerifyTrust'), 'dead IPC wrapper remains');
check('settings has no publish-verify-trust action', !appJs.includes('async publishTrust()') && !indexHtml.includes('@click="publishTrust"'), 'dead publish UI remains');
check('settings has no verify-URL field', !appJs.includes('verifyBaseUrl') && !indexHtml.includes('verifyBaseUrl'), 'dead verify URL field remains');
// Legacy QR restored: id_card.rs mints the plain 7-key JSON again
const goldenQr = readFileSync(join(root, 'tests/golden-card.test.js'), 'utf8');
const qrReadme = readFileSync(join(root, 'gathered-md/QR_PII_README.md'), 'utf8');
check('legacy qr_data() restored in id_card', idCardRs.includes('fn qr_data('), 'signed-URL minter still in place');
check('QR doc prescribes qr_data() again', qrReadme.includes('qr_data()') && !qrReadme.includes('mint_verify_url'), 'doc still points at removed minter');

// 40. Task3: update txn + prod print gate + batched active-only cards
check('validate_resident_complete available in prod (no cfg(test) gate)', !residentsRs.includes('#[cfg(test)]\npub fn validate_resident_complete'), 'print gate still dead in prod (cfg(test))');
check('QR single path enforces completeness gate', idCardRs.includes('validate_resident_complete'), 'get_qr_data_url skips completeness check');
check('card batch filters inactive residents', /is_active\s*=\s*1/.test(idCardRs) , 'inactive cards still printable');
check('card batch uses single WHERE IN query (no N+1 loop)', /WHERE\s+.*IN\s*\(/i.test(idCardRs), 'batch still N+1 sequential SELECTs');

// 41. Task4: civic.json offline truth (no stale verify URL, unsigned QR documented)
const civicJson = readFileSync(join(root, 'config/civic.json'), 'utf8');
const civic = JSON.parse(civicJson);
check('civic.json carries no verify_base_url (offline-only)', !('verify_base_url' in civic), 'stale netlify verify URL still present');
check('QR unsigned-by-design documented', qrReadme.includes('unsigned'), 'QR doc does not state unsigned status');

// 42. Task6: M-suite hardening (no unwrap panics, log injection, case orphans, img scheme, revoke timing)
const loggerJs = readFileSync(join(root, 'src/js/logger.js'), 'utf8');
check('backup tmp-name handles dir-root dest (no file_name().unwrap())', !backupRs.includes('file_name().unwrap()'), 'unwrap panics when dest is a dir root');
check('frontend log strips newline injection', loggerRs.includes("\\n") && (loggerRs.includes("replace") || loggerRs.includes("' '")), 'log forging via embedded newlines');
check('frontend log caps single-message size', /8000|truncate|chars\(\)\.take/.test(loggerRs), 'one unbounded message blows past 5MB rotation check');
check('photo_gc compares basenames case-insensitively', scheduledRs.includes('to_lowercase'), 'ABC.JPG vs abc.jpg false-orphan on Windows');
check('card template allows only safe img schemes', cardTpl.includes('data:image') && (cardTpl.includes('asset:') || cardTpl.includes('allowlist') || cardTpl.includes('SAFE_IMG')), 'javascript: src would execute in print window');
check('photo read path decrypts at-rest photos', residentsRs.includes('decrypt_or_plaintext') && residentsRs.includes('load_key()'), 'encrypted photos render broken images on read');
check('frontend log download revokes URL on delay (no slow-disk abort)', !loggerJs.includes('a.click(); URL.revokeObjectURL(url)'), 'immediate revoke may abort download');
const residentIdTag = /<input[^>]*id="resident-id"[^>]*>/.exec(indexHtml)?.[0] || '';
check('resident-id input has single aria-describedby', (residentIdTag.match(/aria-describedby/g) || []).length <= 1, 'duplicate aria-describedby on #resident-id');

if (failed) { console.error(`\n${failed} regression check(s) failed`); process.exit(1); }
else console.log(`\nAll regression checks passed — HIGH findings repaired`);
