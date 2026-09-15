// Behavioral unit tests for pure frontend helpers (src/js/helpers.js).
// These exercise real logic — not grep guards. Run via `npm test`.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

// helpers.js is a classic browser <script> (no exports) — evaluate it in a
// Function scope so tests run against the real source, not a copy.
const src = readFileSync(new URL('../src/js/helpers.js', import.meta.url), 'utf8');
const H = new Function(`${src}; return { formatDateDDMMYYYY, normalizeBirthDateForSubmit, truncateCard, formatContactPH, todayISO, computeAge, PUROK_OPTIONS, joinPath, backupBaseName, PHOTO_MAX_FILE_BYTES };`)();

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
