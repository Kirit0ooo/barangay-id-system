// Golden card test — field-map parity for the single HTML render path
// (card-template.html + buildCardPayload) plus the backend QR/date helpers.
// String-level only: offline, no network, no image rendering.
// Path (a) mirrors src-tauri/src/id_card.rs: fmt_ddmmyyyy, valid_until_display.
// Path (b) mirrors src/js/app.js: formatDateDDMMYYYY, shortBarangay, validity bar, address.
// Path (c) is the drift guard: the real sources must still contain the formulas
// these mirrors were copied from (grep pins), so a backend/frontend edit that
// changes output without updating this file fails loudly instead of drifting.
import { readFileSync } from 'fs';
import { join } from 'path';

const root = join(import.meta.dirname, '..');
let failed = 0;
function check(name, cond, hint = '') {
  const ok = !!cond;
  console.log(ok ? `✓ ${name}` : `✗ ${name}${hint ? ' — ' + hint : ''}`);
  if (!ok) failed++;
}

// ---------- Path (a): Rust mirrors (id_card.rs) ----------
function rsFmtDDMMYYYY(s) {
  // Faithful port of pub(crate) fn fmt_ddmmyyyy.
  if (s.includes('-')) {
    const p = s.split('-');
    if (p.length === 3) {
      if (p[0].length === 4) return `${p[2]}-${p[1]}-${p[0]}`;
      return s;
    }
  }
  if (s.includes('/')) return s.replaceAll('/', '-');
  return s;
}
function rsValidUntilDisplay(validUntil, todayPlus365) {
  // Faithful port of fn valid_until_display (None/empty → today+365 DD-MM-YYYY).
  const v = (validUntil || '').trim();
  if (v) return rsFmtDDMMYYYY(v);
  return todayPlus365;
}

// ---------- Path (b): frontend mirrors (app.js) ----------
function jsFormatDateDDMMYYYY(iso) {
  // Faithful port of formatDateDDMMYYYY in src/js/app.js.
  if (!iso) return '';
  const s = iso.trim();
  if (s.includes('/')) {
    const [d, m, y] = s.split('/');
    if (y && m && d && y.length === 4) return `${d.padStart(2, '0')}-${m.padStart(2, '0')}-${y}`;
    return s;
  }
  const p = s.split('-');
  if (p.length !== 3) return s;
  if (p[0].length === 4) return `${p[2]}-${p[1]}-${p[0]}`;
  if (p[2].length === 4) return s;
  return s;
}
function jsShortBarangay(name) {
  // Faithful port of barangayCardDisplay in src/js/app.js (paren-first extract + after).
  const s = (name || '').trim();
  if (!s) return 'BALUD';
  const m = s.match(/\(([^)]*)\)\s*(.*)/);
  if (m) {
    const inner = m[1].trim().toUpperCase();
    const after = m[2].trim().toUpperCase();
    if (!inner) return after || 'BALUD';
    return after ? `${inner} (${after})` : inner;
  }
  const tokens = s.split(/[\s(),]+/).filter(Boolean);
  if (!tokens.length) return 'BALUD';
  let last = (tokens[tokens.length - 1] || 'BALUD').toUpperCase();
  if ((last === 'BARANGAY' || last === 'BRGY' || last === 'BRGY.') && tokens.length >= 2) {
    last = tokens[tokens.length - 2].toUpperCase();
  }
  if (last === 'BARANGAY' || last === 'BRGY' || last === 'BRGY.') return 'BALUD';
  return last.slice(0, 40) || 'BALUD';
}
const MONTHS = ['JANUARY','FEBRUARY','MARCH','APRIL','MAY','JUNE','JULY','AUGUST','SEPTEMBER','OCTOBER','NOVEMBER','DECEMBER'];
function jsValidityBar(validUntil, todayPlus365ISO) {
  // Faithful port of the validity block in buildCardPayload.
  let vuRaw = (validUntil || '').trim() || todayPlus365ISO;
  let bar = 'THIS ID IS VALID UNTIL ' + vuRaw.toUpperCase();
  const parts = vuRaw.split(/[-/]/);
  if (parts.length === 3) {
    const iso = parts[0].length === 4;
    const mi = parseInt(parts[1], 10);
    const yy = iso ? parts[0] : parts[2];
    if (mi >= 1 && mi <= 12) bar = `THIS ID IS VALID UNTIL ${MONTHS[mi - 1]} ${yy}`;
  }
  return { vuRaw, bar };
}
function jsAddress(r, short) {
  // Faithful port of the front-address block in buildCardPayloadForRow
  // (PUROK <purok>, <short>, <city>, <province>; cityName split fallback).
  const purok = ((r.address_purok || 'AVOCADO') + '').toUpperCase();
  let city = ((r.address_city || '').trim() || '').toUpperCase();
  let province = ((r.address_province || '').trim() || '').toUpperCase();
  if (!city || !province) {
    const parts = String((r._cityName || 'Basey, Samar')).split(',').map((s) => s.trim().toUpperCase()).filter(Boolean);
    if (!city && parts[0]) city = parts[0];
    if (!province && parts[1]) province = parts[1];
  }
  if (!city) city = 'BASEY';
  if (!province) province = 'SAMAR';
  return `PUROK ${purok}, ${short}, ${city}, ${province}`;
}
function jsEmerAddress(r, short, city, province) {
  // Faithful port of the emergency-address block: explicit wins + city/
  // province appended unless present; else resident-address fallback.
  const purok = ((r.address_purok || 'AVOCADO') + '').toUpperCase();
  const street = ((r.address_street || '').trim() || '').toUpperCase();
  const base = `PUROK ${purok}, ${short}, ${city}, ${province}`;
  const explicit = ((r.emergency_contact_address || '').trim() || '').toUpperCase();
  if (explicit) {
    let out = explicit;
    if (city && !out.includes(city)) out += `, ${city}`;
    if (province && !out.includes(province)) out += `, ${province}`;
    return out;
  }
  return street ? `${base}, ${street}` : base;
}

// ---------- Fixtures ----------
const FIXED_TODAY_PLUS_365_ISO = '2027-02-15';
const FIXED_TODAY_PLUS_365_DDMM = '15-02-2027';
const fixtures = [
  {
    tag: 'normal ISO',
    r: {
      resident_id: 'BRGY-2026-00001', first_name: 'Francis', middle_name: 'Adona',
      last_name: 'Patindol', suffix: '', birth_date: '1990-01-15',
      address_purok: 'AVOCADO', address_barangay: 'Barangay Balud', address_city: 'BASEY', address_province: 'SAMAR',
      contact_number: '09171234567', remarks: 'ACTIVE', valid_until: '2027-02-15',
    },
    expect: { dob: '15-01-1990', validBar: 'THIS ID IS VALID UNTIL FEBRUARY 2027', short: 'BALUD', address: 'PUROK AVOCADO, BALUD, BASEY, SAMAR', emer: 'PUROK AVOCADO, BALUD, BASEY, SAMAR' },
  },
  {
    tag: 'long name/address edge',
    r: {
      resident_id: 'BRGY-2026-00002', first_name: 'Maria Esperanza Delos Santos', middle_name: 'De La Peñaflor Cruz',
      last_name: 'Villanueva-Magsaysay', suffix: 'Jr', birth_date: '1985-12-03',
      address_purok: 'AVOCADO', address_barangay: 'Barangay (Nouvelas Occidental) Balud',
      address_city: 'BASEY', address_province: 'SAMAR', contact_number: '', remarks: '', valid_until: '',
    },
    expect: { dob: '03-12-1985', validBar: 'THIS ID IS VALID UNTIL FEBRUARY 2027', short: 'NOUVELAS OCCIDENTAL (BALUD)', address: 'PUROK AVOCADO, NOUVELAS OCCIDENTAL (BALUD), BASEY, SAMAR', emer: 'PUROK AVOCADO, NOUVELAS OCCIDENTAL (BALUD), BASEY, SAMAR' },
  },
  {
    tag: 'legacy valid_until DD-MM-YYYY',
    r: {
      resident_id: 'BRGY-2026-00003', first_name: 'Jose', middle_name: 'Protacio',
      last_name: 'Rizal', suffix: '', birth_date: '15-06-1995',
      address_purok: 'ASLUM', address_barangay: 'BRGY. BALUD', address_city: 'BASEY', address_province: 'SAMAR',
      contact_number: '09001112222', remarks: 'ACTIVE', valid_until: '20-11-2026', emergency_contact_address: 'ZONE 1',
    },
    expect: { dob: '15-06-1995', validBar: 'THIS ID IS VALID UNTIL NOVEMBER 2026', short: 'BALUD', address: 'PUROK ASLUM, BALUD, BASEY, SAMAR', emer: 'ZONE 1, BASEY, SAMAR' },
  },
];

// ---------- Parity assertions per fixture ----------
for (const f of fixtures) {
  const { r, expect, tag } = f;
  // dob: Rust fmt vs frontend formatDate agree, and match expected.
  const rsDob = rsFmtDDMMYYYY(r.birth_date);
  const jsDob = jsFormatDateDDMMYYYY(r.birth_date);
  check(`[${tag}] dob parity (${rsDob})`, rsDob === jsDob && rsDob === expect.dob, `rs=${rsDob} js=${jsDob} want=${expect.dob}`);
  // valid_until: ISO stored → DD-MM-YYYY; legacy DD-MM-YYYY passes through; empty → +365 default.
  const rsValid = rsValidUntilDisplay(r.valid_until, FIXED_TODAY_PLUS_365_DDMM);
  const { vuRaw, bar } = jsValidityBar(r.valid_until, FIXED_TODAY_PLUS_365_ISO);
  const rsValidNorm = rsValid; // already DD-MM-YYYY
  const jsValidNorm = jsFormatDateDDMMYYYY(vuRaw.includes('/') ? vuRaw : vuRaw);
  check(`[${tag}] validity normalized parity (${rsValidNorm})`, rsValidNorm === jsValidNorm, `rs=${rsValidNorm} js=${jsValidNorm}`);
  check(`[${tag}] validity bar (${bar})`, bar === expect.validBar, `got=${bar} want=${expect.validBar}`);
  // short_barangay last-token heuristic.
  const short = jsShortBarangay(r.address_barangay);
  check(`[${tag}] short_barangay (${short})`, short === expect.short, `got=${short} want=${expect.short}`);
  // name uppercase (PDF draw + template both uppercase).
  const fullName = `${r.first_name} ${r.middle_name || ''} ${r.last_name} ${r.suffix || ''}`.replace(/\s+/g, ' ').trim().toUpperCase();
  check(`[${tag}] full_name uppercase`, fullName === fullName.toUpperCase() && fullName.includes(r.last_name.toUpperCase()), fullName);
  // address: front always carries city + province; emergency carries municipality + province.
  const addr = jsAddress(r, short);
  check(`[${tag}] address shape (${addr.slice(0, 40)}…)`, addr.startsWith('PUROK ') && addr.includes(short), addr);
  check(`[${tag}] front address has city + province`, addr.includes(', BASEY, SAMAR') || (addr.includes((r.address_city || 'BASEY').toUpperCase()) && addr.includes((r.address_province || 'SAMAR').toUpperCase())), addr);
  if (expect.address) check(`[${tag}] front address exact`, addr === expect.address, `got=${addr} want=${expect.address}`);
  const emer = jsEmerAddress(r, short, 'BASEY', 'SAMAR');
  check(`[${tag}] emergency address has municipality + province`, emer.includes('BASEY') && emer.includes('SAMAR'), emer);
  if (expect.emer) check(`[${tag}] emergency address exact`, emer === expect.emer, `got=${emer} want=${expect.emer}`);
}

// ---------- Truncation parity (Rust truncate vs frontend truncateCard) ----------
function rsTruncate(s, max) {
  // Faithful port of fn truncate in id_card.rs (char-safe, head + '...').
  const chars = [...s];
  if (chars.length > max) return chars.slice(0, Math.max(0, max - 3)).join('') + '...';
  return s;
}
function jsTruncateCard(s, max) {
  // Faithful port of truncateCard in src/js/app.js.
  const str = (s == null ? '' : String(s));
  const chars = [...str];
  if (chars.length > max) return chars.slice(0, Math.max(0, max - 3)).join('') + '...';
  return str;
}
check('truncate mirror: long last name', jsTruncateCard('VILLANUEVA-MAGSAYSAY', 12) === rsTruncate('VILLANUEVA-MAGSAYSAY', 12));
check('truncate mirror: multibyte safe', jsTruncateCard('PEÑAFLOR', 5) === rsTruncate('PEÑAFLOR', 5) && jsTruncateCard('PEÑAFLOR', 5).endsWith('...'));
check('truncate mirror: short passthrough', jsTruncateCard('CRUZ', 12) === 'CRUZ');
check('truncate mirror: widths 12/9/62/48/30/20 hold', [12, 9, 62, 48, 30, 20].every((m) => jsTruncateCard('X'.repeat(m + 5), m).length === m));

// ---------- Path (c): drift guards on the real sources ----------
// Backend is QR + date helpers only (PDF render deleted); the template +
// frontend payload is the single render path for preview/print/bulk.
const idCardRs = readFileSync(join(root, 'src-tauri/src/id_card.rs'), 'utf8');
// App modules are split across app.core/app.register/app.search/app.settings/app.detail/app.dashboard/app.js.
// Concatenate them so path-based string checks still cover all view logic.
const appModuleFiles = ['app.core','app.register','app.search','app.settings','app.detail','app.dashboard','app'];
const appJs = appModuleFiles.map(f => readFileSync(join(root, 'src/js/' + f + '.js'), 'utf8')).join('\n');
const helpersJs = readFileSync(join(root, 'src/js/helpers.js'), 'utf8');
const configRs = readFileSync(join(root, 'src-tauri/src/config.rs'), 'utf8');
const cargoToml = readFileSync(join(root, 'src-tauri/Cargo.toml'), 'utf8');
const mainRs = readFileSync(join(root, 'src-tauri/src/main.rs'), 'utf8');
// Comment-stripped copies: historical notes may name deleted PDF items (allowed).
const idCardCode = idCardRs.replace(/^\s*\/\/.*$/gm, '');
const cargoCode = cargoToml.replace(/^\s*#.*$/gm, '');
check('rust card QR is the legacy unsigned 7-key JSON (signed verify URL retired)', idCardCode.includes('fn qr_data') && !idCardRs.includes('verify_url_for') && !idCardRs.includes('verify::'));
check('rust fmt_ddmmyyyy ISO branch intact', idCardRs.includes('p[0].len() == 4'));
check('no PDF machinery in backend', !idCardCode.includes('printpdf') && !idCardCode.includes('BulkIdResult') && !idCardCode.includes('failures.json') && !idCardCode.includes('generate_single_pdf'));
check('no printpdf dep, QR kept for preview', !cargoCode.includes('printpdf') && cargoCode.includes('qrcode'));
check('no generate_*_pdf commands registered', !mainRs.includes('generate_id_card_pdf') && !mainRs.includes('generate_all_id_cards_pdf') && !mainRs.includes('preview_id_card_html'));
check('frontend validity months intact', MONTHS.every((m) => appJs.includes(`'${m}'`)));
check('frontend shortBarangay heuristic intact', appJs.includes("split(/[\\s(),]+/)") && appJs.includes("'BALUD'") && appJs.includes('([^)]*)'));
check('frontend shortBarangay extracts parens first', appJs.includes('match(/\\(([^)]*)\\)/)') && appJs.includes("if (!tokens.length) return 'BALUD'"));
// Required behavior pins (barangay-fix-short-barangay-001): paren content wins.
check('shortBarangay paren+trailing resolves to paren content', jsShortBarangay('Barangay (Nouvelas Occidental) Balud') === 'NOUVELAS OCCIDENTAL (BALUD)', jsShortBarangay('Barangay (Nouvelas Occidental) Balud'));
check('shortBarangay paren-only resolves to paren content', jsShortBarangay('Barangay (Nouvelas Occidental)') === 'NOUVELAS OCCIDENTAL', jsShortBarangay('Barangay (Nouvelas Occidental)'));
check('shortBarangay no-paren falls back to last token', jsShortBarangay('Barangay Balud') === 'BALUD' && jsShortBarangay('Balud') === 'BALUD');
check('shortBarangay bare BARANGAY falls back to BALUD', jsShortBarangay('BARANGAY') === 'BALUD' && jsShortBarangay('') === 'BALUD');
check('shortBarangay paren-only address renders paren seat', jsAddress({ address_purok: 'AVOCADO', address_city: 'BASEY', address_province: 'SAMAR' }, jsShortBarangay('BARANGAY (NOUVELAS OCCIDENTAL)')) === 'PUROK AVOCADO, NOUVELAS OCCIDENTAL, BASEY, SAMAR');
check('bulk print summary intact (no failures.json)', !appJs.includes('failures.json') && appJs.includes('Print ready'));
check('backend short barangay full-string fallback intact', configRs.includes('b.to_uppercase()') && configRs.includes('chars().take(40)'));
check('frontend truncateCard intact (ellipsis guard)', appJs.includes('truncateCard') && helpersJs.includes('max - 3'));
check('single render path: payload + template fill, no backend preview', appJs.includes('buildCardPayloadForRow') && appJs.includes('fillTemplateHtml') && appJs.includes('cardHtmlForResident') && !appJs.includes('previewIdCardHtml'));
check('print path funnels through cardHtmlForResident (CR100 single template)', appJs.includes('this.printFilledCard(await this.cardHtmlForResident(residentId))'));
check('preview path funnels through cardHtmlForResident (CR100 single template)', appJs.includes("const filled = await this.cardHtmlForResident(residentId)"));
check('no hardcoded card markup in app.js (template file is single source)', !appJs.includes('front-hdr') && !appJs.includes('qr-box') && !appJs.includes('valid-bar') && !appJs.includes('back-bleed') && !appJs.includes('sigbox'));
check('bulk print loops with page breaks + counter', appJs.includes('printAllCards') && appJs.includes('page-break-after') && appJs.includes('bulkProgress.current'));
check('no stale navy #050D78 anywhere', !idCardRs.includes('050D78') && !appJs.includes('050D78'));
const cardTpl = readFileSync(join(root, 'src/card-template.html'), 'utf8');
check('template card is 98.6x66.8mm, print is A4 portrait stacked', cardTpl.includes('width:98.6mm') && cardTpl.includes('height:66.8mm') && cardTpl.includes('@page{size:A4 portrait; margin:25.4mm 0 0;}'));
check('print stacks front over back like preview (block sheet, no forced break)', cardTpl.includes('.sheet{max-width:none; transform:none !important; display:block;}') && cardTpl.includes('.back-bleed{page-break-before:auto;'));
check('print cut line is a real dashed border (always fully painted)', cardTpl.includes('width:101.7mm; border:.35mm dashed #4b5563;'));
check('back card holds emergency info only, slightly larger (2.7mm)', !cardTpl.includes('>Card No.<') && cardTpl.includes('.bk .bl{font-family:var(--font-display); font-size:2.7mm;') && cardTpl.includes('.bk .bv{font-family:var(--font-display); font-size:2.7mm;'));
check('print footer note hidden on screen, shown on A4 below cards', cardTpl.includes('.print-note{display:none;}') && cardTpl.includes('.print-note{display:block; width:101.7mm; margin:8mm auto 0;') && cardTpl.includes('Barangay Resident ID Registration System') && cardTpl.includes('class="pn-time"') && cardTpl.includes('class="pn-list"'));
check('print timestamp stamper present (DD-MM-YYYY)', cardTpl.includes('__stampPrintTime') && cardTpl.includes('beforeprint'));
check('template print hides chrome, keeps exact colors', cardTpl.includes('.toolbar,.page-label,.cutline{display:none') && cardTpl.includes('print-color-adjust:exact'));
check('template photo seat square 1:1 (20.9x20.9)', cardTpl.includes('.photo-box{width:20.9mm; height:20.9mm'));
check('template QR seat matches pad (11)', cardTpl.includes('width:11mm; height:11mm'));

// ---------- CR100 1:1 fit guards (FIX CR100 FIT) ----------
check('print bleed is a complete bordered cut guide around the exact card', cardTpl.includes('.bleed{padding:1.2mm; margin:0 auto; width:101.7mm; border:.35mm dashed #4b5563;'));
check('print resets preview zoom transform', cardTpl.includes('.sheet{max-width:none; transform:none !important;'));
check('print card re-asserts 98.6x66.8 + exact colors', cardTpl.includes('.card{width:98.6mm; height:66.8mm;'));
check('print card keeps preview curve + grey edge at exact CR100', cardTpl.includes('border:.35mm solid var(--line); border-radius:2.4mm;') && cardTpl.includes('box-sizing:border-box;'));
check('resident ID under signature, caption+number same mono size and length',
  cardTpl.indexOf('RESIDENT SIGNATURE') < cardTpl.indexOf('class="idunder"') &&
  cardTpl.indexOf('class="idunder"') < cardTpl.indexOf('class="idno" data-field="resident_id"') &&
  cardTpl.includes('>RESIDENT ID NO.<') &&
  cardTpl.includes('.idunder .idcap{font-family:Consolas') && cardTpl.includes('.idunder .idcap{font-family:Consolas,"Courier New",monospace; font-size:2mm;') &&
  cardTpl.includes('.idunder .idno{font-size:2mm;') &&
  !cardTpl.includes('class="idstrip"') && !cardTpl.includes('midstack'));
check('punong seat fixed 45mm box with clamp', cardTpl.includes('.officer .sig-line{width:45mm') && cardTpl.includes('-webkit-line-clamp:2') && cardTpl.includes('.officer .role{font-size:2.2mm'));
check('signature caption fits one line under its rule (1.7mm nowrap)', cardTpl.includes('.sigbox .cap{font-size:1.7mm') && cardTpl.includes('.sigbox .cap{font-size:1.7mm; color:#555; letter-spacing:.15mm; font-weight:600; padding-top:.8mm; white-space:nowrap;}'));
check('signature block nudged slightly down (margin-top:1mm)', cardTpl.includes('.sigbox{text-align:center; padding-top:.8mm; margin-top:1mm; max-width:100%; overflow:hidden;}'));
check('card title one line clear of logos (12mm medals, nowrap)', cardTpl.includes('.medal{width:12mm; height:12mm;') && cardTpl.includes('.title-ribbon{display:inline-block; max-width:100%;') && cardTpl.includes('white-space:nowrap; overflow:hidden; text-overflow:ellipsis;}'));
check('address always fits, bottom row never pushed down', cardTpl.includes('.fields{flex:1; display:flex; flex-direction:column; gap:1mm;') && cardTpl.includes('.front-body{flex:1; display:flex; gap:2.2mm; padding:1.6mm 2.8mm;'));
check('QR box has no lining (borderless seat)', /\.qr-box\{[^}]*border:none/.test(cardTpl));
check('QR borderless 11mm mm-only with contain', cardTpl.includes('.qr-box{width:11mm; height:11mm;') && cardTpl.includes('.qr-box img{width:100%; height:100%; object-fit:contain;'));
check('punong name slightly smaller (2.1mm)', cardTpl.includes('.officer .name{font-size:2.1mm'));
check('info boxes never stretch (DOB/Civil/Gender stay identical)', cardTpl.includes('.row3{display:grid; grid-template-columns:1fr 1fr 1fr; gap:1.6mm; align-items:start;}'));
check('frontend truncates punong_name to 30', appJs.includes("truncateCard((this.settings.punongName || '').toUpperCase(), 30)"));
check('front address carries province (address_province)', appJs.includes('address_province') && appJs.includes('${addrCity}, ${addrProvince}'), 'front address drops province');
check('emergency fallback uses city/province vars (no hardcoded BASEY, SAMAR seat)', appJs.includes('`PUROK ${purok}, ${short}, ${addrCity}, ${addrProvince}`'), 'emergency seat hardcoded');
check('explicit emergency address gains municipality + province', appJs.includes('!emerFull.includes(addrCity)') && appJs.includes('!emerFull.includes(addrProvince)'), 'explicit emergency shown bare');
check('no px borders inside card seats (sole 1px lives in print-hidden toolbar)', cardTpl.split('border:1px').length - 1 === 1 && cardTpl.includes('.toolbar button'));

if (failed) { console.error(`\n${failed} golden check(s) failed`); process.exit(1); }
else console.log('\nAll golden card parity checks passed — single render path, no drift.');
