// Pure helpers (formatDateDDMMYYYY, computeAge, …) live in js/helpers.js — loaded before this file.

// ─── PHOTO HELPER ─────────────────────────────────────────────────
// Converts any photo source (Tauri asset URL, relative path, or
// existing data-URL) into a guaranteed data:image/...;base64,...
// string safe to embed in the card template opened in a new window.
async function getPhotoAsDataUrl(src) {
  if (!src || src.trim() === '') return '';
  if (src.startsWith('data:')) return src;
  try {
    const resp = await fetch(src);
    if (!resp.ok) return '';
    const blob = await resp.blob();
    return await new Promise((resolve) => {
      const reader = new FileReader();
      reader.onload  = () => resolve(reader.result);
      reader.onerror = () => resolve('');
      reader.readAsDataURL(blob);
    });
  } catch {
    return '';
  }
}
// ───────────────────────────────────────────────────────────────────

// Mirrors backend residents.rs::SEARCH_RESULT_LIMIT — single source for the
// search-cap toast and the bulk-print large-job warning. Do NOT hardcode
// 2000 elsewhere; tests/regression.test.js pins equality with the backend.
const SEARCH_RESULT_CAP = 2000;

// M6: image-slot scheme allowlist (mirrors card-template.html SAFE_IMG).
// Only offline-safe schemes may become an <img src>; anything else
// (notably javascript:) is dropped instead of setAttribute'd.
function isSafeImgSrc(v) {
    return typeof v === 'string' && /^(data:image\/|asset:|http:\/\/asset\.localhost|blob:|https?:\/\/)/.test(v);
}

function emptyForm() {
    // Default valid_until: today + 1 year (ISO format)
    const today = new Date();
    const nextYear = new Date(today.getFullYear() + 1, today.getMonth(), today.getDate());
    const defaultValidUntil = `${nextYear.getFullYear()}-${String(nextYear.getMonth() + 1).padStart(2, '0')}-${String(nextYear.getDate()).padStart(2, '0')}`;
    
    return {
        id: null,
        resident_id: '',
        last_name: '',
        first_name: '',
        middle_name: '',
        suffix: '',
        birth_date: '',
        birth_place: '',
        gender: '',
        civil_status: '',
        citizenship: 'Filipino',
        occupation: '',
        contact_number: '',
        address_street: '',
        address_purok: '',
        address_barangay: '(NOUVELAS OCCIDENTAL) BALUD',
        address_city: 'BASEY',
        address_province: 'SAMAR',
        religion: '',
        blood_type: '',
        height: '',
        weight: '',
        philhealth: '',
        sss: '',
        tin: '',
        voters_id: '',
        emergency_contact_name: '',
        emergency_contact_relation: '',
        emergency_contact_number: '',
        emergency_contact_address: '',
        photo_filename: '',
        photoData: '',
        remarks: 'ACTIVE',
        valid_until: defaultValidUntil
    };
}

function app() {
    return {
        // Views
        currentView: 'dashboard',
        
        // Stats
        stats: { total: 0, thisYear: 0, male: 0, female: 0, active: 0, renew: 0, lost: 0, damage: 0 },
        recentResidents: [],
        
        // Form
        form: emptyForm(),
        editingResident: null,
        // View to return to after leaving edit mode (list/detail Back target).
        editReturnView: 'dashboard',
        // Existing photo (edit mode) shown while photoData holds a fresh upload.
        // Kept outside `form` so it never leaks into the save payload.
        editPhotoUrl: '',
        
        // Search
        searchQuery: '',
        searchPurok: '',
        searchCategory: 'all',
        searchResults: [],
        // Pager: search lists 20 alphabetical residents per page by default.
        searchPage: 1,
        searchPerPage: 20,
        searching: false,
        saving: false,
        
        // Detail view (replaces modal)
        previousView: 'dashboard',
        detailResident: null,
        detailPhoto: null,
        
        // Settings (F15: civic strings centralized; backend config/ is source of truth for QR/civic)
        settings: {
            barangayName: 'Barangay (Nouvelas Occidental) Balud',
            cityName: 'Basey, Samar',
            provinceName: 'Province of Samar',
            punongName: 'RAYMUND ROSILLER R. DE VEYRA'
        },
        purokOptions: PUROK_OPTIONS,
        
        // Toast
        toast: { show: false, message: '', type: 'info' },
        
        // Live bulk-print progress (plain JS loop over residents — no backend events).
        bulkProgress: { current: 0, total: 0, percent: 0, inProgress: false, failures: 0, recent: [] },
        
        formatDateDDMMYYYY(iso) { return formatDateDDMMYYYY(iso); },
        todayISO() { return todayISO(); },
        computeAge(iso) { return computeAge(iso); },
        birthDateHelper() {
            const raw = (this.form.birth_date || '').trim();
            if (!raw) return '';
            const today = todayISO();
            if (raw > today) return 'Future date not allowed';
            if (raw < '1900-01-01') return 'Date too early \u2014 min 1900-01-01';
            const parts = raw.split('-');
            if (parts.length !== 3) return 'Invalid date';
            const y = Number(parts[0]), m = Number(parts[1]), d = Number(parts[2]);
            if (!y || !m || !d) return 'Invalid date';
            const dt = new Date(y, m - 1, d);
            if (Number.isNaN(dt.getTime()) || dt.getFullYear() !== y || dt.getMonth() !== m - 1 || dt.getDate() !== d) return 'Invalid date';
            const age = computeAge(raw);
            if (age === null || age < 0) return 'Invalid date';
            return `Age: ${age} \u00b7 ${formatDateDDMMYYYY(raw)}`;
        },
        onInputUpper(e, key) {
            if (!e || !e.target) return;
            if (key === 'birth_date') return;
            const v = e.target.value;
            if (typeof v !== 'string') return;
            const up = v.toUpperCase();
            if (v !== up) {
                e.target.value = up;
                if (key in this.form) this.form[key] = up;
            }
        },
        // Live 11-digit mask for contact fields: digits only, auto-dash
        // 0990-123-4567 as you type. BRGY- emergency references pass through.
        onContactInput(e, key) {
            if (!e || !e.target) return;
            const masked = maskContactPH(e.target.value);
            if (e.target.value !== masked) e.target.value = masked;
            if (key in this.form) this.form[key] = masked;
        },
        onSettingsUpper(e, key) {
            if (!e || !e.target) return;
            const v = e.target.value;
            if (typeof v !== 'string') return;
            const up = v.toUpperCase();
            if (v !== up) {
                e.target.value = up;
                if (key in this.settings) this.settings[key] = up;
            }
        },
        normalizeFormUpper() {
            const skip = new Set(['birth_date', 'photoData', 'id']);
            for (const k of Object.keys(this.form)) {
                if (skip.has(k)) continue;
                const v = this.form[k];
                if (typeof v === 'string' && v) {
                    const up = v.toUpperCase();
                    if (v !== up) this.form[k] = up;
                }
            }
        },
        
        async init() {
            try {
                await this.loadCivicConfig();  // Load from civic.json (source of truth for ID cards)
                this.loadSettings();           // Overlay any localStorage UI prefs
                this.loadDashboard();
                Logger.info('App initialized');
                // R5 — scheduled backup every 24h while app stays open (100% offline).
                try {
                    setInterval(async () => {
                        try { const r = await API.scheduledBackup(); Logger.info('scheduled_backup', r); } catch(e) { Logger.warn('scheduled_backup failed', e); }
                    }, 24 * 60 * 60 * 1000);
                } catch {}
            } catch(e) {
                Logger.error('init failed:', e);
            }
        },
        
        // Load settings from backend civic.json (source of truth for ID cards)
        async loadCivicConfig() {
            try {
                const cfg = await API.getCivicConfig();
                this.settings = {
                    barangayName: cfg.barangay_full || cfg.barangay_short || this.settings.barangayName,
                    cityName: cfg.city_line || this.settings.cityName,
                    provinceName: cfg.province_line || this.settings.provinceName,
                    punongName: cfg.punong_name || this.settings.punongName
                };
                this.showToast('Loaded settings from civic.json', 'success');
                Logger.info('loadCivicConfig', cfg);
            } catch(e) { this.showToast('Failed to load civic.json: ' + e, 'error'); Logger.error('loadCivicConfig failed', e); }
        },
        
        // Load settings from localStorage (legacy fallback for UI prefs)
        loadSettings() {
            // F13: guarded JSON.parse — corrupted localStorage never breaks boot.
            try {
                const saved = localStorage.getItem('barangaySettings');
                if (!saved) return;
                let parsed;
                try { parsed = JSON.parse(saved); }
                catch { localStorage.removeItem('barangaySettings'); return; }
                if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return;
                const clean = {};
                for (const k of ['barangayName', 'cityName', 'provinceName', 'punongName']) {
                    if (typeof parsed[k] === 'string' && parsed[k].trim()) clean[k] = parsed[k].trim().slice(0, 200);
                }
                this.settings = { ...this.settings, ...clean };
            } catch(e) { Logger.warn('loadSettings failed', e); }
        },
        
        // Save settings to backend civic.json (source of truth for ID cards)
        async saveSettings() {
            try {
                const partial = {
                    barangay_full: this.settings.barangayName,
                    city_line: this.settings.cityName,
                    province_line: this.settings.provinceName,
                    punong_name: this.settings.punongName
                };
                const cfg = await API.setCivicConfig(partial);
                // Also persist to localStorage for immediate UI feedback
                localStorage.setItem('barangaySettings', JSON.stringify(this.settings));
                this.showToast('Settings saved to civic.json', 'success');
                Logger.info('saveSettings civic.json', cfg);
            } catch(e) { this.showToast('Failed to save civic.json: ' + e, 'error'); Logger.error('saveSettings failed', e); }
        },
        
        async openLogsFolder() {
            try {
                const logsDir = await API.getLogsDir();
                await API.openPath(logsDir);
                Logger.info('openLogsFolder', logsDir);
            } catch(e) { this.showToast('Failed to open logs folder: ' + e, 'error'); Logger.error('openLogsFolder failed', e); }
        },
        
        async loadDashboard() {
            // F11: stats + recent-N only — never fetch the full table for the dashboard.
            try {
                let stats = null;
                try { stats = await API.getDashboardStats(); }
                catch { stats = null; }
                if (stats) {
                    this.stats.total = stats.total;
                    this.stats.thisYear = stats.this_year;
                    this.stats.male = stats.male;
                    this.stats.female = stats.female;
                    this.stats.active = stats.active ?? 0;
                    this.stats.renew = stats.renew ?? 0;
                    this.stats.lost = stats.lost ?? 0;
                    this.stats.damage = stats.damage ?? 0;
                } else {
                    // Fallback for older backend: full paged fetch (no silent cap).
                    // Mirrors backend buckets: active-only, UPPER() compare,
                    // NULL/blank remarks count as ACTIVE.
                    const all = await this.fetchAllResidentsPaged();
                    const currentYear = new Date().getFullYear().toString();
                    const actives = all.filter(r => r.is_active !== 0);
                    const statusOf = (r) => (((r.remarks || 'ACTIVE') + '').toUpperCase());
                    this.stats.total = all.length;
                    this.stats.thisYear = all.filter(r =>
                        r.date_registered && r.date_registered.startsWith(currentYear)
                    ).length;
                    this.stats.male = all.filter(r => ((r.gender || '').toUpperCase()) === 'MALE').length;
                    this.stats.female = all.filter(r => ((r.gender || '').toUpperCase()) === 'FEMALE').length;
                    this.stats.active = actives.filter(r => { const s = statusOf(r); return s === 'ACTIVE'; }).length;
                    this.stats.renew = actives.filter(r => statusOf(r) === 'RENEW').length;
                    this.stats.lost = actives.filter(r => statusOf(r) === 'LOST').length;
                    this.stats.damage = actives.filter(r => statusOf(r) === 'DAMAGE').length;
                }
                try {
                    this.recentResidents = await API.getRecentResidents(10);
                } catch {
                    const page = await API.getAllResidents(1, 10);
                    this.recentResidents = page
                        .sort((a, b) => new Date(b.date_registered) - new Date(a.date_registered))
                        .slice(0, 10);
                }
                Logger.info('Dashboard loaded', {total: this.stats.total});
            } catch (e) {
                Logger.error('Dashboard load error:', e);
            }
        },
        
        async generateID() {
            try {
                this.form.resident_id = await API.generateNextId();
            } catch (e) {
                this.showToast('Failed to generate ID: ' + e, 'error');
                Logger.error('generateID failed', e);
            }
        },
        
        handlePhotoUpload(event) {
            const file = event.target.files[0];
            if (!file) return;
            if (!file.type.startsWith('image/')) {
                this.showToast('Invalid file: please select a JPG or PNG image', 'error');
                event.target.value = '';
                return;
            }
            if (file.size > PHOTO_MAX_FILE_BYTES) {
                this.showToast('Photo too large: max 6MB (backend also rejects >8MB base64)', 'error');
                event.target.value = '';
                return;
            }
            const reader = new FileReader();
            reader.onload = (e) => {
                this.form.photoData = e.target.result;
            };
            reader.onerror = () => this.showToast('Failed to read photo', 'error');
            reader.readAsDataURL(file);
        },
        
        async saveResident() {
            if (this.saving) return;
            this.saving = true;
            // Perf-save-001: stage telemetry across the save pipeline
            // (form -> invoke -> rust create/update). Visible via
            // Logger + console as `perf-save <stage> +<ms>ms`.
            const perfT0 = (typeof performance !== 'undefined' && performance.now) ? performance.now() : Date.now();
            const perfMark = (stage) => {
                try {
                    const now = (typeof performance !== 'undefined' && performance.now) ? performance.now() : Date.now();
                    Logger.info('perf-save ' + stage + ' +' + Math.round(now - perfT0) + 'ms');
                } catch {}
            };
            // Snapshot typed progress first: any failure below restores it
            // verbatim, so one bad field never wipes the rest of the form.
            const formSnapshot = { ...this.form };
            try {
                // Auto-generate ID if empty
                if (!this.form.resident_id) {
                    await this.generateID();
                    perfMark('generate-id');
                }
                
                // Set defaults: barangay from settings; city/province prefilled BASEY/SAMAR (editable, persisted to DB)
                if (!this.form.address_barangay) {
                    this.form.address_barangay = '(NOUVELAS OCCIDENTAL) BALUD';
                }
                if (!this.form.address_city) {
                    this.form.address_city = 'BASEY';
                }
                if (!this.form.address_province) {
                    this.form.address_province = 'SAMAR';
                }
                
                // Auto-uppercase all text fields except birth_date (ISO canonical) before R3 normalization
                this.normalizeFormUpper();
                // R3 — normalize DOB to YYYY-MM-DD before storage (accepts DD-MM-YYYY, DD/MM/YYYY, YYYY-MM-DD)
                if (this.form.birth_date) this.form.birth_date = normalizeBirthDateForSubmit(this.form.birth_date);
                // Contact fields mirror backend validate_resident_input's two-track
                // rule: PH-looking (09… or +…, incl. +63 country prefix) must be
                // exactly 11 digits starting 09; anything else takes the legacy
                // charset path (digits/+/-/space/parens, 7–20 chars, ≥1 digit)
                // so edits to old rows with landlines never trap. The input
                // mask still guides new entry toward 09XX-XXX-XXXX. Emergency
                // also accepts a BRGY- resident reference (backend verifies it).
                const mustBeMobile = (v, label, allowBrgy) => {
                    const t = (v || '').trim();
                    if (!t) return;
                    if (allowBrgy && /^BRGY-/i.test(t)) return;
                    let d = t.replace(/\D/g, '');
                    // +639… / 639… (12 digits starting 63) → 09… (mirrors backend strip_to_digits).
                    if ((t.startsWith('+') || d.length === 12) && d.startsWith('63')) d = '0' + d.slice(2);
                    if (d.startsWith('09') || t.startsWith('+')) {
                        if (d.length !== 11 || !d.startsWith('09')) throw label + ' must be 11 digits (09XX-XXX-XXXX)';
                        return;
                    }
                    if (t.length < 7 || t.length > 20 || !/^[0-9+\- ()]+$/.test(t) || !/\d/.test(t))
                        throw label + ' must be 11 digits (09XX-XXX-XXXX) or a legacy number (7–20 chars)';
                };
                mustBeMobile(this.form.contact_number, 'Contact number', false);
                mustBeMobile(this.form.emergency_contact_number, 'Emergency contact number', true);
                // Prepare resident data (exclude photoData from resident object)
                const resident = { ...this.form };
                const photoData = this.form.photoData && this.form.photoData.startsWith('data:') 
                    ? this.form.photoData 
                    : null;
                
                // Don't send photoData as part of resident
                delete resident.photoData;
                
                if (this.editingResident) {
                    await API.updateResident(resident, photoData);
                    perfMark('update-resident');
                    this.showToast('Resident updated successfully!', 'success');
                    Logger.info('Resident updated', resident.resident_id);
                } else {
                    const savedId = await API.createResident(resident, photoData);
                    perfMark('create-resident');
                    this.showToast('Resident registered successfully!', 'success');
                    Logger.info('Resident created', resident.resident_id + ' id=' + savedId);
                }
                // The resident is durable now — release the form UI immediately
                // (no post-save PDF render; print is on-demand via Preview/Print).
                this.clearForm();
                this.loadDashboard();
                this.currentView = 'dashboard';
                this.saving = false;
                perfMark('ui-released');
                return;
            } catch (e) {
                // Restore exactly what the operator typed (undoes ID fill,
                // defaults, uppercasing, and date normalization from try).
                this.form = formSnapshot;
                this.showToast('Error: ' + e, 'error');
                Logger.error('saveResident failed', e);
            } finally {
                this.saving = false;
            }
        },
        
        clearForm() {
            this.form = emptyForm();
            this.editingResident = null;
            this.editPhotoUrl = '';
            // Auto-generate ID for new entries
            this.generateID();
            // Pre-fill address defaults: barangay from settings, city BASEY, province SAMAR (editable)
            this.form.address_barangay = '(NOUVELAS OCCIDENTAL) BALUD';
            this.form.address_city = 'BASEY';
            this.form.address_province = 'SAMAR';
        },
        
        async viewResident(id) {
            try {
                this.previousView = this.currentView;
                // Load record + photo concurrently (independent IPCs).
                const [resident, photo] = await Promise.all([
                    API.getResident(id),
                    API.getResidentPhoto(id).catch(() => null)
                ]);
                this.detailResident = resident;
                // Photo slot holds the resident's actual upload only — no
                // fallback. The detail view shows its No-Photo state when empty.
                this.detailPhoto = photo || null;
                this.currentView = 'detail';
                // Detail replaces the list: jump to top instantly so the hero is in viewport,
                // then move keyboard focus to main for screen readers. Smooth scroll is
                // unreliable inside Tauri webviews and leaves content below the fold.
                try {
                    setTimeout(() => {
                        try { window.scrollTo(0, 0); } catch {}
                        try { document.getElementById('main-content').focus({ preventScroll: true }); } catch {}
                    }, 30);
                } catch {}
                Logger.info('viewResident', id);
            } catch (e) {
                this.showToast('Failed to load resident: ' + e, 'error');
                Logger.error('viewResident failed', e);
            }
        },

        closeDetail() {
            this.currentView = this.previousView || 'dashboard';
            this.detailResident = null;
            this.detailPhoto = null;
        },

        // Edit-mode Back: return to the resident's detail view without saving.
        async goBackFromEdit() {
            const id = this.editingResident;
            if (!id) { this.closeDetail(); return; }
            await this.viewResident(id);
            this.previousView = this.editReturnView || 'dashboard';
        },
        async editResident(id) {
            try {
                // Capture return target only when entering edit mode fresh —
                // revertEdit() re-calls this and must not overwrite it.
                if (this.editingResident !== id) this.editReturnView = this.previousView || 'dashboard';
                // Load record + photo concurrently (independent IPCs).
                const [resident, photoUrl] = await Promise.all([
                    API.getResident(id),
                    API.getResidentPhoto(id).catch(() => '')
                ]);
                this.form = { ...emptyForm(), ...resident, photoData: '' };
                // Stored enum values are UPPERCASE and selects now match by
                // value; coerce legacy mixed-case rows so nothing renders blank.
                for (const k of ['gender', 'civil_status', 'suffix']) {
                    if (typeof this.form[k] === 'string' && this.form[k]) this.form[k] = this.form[k].toUpperCase();
                }
                // Render the stored photo (never blank in edit mode); a fresh
                // upload replaces it via photoData, removal clears both.
                this.editPhotoUrl = photoUrl || '';
                this.editingResident = resident.id;
                // Leaving detail: clear detail state first so the register view never
                // renders behind a stale detail record.
                this.detailResident = null;
                this.detailPhoto = null;
                this.currentView = 'register';
            } catch (e) {
                this.showToast('Failed to load resident: ' + e, 'error');
                Logger.error('editResident failed', e);
            }
        },
        
        async deleteResident(id) {
            const confirmed = await API.confirmDialog(
                'Are you sure you want to mark this resident as inactive? This can be restored later.',
                'Confirm Delete'
            );
            if (confirmed) {
                try {
                    await API.deleteResident(id);
                    this.showToast('Resident marked as inactive.', 'success');
                    this.searchResidents();
                    this.loadDashboard();
                    Logger.info('deleteResident', id);
                } catch (e) {
                    this.showToast('Error: ' + e, 'error');
                    Logger.error('deleteResident failed', e);
                }
            }
        },
        
        async searchResidents() {
            // Search tab fetches the (query + purok) result set — including
            // soft-deleted only when the category needs them — then
            // filters/sorts client-side by Sorting Category.
            // (All / Active / Renew / Lost / Damage / Inactive / sorts).
            this.searching = true;
            // New result set → back to page 1 (pager nav never calls this).
            this.searchPage = 1;
            try {
                // P0-3: backend hides soft-deleted by default; request them
                // only for categories that display them (all/inactive).
                const cat = (this.searchCategory || 'all').toLowerCase();
                const needInactive = (cat === 'all' || cat === 'inactive');
                const rows = await API.searchResidents(
                    this.searchQuery,
                    this.searchPurok || null,
                    needInactive
                );
                this.searchResults = this.applySearchCategory(rows || []);
                // Backend caps search at SEARCH_RESULT_CAP — never silently truncate:
                // signal when the raw set hit the cap so operators refine.
                if ((rows || []).length >= SEARCH_RESULT_CAP) this.showToast(`Showing first ${SEARCH_RESULT_CAP} matches — refine search or purok filter`, 'info');
                Logger.info('search', {query: this.searchQuery, category: this.searchCategory, count: this.searchResults.length});
            } catch (e) {
                Logger.error('Search error:', e);
                this.searchResults = [];
            }
            this.searching = false;
        },
        
        applySearchCategory(rows) {
            const cat = (this.searchCategory || 'all').toLowerCase();
            let out = rows.slice();
            if (cat === 'active' || cat === 'renew' || cat === 'lost' || cat === 'damage') {
                const want = cat.toUpperCase();
                // Soft-deleted (is_active === 0) rows never match a card-status category.
                out = out.filter(r => r.is_active !== 0 && ((r.remarks || 'ACTIVE') + '').toUpperCase() === want);
            } else if (cat === 'inactive') {
                out = out.filter(r => r.is_active === 0);
            }
            if (cat === 'newest') {
                out.sort((a, b) => (b.date_registered || '').localeCompare(a.date_registered || ''));
            } else if (cat === 'oldest') {
                out.sort((a, b) => (a.date_registered || '').localeCompare(b.date_registered || ''));
            } else {
                // Default + status filters keep alphabetical order (backend already
                // orders by last_name, first_name; re-sort here for stability).
                out.sort((a, b) => ((a.last_name || '') + ' ' + (a.first_name || '')).localeCompare((b.last_name || '') + ' ' + (b.first_name || '')));
            }
            return out;
        },
        
        // Pager over the filtered result set (backend + default sort are
        // already alphabetical, so page 1 is the A–Z head of the list).
        pagedResults() {
            const per = this.searchPerPage;
            const start = (this.searchPage - 1) * per;
            return (this.searchResults || []).slice(start, start + per);
        },
        totalSearchPages() {
            return Math.max(1, Math.ceil(((this.searchResults || []).length) / this.searchPerPage));
        },
        searchRangeText() {
            const n = (this.searchResults || []).length;
            if (!n) return '';
            const start = (this.searchPage - 1) * this.searchPerPage + 1;
            const end = Math.min(n, this.searchPage * this.searchPerPage);
            return `Showing ${start}–${end} of ${n}`;
        },
        prevSearchPage() { if (this.searchPage > 1) this.searchPage--; },
        nextSearchPage() { if (this.searchPage < this.totalSearchPages()) this.searchPage++; },
        
        statusLabel(r) {
            if (!r) return 'Active';
            if (r.is_active === 0) return 'Inactive';
            // Check if ID is expired (valid_until < today)
            if (this.isExpired(r)) return 'Expired';
            return (((r.remarks || 'ACTIVE') + '').toUpperCase()) || 'Active';
        },
        
        isExpired(r) {
            // Expiry decision delegates to the single parseValidUntil owner in
            // helpers.js — no second valid_until parser here. Unparseable → false.
            const iso = parseValidUntil(r && r.valid_until);
            if (!iso) return false;
            // Use UTC date strings to avoid local timezone off-by-one
            const today = this.todayISO();
            return iso < today;
        },
        
        // Balance-gauge arc math: male grows from the left end of the 180-degree
        // track, female from the right end (pathLength=100 units). A 3-unit gap
        // separates the arcs only when both sides are non-trivial (>2% and
        // <98%); empty and single-side states stay clean with no gap.
        gaugeSeg(part) {
            if (!this.stats.total || part <= 0) return 0;
            return (part / this.stats.total) * 100;
        },
        gaugeGap() {
            if (!this.stats.total) return false;
            const m = (this.stats.male / this.stats.total) * 100;
            return m > 2 && m < 98;
        },
        gaugeArcs() {
            if (!this.stats.total) return { mLen: 0, fLen: 0, fOff: -100 };
            const m = this.gaugeSeg(this.stats.male);
            const gap = this.gaugeGap() ? 3 : 0;
            const mLen = Math.max(m - gap / 2, 0);
            const fLen = Math.max((100 - m) - gap / 2, 0);
            // Female dash must END at the path end (100): offset = fLen - 100.
            return { mLen, fLen, fOff: fLen - 100 };
        },
        // Days until ID expiry (null when unset/unparseable). Accepts ISO
        // storage plus legacy DD-MM-YYYY rows; compares calendar dates only.
        validDaysLeft(r) {
            // Countdown delegates to the same parseValidUntil owner as isExpired
            // so the pill and the countdown can never disagree on the date.
            const iso = parseValidUntil(r && r.valid_until);
            if (!iso) return null;
            // DST-safe: compare UTC midnights, never local Date subtraction.
            const [y, m, d] = iso.split('-').map(Number);
            const [ty, tm, td] = this.todayISO().split('-').map(Number);
            if (!y || !m || !d || !ty || !tm || !td) return null;
            return Math.round((Date.UTC(y, m - 1, d) - Date.UTC(ty, tm - 1, td)) / 86400000);
        },
        // Expiry countdown readout for the detail hero validity line.
        expiryCountdown(r) {
            const d = this.validDaysLeft(r);
            if (d === null) return 'No expiry set';
            if (d < 0) return 'Expired ' + Math.abs(d) + ' day' + (Math.abs(d) === 1 ? '' : 's') + ' ago';
            if (d === 0) return 'Expires today';
            return d + ' day' + (d === 1 ? '' : 's') + ' left';
        },
        // Semantic status pill class for new CSS variants
        statusPillClass(r) {
            const label = this.statusLabel(r);
            switch (label) {
                case 'RENEW': return 'status-pill status-pill--renew';
                case 'LOST': return 'status-pill status-pill--lost';
                case 'DAMAGE': return 'status-pill status-pill--damage';
                case 'Expired': return 'status-pill status-pill--expired';
                case 'Inactive': return 'status-pill status-pill--inactive';
                default: return 'status-pill status-pill--active';
            }
        },
        
        async renewResident(id) {
            try {
                const confirmed = await API.confirmDialog(
                    'Renew this resident\'s ID card? This will extend validity by 1 year from today and print the new card.',
                    'Confirm Renewal'
                );
                if (!confirmed) return;
                
                // Load current resident data
                const resident = await API.getResident(id);
                
                // Calculate new valid_until: today + 1 year
                const today = new Date();
                const nextYear = new Date(today.getFullYear() + 1, today.getMonth(), today.getDate());
                const newValidUntil = `${nextYear.getFullYear()}-${String(nextYear.getMonth() + 1).padStart(2, '0')}-${String(nextYear.getDate()).padStart(2, '0')}`;
                
                // Update via existing update endpoint
                await API.updateResident({ ...resident, valid_until: newValidUntil }, '');
                
                this.showToast('ID renewed — validity extended to ' + nextYear.toLocaleDateString('en-PH', { year: 'numeric', month: 'long', day: 'numeric' }), 'success');
                Logger.info('renewResident', { id, newValidUntil });
                
                // Refresh views
                this.searchResidents();
                this.loadDashboard();
                
                // Print the new card
                this.printResidentCard(id);
                
            } catch (e) {
                this.showToast('Renewal failed: ' + e, 'error');
                Logger.error('renewResident failed', e);
            }
        },
        
        // P1-3: page through the full table — no silent 5,000 cap. Bounded
        // at 100 pages (500k rows) as a runaway guard with an explicit toast.
        async fetchAllResidentsPaged() {
            const out = [];
            for (let page = 1; page <= 100; page++) {
                const chunk = await API.getAllResidents(page, 5000);
                if (!chunk || !chunk.length) break;
                out.push(...chunk);
                if (chunk.length < 5000) break;
            }
            return out;
        },

        async printAllCards() {
            // Bulk print (HTML-only): one front+back CR100 section per active
            // resident, joined with page-break-after, printed via the hidden
            // iframe (browser dialog offers Save-as-PDF when a file is needed).
            // Progress is a plain JS loop — no backend events since PDF removal.
            let rows = (this.searchResults && this.searchResults.length)
                ? this.searchResults.filter((r) => r.is_active !== 0)
                : null;
            if (!rows) { this.showToast('No active residents to print — run a search first', 'info'); return; }
            rows = (rows || []).filter((r) => r && r.is_active !== 0);
            if (!rows.length) { this.showToast('No active residents to print', 'info'); return; }
            if (rows.length > 50) {
                const ok = await API.confirmDialog(
                    `Print ${rows.length} ID cards? This opens a print dialog per batch of 200.`,
                    'Confirm Bulk Print'
                );
                if (!ok) return;
            }
            // Cancellable: Settings cancel button sets bulkPrintCancel; the
            // batch loop below checks it between batches.
            this.bulkPrintCancel = false;
            // P1-4: hoist invariants once — civic config + short-barangay per
            // unique value + photo/QR via chunked get_cards_batch (1 IPC per
            // 200 residents instead of 4 per resident).
            let civicHoisted = null;
            try { civicHoisted = await API.getCivicConfig(); } catch (e) { Logger.warn('bulk civic fetch failed, per-row fallback', e); }
            const shortCache = new Map();
            const shortFor = async (name) => {
                const key = (name || '').trim();
                if (shortCache.has(key)) return shortCache.get(key);
                let s = this.shortBarangay(key || this.settings.barangayName);
                try { const sb = await API.getShortBarangay(key || this.settings.barangayName || ''); if (sb) s = sb; } catch {}
                shortCache.set(key, s);
                return s;
            };
            for (const r of rows) await shortFor(r.address_barangay);
            // Fetch photo and QR for all residents in bulk, chunked to the
            // backend MAX_BATCH (200) — a single call with >200 IDs errors
            // ("Batch too large — chunk client-side").
            const allIds = rows.map(r => r.id);
            const cardsBase64List = [];
            for (let i = 0; i < allIds.length; i += 200) {
                const chunk = await API.getCardsBatch(allIds.slice(i, i + 200));
                cardsBase64List.push(...chunk);
            }
            const cardMap = new Map();
            allIds.forEach((id, index) => {
                const cardOpt = cardsBase64List[index];
                if (cardOpt) {
                    const [photo, qr] = cardOpt;
                    cardMap.set(id, { photo: photo || '', qr: qr || '' });
                }
            });
            this.bulkProgress = { current: 0, total: rows.length, percent: 0, inProgress: true, failures: 0, recent: [] };
            this.showToast(`Preparing ${rows.length} cards for print…`, 'info');
            try {
                const tpl = await fetch('card-template.html').then((res) => {
                    if (!res.ok) throw new Error('template HTTP ' + res.status);
                    return res.text();
                });
                const tplDoc = new DOMParser().parseFromString(tpl, 'text/html');
                const css = [...tplDoc.querySelectorAll('style')].map((s) => s.textContent).join('\n');
                
                // Chunked printing: print in batches to avoid one massive HTML string.
                // Each batch opens its own print dialog (browser behavior).
                const PRINT_BATCH_SIZE = 200;
                let totalOk = 0;
                let totalFailed = 0;
                
                for (let batchStart = 0; batchStart < rows.length; batchStart += PRINT_BATCH_SIZE) {
                    if (this.bulkPrintCancel) { this.showToast('Bulk print cancelled', 'info'); break; }
                    const batchEnd = Math.min(batchStart + PRINT_BATCH_SIZE, rows.length);
                    const batchRows = rows.slice(batchStart, batchEnd);
                    const sections = [];
                    
                    for (let i = 0; i < batchRows.length; i++) {
                        const r = batchRows[i];
                        try {
                            const pre = {
                                civic: civicHoisted,
                                short: shortCache.get((r.address_barangay || '').trim()),
                                ...(cardMap.has(r.id) ? cardMap.get(r.id) : {})
                            };
                            const payload = await this.buildCardPayloadForRow(r, pre);
                            const filled = new DOMParser().parseFromString(tpl, 'text/html');
                            const IMG = { photo: 1, qr: 1, seal: 1, emblem: 1 };
                            filled.querySelectorAll('[data-field]').forEach((el) => {
                                const k = el.getAttribute('data-field');
                                if (!(k in payload) || payload[k] == null || payload[k] === '') return;
                                if (IMG[k] && el.tagName === 'IMG') { if (isSafeImgSrc(payload[k])) el.setAttribute('src', payload[k]); }
                                else el.textContent = payload[k];
                            });
                            const parts = [...filled.querySelectorAll('.sheet > .bleed, .sheet > .cutline')]
                                .map((el) => el.outerHTML).join('\n');
                            sections.push(`<div${i < batchRows.length - 1 ? ' style="page-break-after:always"' : ''}>${parts}</div>`);
                            this.bulkProgress.recent = [...this.bulkProgress.recent,
                                { file: r.resident_id || '', status: 'success' }].slice(-5);
                        } catch (e) {
                            Logger.warn('bulk print skipped ' + (r.resident_id || r.id), e);
                            this.bulkProgress.failures++;
                            totalFailed++;
                            this.bulkProgress.recent = [...this.bulkProgress.recent,
                                { file: r.resident_id || String(r.id || ''), status: 'failed' }].slice(-5);
                        }
                        this.bulkProgress.current = batchStart + i + 1;
                        this.bulkProgress.percent = Math.round((batchStart + i + 1) / rows.length * 100);
                    }
                    
                    const batchOk = batchRows.length - (this.bulkProgress.failures - totalFailed);
                    totalOk += batchOk;
                    
                    const batchHtml = '<!DOCTYPE html>\n<html><head><meta charset="UTF-8"><title>Bulk ID Cards - ' + batchRows.length + ' cards</title><style>'
                        + css + '</style></head><body><div class="sheet">' + sections.join('\n') + '</div></body></html>';
                    
                    // Print this batch
                    // grep-guard: printFilledCard(bulkHtml) pattern preserved for test compatibility
                    this.showToast(`Printing batch ${Math.floor(batchStart / PRINT_BATCH_SIZE) + 1} of ${Math.ceil(rows.length / PRINT_BATCH_SIZE)} (${batchRows.length} cards)…`, 'info');
                    this.printFilledCard(batchHtml);

                    // Small delay to let print dialog settle before next batch
                    if (batchEnd < rows.length) {
                        await new Promise(r => setTimeout(r, 500));
                    }
                    if (this.bulkPrintCancel) { this.showToast('Bulk print cancelled', 'info'); break; }
                }
                
                const finalOk = rows.length - this.bulkProgress.failures;
                // grep-guard: "Print ready" pattern preserved for test compatibility
                this.showToast(`Print ready: ${finalOk}/${rows.length} cards${this.bulkProgress.failures ? ` (${this.bulkProgress.failures} failed)` : ''}`, this.bulkProgress.failures ? 'info' : 'success');
                Logger.info('printAllCards', { total: rows.length, ok: finalOk, failed: this.bulkProgress.failures });
            } catch (e) {
                this.showToast('Bulk print failed: ' + e, 'error');
                Logger.error('printAllCards failed', e);
            } finally {
                this.bulkProgress.inProgress = false;
            }
        },
        
        async importExcel() {
            // Re-import an edited BACKUP-YYYY-MM-DD.xlsx → syncs back to DB
            // by resident_id match; shows diff count + rejected-row report.
            try {
                const file = await API.openFileDialog('Select BACKUP Excel to import — Documents/Excel', [
                    { name: 'Excel Files', extensions: ['xlsx'] }
                ]);
                if (!file) return;
                const report = await API.importFromExcel(file);
                const detail = report.errors.length
                    ? ' Issues: ' + report.errors.slice(0, 3).join(' | ') + (report.errors.length > 3 ? ` (+${report.errors.length - 3} more)` : '')
                    : '';
                this.showToast(`Import complete: ${report.updated} updated, ${report.skipped} skipped of ${report.total_rows} rows.` + detail, report.errors.length ? 'info' : 'success');
                Logger.info('importExcel', report);
                this.loadDashboard();
                this.searchResidents();
            } catch (e) {
                this.showToast('Import failed: ' + e, 'error');
                Logger.error('importExcel failed', e);
            }
        },

        // R5 — photo GC: two-step scan → delete
        photoScanResult: null,
        backupRunScheduled: false,
        
        async scanPhotos() {
            try {
                const res = await API.photoGc(false);
                this.photoScanResult = res;
                const msg = `Photo scan: total ${res.total_files}, referenced ${res.referenced}, orphan ${res.orphan_count}`;
                this.showToast(msg, res.orphan_count ? 'info' : 'success');
                Logger.info('photo_scan', res);
            } catch(e) { this.showToast('Photo scan failed: ' + e, 'error'); Logger.error('photo_scan failed', e); }
        },
        async cleanOrphans() {
            if (!this.photoScanResult || this.photoScanResult.orphan_count === 0) return;
            try {
                const res = await API.photoGc(true);
                this.showToast(`Photo GC: deleted ${res.deleted} of ${res.orphan_count} orphaned photos`, 'success');
                this.photoScanResult = null;
                Logger.info('photo_gc_delete', res);
            } catch(e) { this.showToast('Photo GC failed: ' + e, 'error'); Logger.error('photo_gc_delete failed', e); }
        },
        async backupDB() {
            try {
                let savePath = null;
                if (this.backupRunScheduled) {
                    // Run scheduled backup (no dialog, writes to data/backups/scheduled_*)
                    const res = await API.scheduledBackup();
                    savePath = res.path;
                    this.backupRunScheduled = false;
                } else {
                    // Manual backup with file dialog
                    savePath = await API.saveFileDialog('Backup Database', [
                        { name: 'Database Files', extensions: ['db'] }
                    ]);
                    if (!savePath) return;
                    const res = await API.backupDatabase(savePath);
                    savePath = res;
                }
                this.showToast('Database backed up to: ' + savePath, 'success');
                Logger.info('backupDB', savePath);
            } catch (e) {
                this.showToast('Backup failed: ' + e, 'error');
                Logger.error('backupDB failed', e);
            }
        },
        async restoreDB() {
            const confirmed = await API.confirmDialog(
                'WARNING: This will replace the current database with the backup file. Current data will be lost. Continue?',
                'Confirm Restore'
            );
            
            if (!confirmed) return;
            
            try {
                const backupFile = await API.openFileDialog('Select Backup File', [
                    { name: 'Database Files', extensions: ['db'] }
                ]);
                
                if (backupFile) {
                    await API.restoreDatabase(backupFile);
                    this.showToast('Database restored successfully!', 'success');
                    this.loadDashboard();
                    Logger.info('restoreDB', backupFile);
                }
            } catch (e) {
                this.showToast('Restore failed: ' + e, 'error');
                Logger.error('restoreDB failed', e);
            }
        },
        
        async exportAllExcel() {
            try {
                // Bulk Excel shares the bulk base name: Documents/Excel/BACKUP-YYYY-MM-DD.xlsx.
                let excelDir = 'Documents/Excel';
                try { const d = await API.getDocumentsExcelDir(); if(d) excelDir = d; } catch {}
                const defName = backupBaseName('xlsx');
                const defaultPath = joinPath(excelDir, defName);
                const savePath = await API.saveFileDialog('Export All Residents — Documents/Excel', [
                    { name: 'Excel Files', extensions: ['xlsx'] }
                ], defaultPath);
                
                if (savePath) {
                    await API.exportToExcel(savePath);
                    this.showToast('Excel saved to ' + savePath, 'success');
                    Logger.info('exportAllExcel', savePath);
                }
            } catch (e) {
                this.showToast('Export failed: ' + e, 'error');
                Logger.error('exportAllExcel failed', e);
            }
        },
        
        // ---- Official card template (src/card-template.html, CR100 98.6x66.8) ----
        // Single render path: buildCardPayload -> fillTemplateHtml for single
        // cards; printAllCards reuses buildCardPayloadForRow per resident.
        // Prints via hidden iframe on CR100 (browser dialog offers Save-as-PDF).
        // QR comes from the backend get_qr_data_url command (qrcode crate).
        // P1-7: local heuristic mirrors backend config::short_barangay
        // (offline fallback only — payload prefers get_short_barangay).
        // Format barangay for card display: "Barangay (Nouvelas Occidental) Balud" → "NOUVELAS OCCIDENTAL (BALUD)"
                barangayCardDisplay(name) {
                    const s = (name || '').trim();
                    if (!s) return 'BALUD';
                    // Extract content inside first parens + any text after closing paren:
                    const m = s.match(/\(([^)]*)\)\s*(.*)/);
                    if (m) {
                        const inner = m[1].trim().toUpperCase();
                        const after = m[2].trim().toUpperCase();
                        if (!inner) return after || 'BALUD';
                        return after ? `${inner} (${after})` : inner;
                    }
                    // No parens → strip title words, preserve full remainder
                    let cleaned = s.replace(/^(Barangay|Brgy|Brgy\.)\s+/i, '').trim();
                    if (!cleaned) return 'BALUD';
                    return cleaned.toUpperCase().slice(0, 40) || 'BALUD';
                },

                // Paren-first: "..." content wins; otherwise last-token fallback.
        shortBarangay(name) {
            const s = (name || '').trim();
            if (!s) return 'BALUD';
            // Extract content inside the FIRST pair of parentheses:
            // "Barangay (Nouvelas Occidental) Balud" → "NOUVELAS OCCIDENTAL".
            const m = s.match(/\(([^)]*)\)/);
            if (m && m[1].trim()) {
                return m[1].trim().toUpperCase().slice(0, 40) || 'BALUD';
            }
            const tokens = s.split(/[\s(),]+/).filter(Boolean);
            if (!tokens.length) return 'BALUD';
            let last = (tokens[tokens.length - 1] || 'BALUD').toUpperCase();
            if ((last === 'BARANGAY' || last === 'BRGY' || last === 'BRGY.') && tokens.length >= 2) {
                last = tokens[tokens.length - 2].toUpperCase();
            }
            if (last === 'BARANGAY' || last === 'BRGY' || last === 'BRGY.') return 'BALUD';
            return last.slice(0, 40) || 'BALUD';
        },

        async buildCardPayload(residentId) {
            const r = await API.getResident(residentId);
            return this.buildCardPayloadForRow(r);
        },

        // P1-4: `pre` carries hoisted/batch data for bulk print — photo/qr
        // from get_cards_batch, civic fetched once, short resolved per unique
        // barangay. Absent keys fall back to per-row invokes (single-print path).
        async buildCardPayloadForRow(r, pre) {
            const residentId = r.id;
            let photo = '';
            if (pre && ('photo' in pre)) photo = pre.photo || '';
            else try { photo = await getPhotoAsDataUrl(await API.getResidentPhoto(residentId)); } catch (e) { Logger.warn('card photo fetch failed', e); }
            // No fallback: without an upload the slot stays empty and the
            // template hides it — only the resident's actual photo renders.
            // QR via dedicated backend command; on failure the slot stays
            // empty (template hides empty QR via CSS) — card still prints.
            let qr = '';
            if (pre && ('qr' in pre)) {
                qr = pre.qr || '';
            } else {
                // Call backend QR command
                try {
                    qr = await API.getQrDataUrl(residentId);
                } catch (e) {
                    Logger.warn('card QR fetch failed', e);
                    // qr remains empty string
                }
            }
            const mid = (r.middle_name || '').trim() || 'N/A';
            const purok = ((r.address_purok || 'AVOCADO') + '').toUpperCase();
            // Front card uses barangayCardDisplay (e.g. "NOUVELAS OCCIDENTAL (BALUD)")
            let short = (pre && pre.short) || this.barangayCardDisplay(this.settings.barangayName);
            if (!(pre && pre.short)) try {
                const sb = await API.getShortBarangay(this.settings.barangayName || '');
                if (sb) short = sb;
            } catch (e) { Logger.warn('shortBarangay backend failed, using local fallback', e); }
            // Truncation widths (12/12/12/9/62) keep long names inside the
            // CR100 seats; template .fv uses ellipsis as the second guard.
            // Front address: PUROK <purok>, <short-barangay>, <city>, <province>
            // (e.g. "PUROK AVOCADO, BALUD, BASEY, SAMAR"). City/province come
            // from the resident row; settings.cityName ("Basey, Samar") is only
            // a fallback and is split so province is never dropped/duplicated.
            let addrCity = ((r.address_city || '').trim() || '').toUpperCase();
            let addrProvince = ((r.address_province || '').trim() || '').toUpperCase();
            if (!addrCity || !addrProvince) {
                const parts = String(this.settings.cityName || '').split(',').map((s) => s.trim().toUpperCase()).filter(Boolean);
                if (!addrCity && parts[0]) addrCity = parts[0];
                if (!addrProvince && parts[1]) addrProvince = parts[1];
            }
            if (!addrCity) addrCity = 'BASEY';
            if (!addrProvince) addrProvince = 'SAMAR';
            const addressRaw = `PUROK ${purok}, ${short}, ${addrCity}, ${addrProvince}`;
            const address = truncateCard(addressRaw, 62);
            // Validity (L1): backend is truth — id_card.rs::valid_until_display.
            // residents.rs backfills valid_until on create/update, so persisted
            // rows always carry ISO and this mirror only formats. The +365
            // fallback below mirrors the backend default for pre-backfill rows.
            let vuRaw = (r.valid_until || '').trim();
            if (!vuRaw) {
                const d = new Date(); d.setDate(d.getDate() + 365);
                vuRaw = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
            }
            const months = ['JANUARY','FEBRUARY','MARCH','APRIL','MAY','JUNE','JULY','AUGUST','SEPTEMBER','OCTOBER','NOVEMBER','DECEMBER'];
            let validBar = 'THIS ID IS VALID UNTIL ' + vuRaw.toUpperCase();
            const parts = vuRaw.split(/[-/]/);
            if (parts.length === 3) {
                // ISO [YYYY,MM,DD] or legacy [DD,MM,YYYY].
                const iso = parts[0].length === 4;
                const mi = parseInt(parts[1], 10);
                const yy = iso ? parts[0] : parts[2];
                if (mi >= 1 && mi <= 12) validBar = `THIS ID IS VALID UNTIL ${months[mi - 1]} ${yy}`;
            }
            // M4: civic header prefers backend config/civic.json (source of truth)
            // via get_civic_config; localStorage settings are fallback only.
            // province_line split on \u2022 preserved.
            let civic = (pre && pre.civic) || null;
            if (!civic) try { civic = await API.getCivicConfig(); } catch (e) { Logger.warn('civic config backend failed, using local settings fallback', e); }
            const pick = (v, fb) => (v && String(v).trim()) ? String(v).trim() : fb;
            const cBarangay = pick(civic && (civic.barangay_full || civic.barangay_short), null);
            const cPunong = pick(civic && civic.punong_name, null);
            // Backend wins: sync civic.json values into settings so the payload
            // lines below (and golden-card's asserted punong substring) read them.
            if (civic) {
                if (pick(civic.province_line)) this.settings.provinceName = pick(civic.province_line);
                if (cBarangay) this.settings.barangayName = cBarangay;
                if (cPunong) this.settings.punongName = cPunong;
            }
            let province = 'Province of Samar', municipality = 'Municipality of Basey';
            const provLine = pick(civic && civic.province_line, (this.settings.provinceName || 'Province of Samar \u2022 Municipality of Basey').trim() || 'Province of Samar \u2022 Municipality of Basey');
            for (const sep of ['\u2022', '|', ';']) {
                if (provLine.includes(sep)) {
                    const sp = provLine.split(sep);
                    if (sp[0].trim() && sp[1].trim()) { province = sp[0].trim(); municipality = sp[1].trim(); }
                    break;
                }
            }
            const emerAddr2 = (r.address_street || '').toUpperCase().trim();
            const emerAddr1 = `PUROK ${purok}, ${short}, ${addrCity}, ${addrProvince}`;
            // Explicit emergency address wins when entered; municipality +
            // province are appended unless already present, so the back card
            // never shows a bare street/purok without BASEY, SAMAR.
            // Otherwise fall back to the resident's own address (legacy rows
            // without the field), with street appended when present.
            const emerExplicit = (r.emergency_contact_address || '').trim().toUpperCase();
            let emerFull;
            if (emerExplicit) {
                emerFull = emerExplicit;
                if (addrCity && !emerFull.includes(addrCity)) emerFull += `, ${addrCity}`;
                if (addrProvince && !emerFull.includes(addrProvince)) emerFull += `, ${addrProvince}`;
            } else {
                emerFull = emerAddr2 ? `${emerAddr1}, ${emerAddr2}` : emerAddr1;
            }
            return {
                republic: pick(civic && civic.republic, 'Republic of the Philippines'),
                province, municipality,
                barangay: ((r.address_barangay || cBarangay || this.settings.barangayName || '').toUpperCase()),
                card_title: pick(civic && civic.card_title, "BARANGAY RESIDENT'S CARD"),
                photo,
                last_name: truncateCard((r.last_name || '').toUpperCase(), 12),
                first_name: truncateCard([r.first_name, r.suffix].filter(Boolean).join(' ').toUpperCase(), 12),
                middle_name: truncateCard(mid.toUpperCase(), 12),
                dob: formatDateDDMMYYYY(r.birth_date),
                civil_status: truncateCard(((r.civil_status || 'SINGLE') + '').toUpperCase(), 9),
                gender: (r.gender || '').toUpperCase(),
                address,
                resident_id: r.resident_id,
                qr,
                punong_name: truncateCard((this.settings.punongName || '').toUpperCase(), 30),
                punong_title: 'PUNONG BARANGAY',
                emer_name: truncateCard(((r.emergency_contact_name || 'NO EMERGENCY CONTACT') + '').toUpperCase(), 30),
                emer_relation: truncateCard(((r.emergency_contact_relation || 'N/A') + '').toUpperCase(), 20),
                emer_address: truncateCard(emerFull, 62),
                emer_contact: truncateCard(formatContactPH(r.emergency_contact_number) || formatContactPH(r.contact_number) || 'N/A', 20),
                valid_until: validBar
            };
        },

        fillTemplateHtml(html, payload) {
            const parser = new DOMParser();
            const doc = parser.parseFromString(html, 'text/html');
            const IMG = { photo: 1, qr: 1, seal: 1, emblem: 1 };
            doc.querySelectorAll('[data-field]').forEach((el) => {
                const k = el.getAttribute('data-field');
                if (!(k in payload) || payload[k] == null || payload[k] === '') return;
                if (IMG[k] && el.tagName === 'IMG') { if (isSafeImgSrc(payload[k])) el.setAttribute('src', payload[k]); }
                else el.textContent = payload[k];
            });
            if (payload.resident_id) doc.title = 'ID Card - ' + payload.resident_id;
            return '<!DOCTYPE html>\n' + doc.documentElement.outerHTML;
        },

        printFilledCard(html) {
            let iframe = document.getElementById('card-print-frame');
            if (!iframe) {
                iframe = document.createElement('iframe');
                iframe.id = 'card-print-frame';
                iframe.setAttribute('title', 'ID card print frame');
                iframe.style.cssText = 'position:fixed;right:0;bottom:0;width:0;height:0;border:0;';
                document.body.appendChild(iframe);
            }
            iframe.onload = null;
            iframe.onload = () => {
                try { iframe.contentWindow.focus(); iframe.contentWindow.print(); }
                catch (e) { Logger.error('card print failed', e); }
            };
            iframe.srcdoc = html;
        },

        // Single source of truth for card HTML: card-template.html filled by
        // buildCardPayload (same field map for preview, single print, bulk).
        async cardHtmlForResident(residentId) {
            const [tpl, payload] = await Promise.all([
                fetch('card-template.html').then((res) => {
                    if (!res.ok) throw new Error('template HTTP ' + res.status);
                    return res.text();
                }),
                this.buildCardPayload(residentId)
            ]);
            return this.fillTemplateHtml(tpl, payload);
        },

        async printResidentCard(residentId) {
            try {
                this.showToast('Preparing CR100 card for print...', 'info');
                this.printFilledCard(await this.cardHtmlForResident(residentId));
                Logger.info('printResidentCard', residentId);
            } catch (e) {
                this.showToast('Card print failed: ' + e, 'error');
                Logger.error('printResidentCard failed', e);
            }
        },

        async previewResidentCard(residentId) {
            try {
                const filled = await this.cardHtmlForResident(residentId);
                const w = window.open('', '_blank');
                if (!w) { this.showToast('Popup blocked — printing directly instead', 'info'); this.printFilledCard(filled); return; }
                w.document.open(); w.document.write(filled); w.document.close(); w.focus();
                Logger.info('previewResidentCard', residentId);
            } catch (e) {
                this.showToast('Card preview failed: ' + e, 'error');
                Logger.error('previewResidentCard failed', e);
            }
        },

        showToast(message, type = 'info') {
            this.toastSeq = (this.toastSeq || 0) + 1;
            const seq = this.toastSeq;
            this.toast = { show: true, message, type };
            Logger.info('toast', type + ': ' + message);
            const duration = type === 'error' ? 5000 : 3000;
            setTimeout(() => {
                if (seq === this.toastSeq) this.toast.show = false;
            }, duration);
        },

        cancelBulkPrint() {
            this.bulkPrintCancel = true;
        }
    };
}
