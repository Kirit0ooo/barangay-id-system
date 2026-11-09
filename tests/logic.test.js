// Behavioral unit tests for pure frontend helpers (src/js/helpers.js).
// These exercise real logic — not grep guards. Run via `npm test`.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

// helpers.js is a classic browser <script> (no exports) — evaluate it in a
// Function scope so tests run against the real source, not a copy.
const src = readFileSync(new URL('../src/js/helpers.js', import.meta.url), 'utf8');
const H = new Function(`${src}; return { formatDateDDMMYYYY, normalizeBirthDateForSubmit, truncateCard, formatContactPH, maskContactPH, todayISO, computeAge, parseValidUntil, PUROK_OPTIONS, joinPath, backupBaseName, PHOTO_MAX_FILE_BYTES, residentStatus };`)();

describe('formatDateDDMMYYYY', () => {
  it('converts ISO to DD-MM-YYYY', () => {
    assert.equal(H.formatDateDDMMYYYY('2026-01-15'), '15-01-2026');
  });
  it('passes through legacy DD-MM-YYYY', () => {
    assert.equal(H.formatDateDDMMYYYY('15-01-2026'), '15-01-2026');
  });
  it('converts DD/MM/YYYY slash form', () => {
    assert.equal(H.formatDateDDMMYYYY('15/01/2026'), '15-01-2026');
  });
  it('returns empty for empty input', () => {
    assert.equal(H.formatDateDDMMYYYY(''), '');
    assert.equal(H.formatDateDDMMYYYY(null), '');
  });
});

describe('normalizeBirthDateForSubmit', () => {
  it('normalizes DD-MM-YYYY to ISO', () => {
    assert.equal(H.normalizeBirthDateForSubmit('15-01-1990'), '1990-01-15');
  });
  it('normalizes DD/MM/YYYY to ISO', () => {
    assert.equal(H.normalizeBirthDateForSubmit('15/01/1990'), '1990-01-15');
  });
  it('leaves ISO untouched', () => {
    assert.equal(H.normalizeBirthDateForSubmit('1990-01-15'), '1990-01-15');
  });
});

describe('truncateCard', () => {
  it('passes short strings through', () => {
    assert.equal(H.truncateCard('JUAN', 12), 'JUAN');
  });
  it('truncates long strings with ellipsis totaling max', () => {
    const out = H.truncateCard('A'.repeat(20), 12);
    assert.equal(out.length, 12);
    assert.ok(out.endsWith('...'));
  });
  it('is char-safe (emoji counted as one)', () => {
    const out = H.truncateCard('😀'.repeat(10), 9);
    assert.equal([...out].length, 9);
  });
  it('handles null', () => {
    assert.equal(H.truncateCard(null, 12), '');
  });
});

describe('formatContactPH', () => {
  it('formats bare 11-digit mobile', () => {
    assert.equal(H.formatContactPH('09171234567'), '0917-123-4567');
  });
  it('folds +63 prefix', () => {
    assert.equal(H.formatContactPH('+639171234567'), '0917-123-4567');
  });
  it('folds bare 63 prefix', () => {
    assert.equal(H.formatContactPH('639171234567'), '0917-123-4567');
  });
  it('passes legacy numbers through trimmed', () => {
    assert.equal(H.formatContactPH('1234567'), '1234567');
  });
  it('returns empty for empty', () => {
    assert.equal(H.formatContactPH('  '), '');
    assert.equal(H.formatContactPH(null), '');
  });
});

describe('maskContactPH', () => {
  it('dashes progressively while typing', () => {
    assert.equal(H.maskContactPH('0990'), '0990');
    assert.equal(H.maskContactPH('09901'), '0990-1');
    assert.equal(H.maskContactPH('0990123'), '0990-123');
    assert.equal(H.maskContactPH('09901234567'), '0990-123-4567');
  });
  it('caps at 11 digits', () => {
    assert.equal(H.maskContactPH('099012345678999'), '0990-123-4567');
  });
  it('strips spaces, parens and dashes', () => {
    assert.equal(H.maskContactPH('0990 123 4567'), '0990-123-4567');
    assert.equal(H.maskContactPH('(0990)123-4567'), '0990-123-4567');
  });
  it('folds 63 prefix and bare 10-digit mobiles', () => {
    assert.equal(H.maskContactPH('+639901234567'), '0990-123-4567');
    assert.equal(H.maskContactPH('639901234567'), '0990-123-4567');
    assert.equal(H.maskContactPH('9901234567'), '0990-123-4567');
  });
  it('leaves BRGY- references untouched (uppercased)', () => {
    assert.equal(H.maskContactPH('brgy-2026-00001'), 'BRGY-2026-00001');
  });
  it('returns empty for empty', () => {
    assert.equal(H.maskContactPH(''), '');
    assert.equal(H.maskContactPH(null), '');
  });
});

describe('computeAge', () => {
  it('computes age from ISO', () => {
    const now = new Date();
    const twenty = new Date(now.getFullYear() - 20, now.getMonth(), now.getDate());
    const iso = `${twenty.getFullYear()}-${String(twenty.getMonth() + 1).padStart(2, '0')}-${String(twenty.getDate()).padStart(2, '0')}`;
    assert.equal(H.computeAge(iso), 20);
  });
  it('rejects future dates', () => {
    assert.equal(H.computeAge('2099-01-01'), null);
  });
  it('rejects impossible dates', () => {
    assert.equal(H.computeAge('1990-02-30'), null);
    assert.equal(H.computeAge('not-a-date'), null);
    assert.equal(H.computeAge(''), null);
  });
  it('accepts DD-MM-YYYY', () => {
    assert.ok(typeof H.computeAge('15-01-1990') === 'number');
  });
});

describe('todayISO', () => {
  it('matches local date', () => {
    const d = new Date();
    const expected = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
    assert.equal(H.todayISO(), expected);
  });
});

describe('parseValidUntil', () => {
  it('passes ISO YYYY-MM-DD through unchanged', () => {
    assert.equal(H.parseValidUntil('2027-01-15'), '2027-01-15');
  });
  it('converts legacy DD-MM-YYYY to ISO', () => {
    assert.equal(H.parseValidUntil('15-01-2027'), '2027-01-15');
  });
  it('converts DD/MM/YYYY slash form to ISO', () => {
    assert.equal(H.parseValidUntil('15/01/2027'), '2027-01-15');
  });
  it('rejects empty, null, and unparseable input', () => {
    assert.equal(H.parseValidUntil(''), null);
    assert.equal(H.parseValidUntil('   '), null);
    assert.equal(H.parseValidUntil(null), null);
    assert.equal(H.parseValidUntil(undefined), null);
    assert.equal(H.parseValidUntil('15-01'), null);
    assert.equal(H.parseValidUntil('not-a-date'), null);
  });
  it('rejects partial-parseInt traps (no fabricated year from garbage)', () => {
    // parseInt('20XX') === 20 — the owner must not accept non-numeric parts.
    assert.equal(H.parseValidUntil('15-01-20XX'), null);
    assert.equal(H.parseValidUntil('XX-01-2027'), null);
    assert.equal(H.parseValidUntil('15-XX-2027'), null);
  });
  it('rejects zero day/month/year', () => {
    assert.equal(H.parseValidUntil('00-01-2027'), null);
    assert.equal(H.parseValidUntil('15-00-2027'), null);
    assert.equal(H.parseValidUntil('15-01-0000'), null);
  });
  it('agrees with the pill + countdown: expired vs not (isExpired/validDaysLeft share it)', () => {
    // Sanity that both consumers read the same parse: an ISO past date is expired
    // with a negative countdown; a future date is not.
    const past = { valid_until: '2020-01-01' };
    const iso = H.parseValidUntil(past.valid_until);
    assert.ok(iso < H.todayISO()); // isExpired reads this comparison
  });
});

describe('joinPath / backupBaseName / constants', () => {
  it('joins without backslashes and trims trailing sep', () => {
    assert.equal(H.joinPath('a/b/', 'c.xlsx'), 'a/b/c.xlsx');
    assert.equal(H.joinPath('a\\b\\\\', 'c.xlsx'), 'a\\b/c.xlsx');
    assert.equal(H.joinPath('', 'c.xlsx'), 'c.xlsx');
  });
  it('backupBaseName matches BACKUP-YYYY-MM-DD.<ext>', () => {
    assert.match(H.backupBaseName('xlsx'), /^BACKUP-\d{4}-\d{2}-\d{2}\.xlsx$/);
  });
  it('purok allowlist has the 5 puroks', () => {
    assert.deepEqual(H.PUROK_OPTIONS, ['AVOCADO', 'RIMAS', 'PASAYAN', 'BUBUNAO', 'ASLUM']);
  });
  it('photo cap is 6MB', () => {
    assert.equal(H.PHOTO_MAX_FILE_BYTES, 6 * 1024 * 1024);
  });
});

describe('residentStatus (D6: remarks outrank expiry)', () => {
  it('expired LOST card shows LOST, not Expired', () => {
    assert.equal(H.residentStatus({ remarks: 'LOST', is_active: 1 }, true), 'LOST');
  });
  it('expired RENEW and DAMAGE keep their labels', () => {
    assert.equal(H.residentStatus({ remarks: 'RENEW', is_active: 1 }, true), 'RENEW');
    assert.equal(H.residentStatus({ remarks: 'DAMAGE', is_active: 1 }, true), 'DAMAGE');
  });
  it('expired ACTIVE card still shows Expired', () => {
    assert.equal(H.residentStatus({ remarks: 'ACTIVE', is_active: 1 }, true), 'Expired');
    assert.equal(H.residentStatus({ is_active: 1 }, true), 'Expired'); // no remarks
  });
  it('non-expired ACTIVE stays ACTIVE (uppercase, unchanged)', () => {
    assert.equal(H.residentStatus({ remarks: 'ACTIVE', is_active: 1 }, false), 'ACTIVE');
    assert.equal(H.residentStatus({ is_active: 1 }, false), 'ACTIVE');
  });
  it('inactive beats everything', () => {
    assert.equal(H.residentStatus({ is_active: 0, remarks: 'LOST' }, true), 'Inactive');
  });
  it('null record defaults to Active', () => {
    assert.equal(H.residentStatus(null, false), 'Active');
  });
});
