// Settings view methods.
// Extends window.appState (defined in app.core.js).
Object.assign(window.appState, {

    // ─── Settings: backup / restore / export / import ──────────────────
    async backupDB() {
        try {
            let savePath = null;
            if (this.backupRunScheduled) {
                const res = await API.scheduledBackup();
                savePath = res.path;
                this.backupRunScheduled = false;
            } else {
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
            let excelDir = 'Documents/Excel';
            try { const d = await API.getDocumentsExcelDir(); if (d) excelDir = d; } catch {}
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

    async importExcel() {
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

    // ─── Settings: photo GC ────────────────────────────────────────────
    // R5 — photo GC: two-step scan → delete
    photoScanResult: null,

    async scanPhotos() {
        try {
            const res = await API.photoGc(false);
            this.photoScanResult = res;
            const msg = `Photo scan: total ${res.total_files}, referenced ${res.referenced}, orphan ${res.orphan_count}`;
            this.showToast(msg, res.orphan_count ? 'info' : 'success');
            Logger.info('photo_scan', res);
        } catch (e) {
            this.showToast('Photo scan failed: ' + e, 'error');
            Logger.error('photo_scan failed', e);
        }
    },

    async cleanOrphans() {
        if (!this.photoScanResult || this.photoScanResult.orphan_count === 0) return;
        try {
            const res = await API.photoGc(true);
            this.showToast(`Photo GC: deleted ${res.deleted} of ${res.orphan_count} orphaned photos`, 'success');
            this.photoScanResult = null;
            Logger.info('photo_gc_delete', res);
        } catch (e) {
            this.showToast('Photo GC failed: ' + e, 'error');
            Logger.error('photo_gc_delete failed', e);
        }
    },

    // ─── Settings: database maintenance ────────────────────────────────
    async optimizeDatabase() {
        try {
            await API.optimizeDatabase();
            this.showToast('Database optimized', 'success');
        } catch (e) {
            this.showToast('Database optimize failed: ' + e, 'error');
            Logger.error('optimizeDatabase failed', e);
        }
    },
    
    // ─── Settings: encryption at rest ──────────────────────────────────
    async encryptionGenerateKey() {
        try {
            const msg = await API.encryptionGenerateKey();
            this.showToast(msg, 'success');
            return msg;
        } catch (e) {
            this.showToast('Encryption key failed: ' + e, 'error');
            throw e;
        }
    },
    async encryptionStatus() {
        try {
            return await API.encryptionStatus();
        } catch (e) {
            this.showToast('Encryption status failed: ' + e, 'error');
            throw e;
        }
    },
    async encryptionEncryptDatabase() {
        try {
            const msg = await API.encryptionEncryptDatabase();
            this.showToast(msg, 'success');
            return msg;
        } catch (e) {
            this.showToast('Database encryption failed: ' + e, 'error');
            throw e;
        }
    },
    async encryptionDecryptDatabase() {
        try {
            const msg = await API.encryptionDecryptDatabase();
            this.showToast(msg, 'success');
            return msg;
        } catch (e) {
            this.showToast('Database decryption failed: ' + e, 'error');
            throw e;
        }
    },
    async encryptionEncryptPhotos() {
        try {
            const msg = await API.encryptionEncryptPhotos();
            this.showToast(msg, 'success');
            return msg;
        } catch (e) {
            this.showToast('Photo encryption failed: ' + e, 'error');
            throw e;
        }
    },
    async encryptionDecryptPhotos() {
        try {
            const msg = await API.encryptionDecryptPhotos();
            this.showToast(msg, 'success');
            return msg;
        } catch (e) {
            this.showToast('Photo decryption failed: ' + e, 'error');
            throw e;
        }
    },
    
});
