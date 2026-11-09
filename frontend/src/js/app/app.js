// App entry point — replaces the monolithic app.js.
// window.appState is defined in app.core.js and extended by view modules.
// x-data="app()" resolves this global function, which returns window.appState.
// All scripts are classic <script> (no ES modules).

function app() {
    return window.appState;
}

// Alpine is loaded with `defer` in index.html and self-starts via its own
// DOMContentLoaded microtask (vendor/alpine.min.js calls start() internally).
// No manual Alpine.start() needed here — calling it twice risks re-running
// x-init handlers (double loadDashboard/loadCivicConfig IPCs).
