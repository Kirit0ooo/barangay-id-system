// API wrapper for Tauri backend calls
const API = {
    // Resident operations
    async generateNextId() {
        return await window.__TAURI__.invoke('generate_next_id');
    },
    
    async createResident(resident, photoData) {
        return await window.__TAURI__.invoke('create_resident', { 
            input: { resident, photoData } 
        });
    },
    
    async getResident(id) {
        return await window.__TAURI__.invoke('get_resident', { id });
    },
    
    async updateResident(resident, photoData) {
        return await window.__TAURI__.invoke('update_resident', { 
            input: { resident, photoData } 
        });
    },
    
    async deleteResident(id) {
        return await window.__TAURI__.invoke('delete_resident', { id });
    },
    
    // P0-3: backend hides soft-deleted (is_active=0) unless includeInactive
    // is true. Callers pass true only for All/Inactive categories; the
    // Sorting Category dropdown still filters/sorts client-side (see applySearchCategory).
    async searchResidents(query, purok, includeInactive) {
        return await window.__TAURI__.invoke('search_residents', { 
            query, purok, includeInactive 
        });
    },
    
    async getAllResidents(page, perPage) {
        // F10: paginated; omit args for legacy default (first page, safety cap server-side).
        const args = {};
        if (page != null) args.page = page;
        if (perPage != null) args.perPage = perPage;
        // Tauri v1 maps camelCase JS keys to snake_case Rust params.
        return await window.__TAURI__.invoke('get_all_residents', args);
    },
    async getDashboardStats() {
        // F11: aggregate counts without fetching full rows.
        return await window.__TAURI__.invoke('get_dashboard_stats');
    },
    async getCivicConfig() {
        // F15: centralized civic/officer strings.
        return await window.__TAURI__.invoke('get_civic_config');
    },
    async setCivicConfig(partial) {
        // Write operator-editable civic config to config/civic.json
        return await window.__TAURI__.invoke('set_civic_config', { partial });
    },
    async getShortBarangay(barangay) {
        // P1-7: single server-side short-barangay implementation.
        return await window.__TAURI__.invoke('get_short_barangay', { barangay });
    },
    async getQrDataUrl(residentId) {
        // P0-4: dedicated QR data-URL (no regex scraping of preview HTML).
        return await window.__TAURI__.invoke('get_qr_data_url', { residentId });
    },
    async getCardsBatch(ids) {
        // P1-4: one IPC call returns photo+QR for up to 200 residents.
        return await window.__TAURI__.invoke('get_cards_batch', { ids });
    },
    async getResidentPhoto(id) {
        return await window.__TAURI__.invoke('get_resident_photo', { id });
    },
    
    // ID Card (HTML-only print via card-template.html; no PDF commands exist).
    
    // Backup
    async backupDatabase(backupPath) {
        return await window.__TAURI__.invoke('backup_database', { backupPath });
    },
    
    async restoreDatabase(backupFile) {
        return await window.__TAURI__.invoke('restore_database', { backupFile });
    },
    
    // Export
    async exportToExcel(savePath) {
        return await window.__TAURI__.invoke('export_to_excel', { savePath });
    },
    
    async importFromExcel(filePath) {
        return await window.__TAURI__.invoke('import_from_excel', { filePath });
    },
    async scheduledBackup() {
        return await window.__TAURI__.invoke('scheduled_backup');
    },
    async photoGc(doDelete) {
        return await window.__TAURI__.invoke('photo_gc', { delete: !!doDelete });
    },
    async optimizeDatabase() {
        // DB maintenance: PRAGMA optimize + incremental vacuum (see db.rs).
        return await window.__TAURI__.invoke('optimize_database_cmd');
    },
    
        // Logging (backend `log_frontend` is invoked directly by js/logger.js;
    // no wrapper needed here)
    async getDocumentsExcelDir() {
        return await window.__TAURI__.invoke('get_documents_excel_dir');
    },
    async getLogsDir() {
        return await window.__TAURI__.invoke('get_logs_dir');
    },
    async openPath(path) {
        return await window.__TAURI__.invoke('open_path', { path });
    },
    
    // Dialog helpers
    async saveFileDialog(title, filters, defaultPath) {
        const opts = { title, filters };
        if (defaultPath) opts.defaultPath = defaultPath;
        return await window.__TAURI__.dialog.save(opts);
    },
    
    async openFileDialog(title, filters) {
        return await window.__TAURI__.dialog.open({ title, filters, multiple: false });
    },
    
    async confirmDialog(message, title) {
        return await window.__TAURI__.dialog.ask(message, { title, type: 'warning' });
    },
    
    // Encryption at rest
    async encryptionGenerateKey() {
        return await window.__TAURI__.invoke('encryption_generate_key');
    },
    async encryptionStatus() {
        return await window.__TAURI__.invoke('encryption_status');
    },
    async encryptionEncryptDatabase() {
        return await window.__TAURI__.invoke('encryption_encrypt_database');
    },
    async encryptionDecryptDatabase() {
        return await window.__TAURI__.invoke('encryption_decrypt_database');
    },
    async encryptionEncryptPhotos() {
        return await window.__TAURI__.invoke('encryption_encrypt_photos');
    },
    async encryptionDecryptPhotos() {
        return await window.__TAURI__.invoke('encryption_decrypt_photos');
    },
};