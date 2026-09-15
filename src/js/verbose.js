// Early verbose bootstrap — must run before logger.js so BARANGAY_VERBOSE is set for first logs.
// Applies ?verbose=1 -> localStorage, then mirrors localStorage -> window.BARANGAY_VERBOSE.
// No inline <script> needed (CSP keeps 'unsafe-inline' only for Alpine's new Function).
try {
  if (new URLSearchParams(location.search).has('verbose')) localStorage.setItem('barangay_verbose', '1');
  if (localStorage.getItem('barangay_verbose') === '1') window.BARANGAY_VERBOSE = true;
} catch {}
