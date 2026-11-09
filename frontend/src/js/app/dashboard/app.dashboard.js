// Dashboard view methods (status, expiry).
// Extends window.appState (defined in app.core.js).
Object.assign(window.appState, {
    
    // ─── Dashboard: status label ───────────────────────────────────────
    statusLabel(r) {
        // Delegates to pure residentStatus in helpers.js (D6): operator
        // remarks outrank generic expiry; expiry is computed here once.
        return residentStatus(r, this.isExpired(r));
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
    
    // ─── Dashboard: expiry ─────────────────────────────────────────────
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
    
});
