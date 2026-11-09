// Pure frontend helpers — shared by app.js (classic <script> globals) and
// tests/logic.test.js (via the module.exports guard at the bottom).
// No DOM, no Tauri, no Alpine: keep it that way so tests stay dependency-free.
function formatDateDDMMYYYY(iso) {
    if (!iso) return '';
    const s = iso.trim();
    // Accept YYYY-MM-DD, DD-MM-YYYY, DD/MM/YYYY, with or without '/'.
    // Stored is now canonical YYYY-MM-DD (R3), but old rows may still be DD-MM-YYYY.
    if (s.includes('/')) {
        const [d,m,y] = s.split('/');
        if (y && m && d && y.length === 4) return `${d.padStart(2,'0')}-${m.padStart(2,'0')}-${y}`;
        return s;
    }
    const p = s.split('-');
    if (p.length !== 3) return s;
    if (p[0].length === 4) return `${p[2]}-${p[1]}-${p[0]}`; // YYYY-MM-DD -> DD-MM-YYYY
    if (p[2].length === 4) return s; // already DD-MM-YYYY
    return s;
}
function normalizeBirthDateForSubmit(s) {
    // R3 helper: normalize user input to YYYY-MM-DD before invoke (backend also normalizes).
    if (!s) return s;
    const t = s.trim();
    if (!t) return t;
    if (t.includes('/')) { const [d,m,y]=t.split('/'); if (y&&m&&d) return `${y.padStart(4,'0')}-${m.padStart(2,'0')}-${d.padStart(2,'0')}`; }
    if (t.includes('-')) { const p=t.split('-'); if (p.length===3 && p[0].length!==4 && p[2].length===4) return `${p[2]}-${p[1].padStart(2,'0')}-${p[0].padStart(2,'0')}`; }
    return t;
}

// Mirrors id_card.rs truncate(): char-safe, over-long → head + '...' (total max chars).
// Keeps card-template preview overflow identical across preview/print/bulk.
function truncateCard(s, max) {
    const str = (s == null ? '' : String(s));
    const chars = [...str];
    if (chars.length > max) return chars.slice(0, Math.max(0, max - 3)).join('') + '...';
    return str;
}
// Mirrors residents.rs normalize_ph_contact/format_contact_for_display:
// PH mobiles → 09XX-XXX-XXXX display form, anything else trimmed passthrough.
function formatContactPH(raw) {
    if (raw == null) return '';
    const t = String(raw).trim();
    if (!t) return '';
    let digits = t.replace(/\D/g, '');
    if ((t.startsWith('+') || digits.length === 12) && digits.startsWith('63')) digits = '0' + digits.slice(2);
    if (/^09\d{9}$/.test(digits)) return `${digits.slice(0, 4)}-${digits.slice(4, 7)}-${digits.slice(7, 11)}`;
    return t;
}
// Live mask for PH mobile inputs: digits only, max 11, progressive 4-3-4
// (0990 → 0990-123 → 0990-123-4567). Emergency BRGY- ID references are
// not phones and pass through uppercased (backend verifies existence).
function maskContactPH(raw) {
    if (raw == null) return '';
    const t = String(raw);
    if (/^\s*brgy-/i.test(t)) return t.trim().toUpperCase();
    let digits = t.replace(/\D/g, '');
    // Fold +63/63 country prefix and bare 10-digit mobiles into 09 form.
    if (digits.length >= 12 && digits.startsWith('63')) digits = '0' + digits.slice(2);
    else if (digits.length === 10 && digits.startsWith('9')) digits = '0' + digits;
    digits = digits.slice(0, 11);
    if (digits.length <= 4) return digits;
    if (digits.length <= 7) return digits.slice(0, 4) + '-' + digits.slice(4);
    return digits.slice(0, 4) + '-' + digits.slice(4, 7) + '-' + digits.slice(7);
}
function todayISO() {
    // P2-14: local date (backend uses chrono::Local) — UTC slicing was off
    // by one near midnight (up to ~8h PH offset edge).
    const d = new Date();
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

// Single source of truth for parsing a resident's valid_until into an ISO
// YYYY-MM-DD string (or null when unset/unparseable). Accepts ISO storage plus
// legacy DD-MM-YYYY / DD/MM/YYYY rows. isExpired() and validDaysLeft() both
// derive from this so the two can never drift on date-format handling again.
// (Previously each had its own parser that disagreed on malformed input.)
function parseValidUntil(vu) {
    if (vu == null) return null;
    const s = String(vu).trim();
    if (!s) return null;
    if (/^\d{4}-\d{2}-\d{2}$/.test(s)) return s;
    const p = s.split(/[-/]/);
    if (p.length !== 3) return null;
    // DD-MM-YYYY / DD/MM/YYYY = day, month, year. Require all-numeric parts so
    // a partial parseInt (e.g. parseInt('20XX') === 20) can't fabricate a date.
    const [d, m, y] = p;
    if (!/^\d+$/.test(d) || !/^\d+$/.test(m) || !/^\d+$/.test(y)) return null;
    const day = parseInt(d, 10), month = parseInt(m, 10), year = parseInt(y, 10);
    if (!year || !month || !day) return null;
    return `${String(year).padStart(4, '0')}-${String(month).padStart(2, '0')}-${String(day).padStart(2, '0')}`;
}

function computeAge(iso) {
    if (!iso) return null;
    const s = iso.trim();
    let y, m, d;
    if (s.includes('/')) {
        const p = s.split('/');
        if (p.length !== 3) return null;
        d = Number(p[0]); m = Number(p[1]); y = Number(p[2]);
    } else if (s.includes('-')) {
        const p = s.split('-');
        if (p.length !== 3) return null;
        if (p[0].length === 4) { y = Number(p[0]); m = Number(p[1]); d = Number(p[2]); }
        else { d = Number(p[0]); m = Number(p[1]); y = Number(p[2]); }
    } else return null;
    if (!y || !m || !d) return null;
    const dob = new Date(y, m - 1, d);
    if (Number.isNaN(dob.getTime()) || dob.getFullYear() !== y || dob.getMonth() !== m - 1 || dob.getDate() !== d) return null;
    const now = new Date();
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    if (dob > today) return null;
    let age = today.getFullYear() - dob.getFullYear();
    const mdiff = today.getMonth() - dob.getMonth();
    if (mdiff < 0 || (mdiff === 0 && today.getDate() < dob.getDate())) age--;
    return age;
}

// F14: single source of truth for purok options (register + search selects mirror this).
const PUROK_OPTIONS = ['AVOCADO', 'RIMAS', 'PASAYAN', 'BUBUNAO', 'ASLUM'];

// F12: portable path join — never backslash concat; dialog + Rust accept '/' on Windows.
function joinPath(dir, file) {
    if (!dir) return file;
    return dir.replace(/[\\/]+$/, '') + '/' + file;
}

// Bulk Excel base name: BACKUP-YYYY-MM-DD.<ext> (also used for bulk-print titles).
function backupBaseName(ext) {
    const d = new Date();
    const yyyy = d.getFullYear();
    const mm = String(d.getMonth() + 1).padStart(2, '0');
    const dd = String(d.getDate()).padStart(2, '0');
    return `BACKUP-${yyyy}-${mm}-${dd}.${ext}`;
}

// F17: photo limits aligned with backend (6MB binary ~= 8MB base64); both enforced.
const PHOTO_MAX_FILE_BYTES = 6 * 1024 * 1024;

// D6: card status shown on the pill. Operator-set remarks (LOST/RENEW/DAMAGE)
// outrank generic expiry so a lost card never reads as merely "Expired"; the
// detail view still surfaces expiry on its own via expiryCountdown. `isExpired`
// is injected so this stays a pure, testable function.
function residentStatus(r, isExpired) {
    if (!r) return 'Active';
    if (r.is_active === 0) return 'Inactive';
    const raw = (((r.remarks || 'ACTIVE') + '').toUpperCase());
    if (raw === 'LOST' || raw === 'RENEW' || raw === 'DAMAGE') return raw;
    if (isExpired) return 'Expired';
    return raw;
}

// Node test harness only — undefined in the browser, so this is a no-op there.
if (typeof module !== 'undefined' && module.exports) {
    module.exports = {
        formatDateDDMMYYYY, normalizeBirthDateForSubmit, truncateCard,
        formatContactPH, maskContactPH, todayISO, computeAge, PUROK_OPTIONS,
        joinPath, backupBaseName, PHOTO_MAX_FILE_BYTES, residentStatus,
    };
}
