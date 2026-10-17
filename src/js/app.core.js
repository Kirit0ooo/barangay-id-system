// Core Alpine app state + shared methods.
// Loaded first: defines window.appState; view modules extend it via Object.assign.
// All scripts are classic <script> (no ES modules) — x-data="app()" resolves
// the global app() function which returns window.appState.

// Module-level helpers
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

// ─── Shared state (window.appState) ─────────────────────────────────────────
window.appState = {
    // Views
    currentView: 'dashboard',
    
    // Stats
    stats: { total: 0, thisYear: 0, male: 0, female: 0, active: 0, renew: 0, lost: 0, damage: 0 },
    
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
    // Theme (red-gradient backdrop opt-in, default off / paper).
    // Own localStorage key so it survives the loadCivicConfig()
    // wholesale settings rebuild on every boot.
    theme: 'paper',
    purokOptions: PUROK_OPTIONS,
    
    // Toast
    toast: { show: false, message: '', type: 'info' },
    
    // Live bulk-print progress (plain JS loop over residents — no backend events).
    bulkProgress: { current: 0, total: 0, percent: 0, inProgress: false, failures: 0, recent: [] },
    
    // Photo GC
    photoScanResult: null,
    backupRunScheduled: false,
    
    // ─── Shared helper wrappers ────────────────────────────────────────
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
    
    // ─── Init + theme ──────────────────────────────────────────────────
    async init() {
        try {
            this.applyTheme();             // Theme from localStorage before any backend await — no red flash on boot.
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
    
    applyTheme() {
        this.theme = localStorage.getItem('barangayTheme') === 'red-gradient' ? 'red-gradient' : 'paper';
        if (this.theme === 'red-gradient') document.documentElement.dataset.theme = 'red-gradient';
        else delete document.documentElement.dataset.theme;
    },
    setTheme(on) { localStorage.setItem('barangayTheme', on ? 'red-gradient' : 'paper'); this.applyTheme(); },
    
    // ─── Civic config + settings ───────────────────────────────────────
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
    
    // ─── Toast ─────────────────────────────────────────────────────────
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
};
