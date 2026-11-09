// Register view methods.
// Extends window.appState (defined in app.core.js).
Object.assign(window.appState, {
    
    // ─── Register: ID generation ───────────────────────────────────────
    async generateID() {
        try {
            this.form.resident_id = await API.generateNextId();
        } catch (e) {
            this.showToast('Failed to generate ID: ' + e, 'error');
            Logger.error('generateID failed', e);
        }
    },
    
    // ─── Register: photo upload ────────────────────────────────────────
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
    
    // ─── Register: save / update ───────────────────────────────────────
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
        // generateID reports its own failure (try/catch + toast), so this stays sync.
        this.generateID();
        // Address defaults (barangay/city/province) are already set by emptyForm()
        // as fixed literals; nothing here reads settings.barangayName.
    },
    
    // ─── Register: edit mode ───────────────────────────────────────────
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
    
    // Edit-mode Back: return to the resident's detail view without saving.
    async goBackFromEdit() {
        const id = this.editingResident;
        if (!id) { this.closeDetail(); return; }
        await this.viewResident(id);
        this.previousView = this.editReturnView || 'dashboard';
    },
    
});
