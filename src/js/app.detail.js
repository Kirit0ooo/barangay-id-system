// Detail view methods (resident details, print, preview, renew, delete).
// Extends window.appState (defined in app.core.js).
Object.assign(window.appState, {

    // ─── Detail: view / close ──────────────────────────────────────────
    async viewResident(id) {
        try {
            this.previousView = this.currentView;
            const [resident, photo] = await Promise.all([
                API.getResident(id),
                API.getResidentPhoto(id).catch(() => null)
            ]);
            this.detailResident = resident;
            this.detailPhoto = photo || null;
            this.currentView = 'detail';
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

    // ─── Detail: delete / renew ────────────────────────────────────────
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

    async renewResident(id) {
        try {
            const confirmed = await API.confirmDialog(
                'Renew this resident\'s ID card? This will extend validity by 1 year from today and print the new card.',
                'Confirm Renewal'
            );
            if (!confirmed) return;
            const resident = await API.getResident(id);
            const today = new Date();
            const nextYear = new Date(today.getFullYear() + 1, today.getMonth(), today.getDate());
            const newValidUntil = `${nextYear.getFullYear()}-${String(nextYear.getMonth() + 1).padStart(2, '0')}-${String(nextYear.getDate()).padStart(2, '0')}`;
            await API.updateResident({ ...resident, valid_until: newValidUntil }, '');
            this.showToast('ID renewed — validity extended to ' + nextYear.toLocaleDateString('en-PH', { year: 'numeric', month: 'long', day: 'numeric' }), 'success');
            Logger.info('renewResident', { id, newValidUntil });
            this.searchResidents();
            this.loadDashboard();
            this.printResidentCard(id);
        } catch (e) {
            this.showToast('Renewal failed: ' + e, 'error');
            Logger.error('renewResident failed', e);
        }
    },

    // ─── Detail: card display helpers ──────────────────────────────────
    // (offline fallback only — payload prefers get_short_barangay).
    // Format barangay for card display: "Barangay (Nouvelas Occidental) Balud" → "NOUVELAS OCCIDENTAL (BALUD)"
    barangayCardDisplay(name) {
        const s = (name || '').trim();
        if (!s) return 'BALUD';
        const m = s.match(/\(([^)]*)\)\s*(.*)/);
        if (m) {
            const inner = m[1].trim().toUpperCase();
            const after = m[2].trim().toUpperCase();
            if (!inner) return after || 'BALUD';
            return after ? `${inner} (${after})` : inner;
        }
        let cleaned = s.replace(/^(Barangay|Brgy|Brgy\.)\s+/i, '').trim();
        if (!cleaned) return 'BALUD';
        return cleaned.toUpperCase().slice(0, 40) || 'BALUD';
    },

    // Paren-first: "..." content wins; otherwise last-token fallback.
    shortBarangay(name) {
        const s = (name || '').trim();
        if (!s) return 'BALUD';
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

    // ─── Detail: card payload ──────────────────────────────────────────
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
        let qr = '';
        if (pre && ('qr' in pre)) {
            qr = pre.qr || '';
        } else {
            try {
                qr = await API.getQrDataUrl(residentId);
            } catch (e) {
                Logger.warn('card QR fetch failed', e);
            }
        }
        const mid = (r.middle_name || '').trim() || 'N/A';
        const purok = ((r.address_purok || 'AVOCADO') + '').toUpperCase();
        let short = (pre && pre.short) || this.barangayCardDisplay(this.settings.barangayName);
        if (!(pre && pre.short)) try {
            const sb = await API.getShortBarangay(this.settings.barangayName || '');
            if (sb) short = sb;
        } catch (e) { Logger.warn('shortBarangay backend failed, using local fallback', e); }
        let addrCity = ((r.address_city || '').trim() || '').toUpperCase();
        let addrProvince = ((r.address_province || '').trim() || '').toUpperCase();
        if (!addrCity || !addrProvince) {
            const parts = String(this.settings.cityName || '').split(',').map((s) => s.trim().toUpperCase()).filter(Boolean);
            if (!addrCity && parts[0]) addrCity = parts[0];
            if (!addrProvince && parts[1]) addrProvince = parts[1];
        }
        if (!addrCity) addrCity = 'BASEY';
        if (!addrProvince) addrProvince = 'SAMAR';
        let addressRaw = `PUROK ${purok}, ${short}, ${addrCity}, ${addrProvince}`;
        let address = truncateCard(addressRaw, 62);
        // Validity (L1): backend is truth — id_card.rs::valid_until_display.
        let vuRaw = (r.valid_until || '').trim();
        if (!vuRaw) {
            const d = new Date(); d.setDate(d.getDate() + 365);
            vuRaw = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
        }
        const months = ['JANUARY','FEBRUARY','MARCH','APRIL','MAY','JUNE','JULY','AUGUST','SEPTEMBER','OCTOBER','NOVEMBER','DECEMBER'];
        let validBar = 'THIS ID IS VALID UNTIL ' + vuRaw.toUpperCase();
        const parts = vuRaw.split(/[-/]/);
        if (parts.length === 3) {
            const iso = parts[0].length === 4;
            const mi = parseInt(parts[1], 10);
            const yy = iso ? parts[0] : parts[2];
            if (mi >= 1 && mi <= 12) validBar = `THIS ID IS VALID UNTIL ${months[mi - 1]} ${yy}`;
        }
        let civic = (pre && pre.civic) || null;
        if (!civic) try { civic = await API.getCivicConfig(); } catch (e) { Logger.warn('civic config backend failed, using local settings fallback', e); }
        const pick = (v, fb) => (v && String(v).trim()) ? String(v).trim() : fb;
        const cBarangay = pick(civic && (civic.barangay_full || civic.barangay_short), null);
        const cPunong = pick(civic && civic.punong_name, null);
        if (civic) {
            if (pick(civic.province_line)) this.settings.provinceName = pick(civic.province_line);
            if (cBarangay) this.settings.barangayName = cBarangay;
            if (cPunong) this.settings.punongName = cPunong;
        }
        // Use civic barangay_short as the canonical short barangay name for card
        // addresses. barangayCardDisplay/getShortBarangay compose paren-form names
        // like "NOUVELAS OCCIDENTAL (BALUD)" but the address needs just "BALUD".
        if (civic && civic.barangay_short) {
            short = civic.barangay_short.toUpperCase();
            addressRaw = `PUROK ${purok}, ${short}, ${addrCity}, ${addrProvince}`;
            address = truncateCard(addressRaw, 62);
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

    fillTemplate(el, payload) {
        const IMG = { photo: 1, qr: 1, seal: 1, emblem: 1 };
        el.querySelectorAll('[data-field]').forEach((el2) => {
            const k = el2.getAttribute('data-field');
            if (!(k in payload) || payload[k] == null || payload[k] === '') return;
            if (IMG[k] && el2.tagName === 'IMG') { if (isSafeImgSrc(payload[k])) el2.setAttribute('src', payload[k]); }
            else el2.textContent = payload[k];
        });
    },

    fillTemplateHtml(html, payload) {
        const parser = new DOMParser();
        const doc = parser.parseFromString(html, 'text/html');
        this.fillTemplate(doc, payload);
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

    // ─── Detail: bulk print ────────────────────────────────────────────
    async printAllCards() {
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
        this.bulkPrintCancel = false;
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
                        this.fillTemplate(filled, payload);
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
                const bulkHtml = '<!DOCTYPE html>\n<html><head><meta charset="UTF-8"><title>Bulk ID Cards - ' + batchRows.length + ' cards</title><style>'
                    + css + '</style></head><body><div class="sheet">' + sections.join('\n') + '</div></body></html>';
                this.showToast(`Printing batch ${Math.floor(batchStart / PRINT_BATCH_SIZE) + 1} of ${Math.ceil(rows.length / PRINT_BATCH_SIZE)} (${batchRows.length} cards)…`, 'info');
                this.printFilledCard(bulkHtml);
                if (batchEnd < rows.length) {
                    await new Promise(r => setTimeout(r, 500));
                }
                if (this.bulkPrintCancel) { this.showToast('Bulk print cancelled', 'info'); break; }
            }
            const finalOk = rows.length - this.bulkProgress.failures;
            this.showToast(`Print ready: ${finalOk}/${rows.length} cards${this.bulkProgress.failures ? ` (${this.bulkProgress.failures} failed)` : ''}`, this.bulkProgress.failures ? 'info' : 'success');
            Logger.info('printAllCards', { total: rows.length, ok: finalOk, failed: this.bulkProgress.failures });
        } catch (e) {
            this.showToast('Bulk print failed: ' + e, 'error');
            Logger.error('printAllCards failed', e);
        } finally {
            this.bulkPrintCancel = false;
            this.bulkProgress.inProgress = false;
        }
    },

    cancelBulkPrint() {
        this.bulkPrintCancel = true;
    },

});
