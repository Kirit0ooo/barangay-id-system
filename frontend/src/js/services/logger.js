// Isolated frontend logger - all logs stay inside D:\Barangay\barangay-id-system\logs
// Writes to: console + Tauri backend file (logs/frontend.log + app.log) + localStorage buffer

const Logger = (() => {
  const BUFFER_KEY = 'barangay_log_buffer';
  const VERBOSE_KEY = 'barangay_verbose';
  const MAX_BUFFER = 500;
  // Verbose gate: debug lines always buffer/file, but console only when verbose.
  // Toggle: localStorage barangay_verbose=1, ?verbose=1, BARANGAY_VERBOSE=1, or Logger.setVerbose()
  function isVerbose() {
    try {
      if (typeof window !== 'undefined' && window.BARANGAY_VERBOSE === true) return true;
      if (typeof localStorage !== 'undefined' && localStorage.getItem(VERBOSE_KEY) === '1') return true;
      if (typeof location !== 'undefined' && new URLSearchParams(location.search).has('verbose')) return true;
    } catch {}
    return false;
  }

  function buffer(level, msg) {
    // F13: guarded JSON.parse — corrupted buffer never breaks logging.
    try {
      let arr;
      try { arr = JSON.parse(localStorage.getItem(BUFFER_KEY) || '[]'); }
      catch { localStorage.removeItem(BUFFER_KEY); arr = []; }
      if (!Array.isArray(arr)) arr = [];
      arr.push({ t: new Date().toISOString(), level, msg });
      while (arr.length > MAX_BUFFER) arr.shift();
      localStorage.setItem(BUFFER_KEY, JSON.stringify(arr));
    } catch {}
  }

  async function toBackend(level, msg) {
    try {
      if (window.__TAURI__ && window.__TAURI__.invoke) {
        await window.__TAURI__.invoke('log_frontend', { level, message: msg });
      }
    } catch {}
  }

  function fmt(args) {
    return args.map(a => {
      if (a instanceof Error) return a.stack || (a.name + ': ' + a.message);
      try { return typeof a === 'object' ? JSON.stringify(a) : String(a); }
      catch { return String(a); }
    }).join(' ');
  }

  function log(level, ...args) {
    const msg = fmt(args);
    const line = `[frontend] [${level}] ${msg}`;
    // Console — debug only when verbose, others always
    const verbose = isVerbose();
    if (level === 'debug' && !verbose) {
      // still buffer/file, just quiet console (use console.debug so DevTools 'Verbose' filter applies)
      console.debug(line);
    } else if (level === 'error') console.error(line);
    else if (level === 'warn') console.warn(line);
    else if (level === 'debug') console.debug(line);
    else console.log(line);
    buffer(level, msg);
    toBackend(level, msg);
  }

  return {
    info: (...a) => log('info', ...a),
    warn: (...a) => log('warn', ...a),
    error: (...a) => log('error', ...a),
    debug: (...a) => log('debug', ...a),
    isVerbose,
    setVerbose: (on) => { try { if (on) localStorage.setItem(VERBOSE_KEY, '1'); else localStorage.removeItem(VERBOSE_KEY); } catch {} return isVerbose(); },
    getBuffer: () => {
      try { return JSON.parse(localStorage.getItem(BUFFER_KEY) || '[]'); } catch { return []; }
    },
    download: () => {
      const data = JSON.stringify(Logger.getBuffer(), null, 2);
      const blob = new Blob([data], {type: 'application/json'});
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url; a.download = `barangay-frontend-${new Date().toISOString().slice(0,10)}.json`;
      a.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
    },
    clear: () => localStorage.removeItem(BUFFER_KEY)
  };
})();

// Global error capture -> isolated log
window.addEventListener('error', e => Logger.error('window.onerror:', e.message, e.filename + ':' + e.lineno));
window.addEventListener('unhandledrejection', e => Logger.error('unhandledrejection:', e.reason));

// Expose
window.Logger = Logger;
