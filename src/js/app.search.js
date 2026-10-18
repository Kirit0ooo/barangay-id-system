// Search view methods.
// Extends window.appState (defined in app.core.js).
Object.assign(window.appState, {
    
    // ─── Search: resident lookup ───────────────────────────────────────
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
                (this.searchQuery || '').toUpperCase(),
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
            this.showToast('Search failed: ' + e, 'error');
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

    // ─── Search: pager ─────────────────────────────────────────────────
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
    
    // ─── Search: full-table pager (dashboard fallback) ─────────────────
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
    
});
