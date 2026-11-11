# Repository Structure and Modularity Guide

**Project:** Barangay Resident ID Registration System  
**Companion to:** `/AGENTS.md`  
**Purpose:** Give coding agents a consistent, low-risk method for organizing, naming, documenting, and maintaining frontend and backend files.  
**Scope:** Repository layout, module boundaries, file naming, dependency direction, documentation, safe file moves, and structural verification.

---

## 1. Instruction precedence and how to use this guide

1. Read the root `AGENTS.md` before doing any work. It contains the project's permanent operating rules, including branch discipline, project-directory confinement, offline-first behavior, data protection, testing requirements, and known technical constraints.
2. Use this guide when a task concerns repository structure, file placement, module boundaries, naming, documentation organization, or refactoring for maintainability.
3. Treat the current source code and working configuration as the source of truth. Documentation can be stale. Verify paths, imports, script references, command registration, test assumptions, and runtime paths before relying on a written tree.
4. If this guide conflicts with the root `AGENTS.md`, follow the root `AGENTS.md` and report the conflict. Do not silently rewrite permanent project rules.
5. This guide describes a maintainable direction, not permission to perform a repository-wide rewrite. Make only the changes authorized by the current task.

The intended audience includes beginner and intermediate programmers. Prefer familiar folder names, explicit responsibilities, short dependency paths, and examples over clever abstractions.

## 2. Project architecture that must be preserved

This repository is an offline-first desktop application built with:

- **Tauri 1.5:** desktop application shell and JavaScript-to-Rust command bridge.
- **Rust:** backend commands, business rules, data handling, security-sensitive operations, and file operations.
- **SQLite + SQLx:** local persistent database.
- **Alpine.js:** frontend behavior in a static HTML page.
- **HTML/CSS/JavaScript:** user interface and HTML-based CR100 card printing.
- **Node.js:** frontend tests and development utilities.

The frontend currently uses classic script tags rather than a module bundler. JavaScript files share browser globals and are loaded in a deliberate order. Do not assume that moving a script is equivalent to changing an ES module import.

The current frontend script order in `src/index.html` is significant:

1. `js/verbose.js` is loaded in the document head before the logger bootstrap.
2. `js/logger.js`
3. `js/api.js`
4. `js/helpers.js`
5. `js/app.core.js`
6. `js/app.register.js`
7. `js/app.search.js`
8. `js/app.settings.js`
9. `js/app.detail.js`
10. `js/app.dashboard.js`
11. `js/app.js` last, as the application entry point.

The application currently has these important frontend responsibilities:

- `app.core.js`: shared application state and core initialization.
- `app.register.js`: resident registration and editing workflows.
- `app.search.js`: resident search, filtering, and pagination.
- `app.settings.js`: settings, backup, restore, import/export, and related UI actions.
- `app.detail.js`: resident detail, status actions, card preview, and printing workflows.
- `app.dashboard.js`: dashboard values and status presentation.
- `app.js`: final Alpine application entry point.
- `api.js`: Tauri command wrappers.
- `helpers.js`: reusable, low-dependency helper functions.
- `logger.js` and `verbose.js`: frontend logging and logging bootstrap.

The current Rust backend is already divided into focused modules, including `backup.rs`, `config.rs`, `crypto.rs`, `db.rs`, `encryption.rs`, `export.rs`, `id_card.rs`, `logger.rs`, `residents.rs`, and `scheduled.rs`. `main.rs` initializes the application and registers Tauri commands.

The print flow is HTML-only. `src/card-template.html` is the CR100 print template. Do not introduce PDF-generation code or resurrect removed PDF-era directories unless the product requirements are explicitly changed.

## 3. Core organization principles

Apply these principles to every proposed change:

### 3.1 One clear responsibility per file

A file should have a short, concrete explanation of what it owns. Avoid files named `misc`, `common2`, `helpers-new`, or `utils` that collect unrelated code. A shared utility must be genuinely reusable and must not become a second home for feature logic.

### 3.2 Organize by responsibility, then by feature

Use a small number of recognizable top-level areas: frontend, backend, tests, tools, and documentation. Within the frontend, group code into application features, services, and shared helpers. Within the backend, keep cohesive Rust modules and introduce deeper layers only when complexity justifies them.

### 3.3 Keep dependencies one-way

- Feature code may depend on shared helpers and service interfaces.
- Shared helpers must not import or call application feature modules.
- UI code must not access SQLite directly.
- Tauri command handlers must validate and delegate; they should not become large containers for unrelated workflows.
- Domain or business rules should not depend on HTML, CSS, Alpine.js, or browser globals.
- Infrastructure code (database, filesystem, cryptography, logging) should not decide how the UI is rendered.

### 3.4 Prefer the simplest structure that solves a real problem

Do not add directories, layers, traits, wrappers, frameworks, or dependencies just to make the tree look more sophisticated. A small cohesive Rust module is better than a multi-layer architecture with no practical benefit.

### 3.5 Preserve compatibility during structural work

Treat every file move or rename as a code change. Update all references and test assumptions. Preserve public Tauri command names, payload shapes, database schema, HTML selectors, Alpine method names, card output, and runtime path behavior unless the task explicitly calls for a functional change.

## 4. Recommended frontend structure

The following is the recommended **incremental target**. It groups the existing files without requiring a new frontend framework or build system.

```text
src/
├── index.html                  # Main single-page application
├── card-template.html          # CR100 print template; HTML-only print flow
├── barangay.png                # Existing navigation/brand image
├── icons/                      # Existing UI icons; keep paths valid
├── css/
│   ├── style.css               # Application styles and design-system usage
│   └── tailwind.css            # Existing prebuilt utility CSS
├── vendor/
│   └── alpine.min.js           # Vendored offline dependency
└── js/
    ├── bootstrap/
    │   └── verbose.js          # Logging configuration bootstrap
    ├── app/
    │   ├── app.core.js         # Shared state and initialization
    │   ├── app.js              # Alpine app entry point; load last
    │   ├── dashboard/
    │   │   └── app.dashboard.js
    │   ├── residents/
    │   │   ├── app.register.js
    │   │   ├── app.search.js
    │   │   └── app.detail.js
    │   └── settings/
    │       └── app.settings.js
    ├── services/
    │   ├── api.js              # Tauri invoke wrappers
    │   └── logger.js           # Frontend logging facade
    └── shared/
        └── helpers.js          # Pure/reusable helper functions
```

### 4.1 Frontend folder responsibilities

| Folder or file | Owns | Must not own |
|---|---|---|
| `src/index.html` | Page markup, Alpine bindings, script/style references | Database queries or large business workflows |
| `src/card-template.html` | CR100 card markup and print-specific layout | Database access or application startup |
| `src/css/` | Styles, responsive rules, design tokens | JavaScript behavior or data access |
| `src/icons/` | UI icon assets | Source code |
| `src/vendor/` | Third-party files intentionally vendored for offline use | Project-authored feature code |
| `src/js/app/` | Alpine application state and feature behavior | Direct SQL or filesystem access |
| `src/js/app/residents/` | Register, search, and detail UI workflows | Shared infrastructure implementations |
| `src/js/app/settings/` | Settings screens and user-triggered maintenance actions | Low-level encryption implementation |
| `src/js/app/dashboard/` | Dashboard-specific presentation and derived display state | Database persistence |
| `src/js/services/` | Tauri IPC wrappers and frontend logging facade | View markup and feature-specific UI state |
| `src/js/shared/` | Reusable, low-dependency functions | Calls that mutate feature state or depend on Alpine components |
| `src/js/bootstrap/` | Early startup configuration required before other scripts | Feature workflows |

### 4.2 Frontend rules

1. Keep the app on its current static HTML + Alpine.js architecture unless a separate, approved task justifies a build-system migration.
2. Keep classic `<script>` loading semantics unless there is a deliberate, separately tested migration to ES modules. Do not casually add `type="module"`, `defer`, bundlers, or dynamic imports.
3. Preserve the current script execution order. `verbose.js` must run before `logger.js`; shared dependencies must load before the feature scripts that consume them; `app.js` remains last unless the architecture is intentionally changed and verified.
4. After moving a file, update every affected reference in `index.html`, tests, `tools/check-css.mjs`, documentation, scripts, and any other path consumer. Search the whole repository for the old path.
5. The existing JavaScript modules use shared browser globals. Document any global they publish or consume. Do not create new implicit globals. Prefer an explicit namespace or a small documented interface when adding new shared state, without breaking existing callers.
6. Keep `helpers.js` dependency-light and testable. Put pure transformations and validation helpers there only when they are useful across features. Do not place Tauri calls in this file.
7. Keep `api.js` as the single frontend boundary for Tauri `invoke` calls. Feature modules call named API wrappers rather than scattering raw `invoke()` calls throughout the UI.
8. Keep UI notifications and logging behavior behind existing services. Do not duplicate logging or toast implementations in feature modules.
9. Keep CSS class names and selectors stable during file organization. A file move alone must not redesign the interface.
10. Keep the CR100 template independent from resident database access. It should receive prepared card data from the existing print flow.

### 4.3 Important CSS fingerprint rule

The current `tools/check-css.mjs` checks the HTML/JavaScript inputs used to guard the prebuilt CSS fingerprint. The root `AGENTS.md` documents the current check as covering `src/index.html` and JavaScript files under `src/js/` while excluding vendored libraries.

If JavaScript files are moved into nested folders:

- Inspect `tools/check-css.mjs` before changing the tree.
- Update its file discovery to include nested project-owned JavaScript files.
- Exclude `src/vendor/` (and any other vendored code) from the project fingerprint.
- Keep file ordering deterministic, for example by sorting normalized relative paths.
- Update tests for the fingerprint tool if present.
- Run `npm run check:css` after the move.
- Run `npm run stamp:css` only when the relevant HTML/JavaScript/CSS inputs intentionally changed and the fingerprint is meant to be refreshed. Never stamp merely to hide a failed or incomplete check.

## 5. Recommended Rust backend structure

### 5.1 Immediate structure: retain the cohesive modules that already exist

The current backend has useful module boundaries. Do not split or relocate every Rust file just to create more folders. For normal maintenance, preserve this simple structure unless a module has become difficult to understand or test.

```text
src-tauri/
├── Cargo.toml
├── Cargo.lock
├── build.rs
├── tauri.conf.json
├── icons/
├── assets/                       # Keep only if currently used by Tauri/build configuration
└── src/
    ├── main.rs                   # Startup, shared state setup, Tauri command registration
    ├── db.rs                     # SQLite pool, AppState, schema/init, project-local data paths
    ├── residents.rs              # Resident command handlers and resident persistence rules
    ├── id_card.rs                # QR/card data preparation and card batch operations
    ├── config.rs                 # Civic/application configuration operations
    ├── backup.rs                 # Database backup and restore operations
    ├── export.rs                 # Excel import/export operations
    ├── encryption.rs             # Tauri-facing encryption operations
    ├── crypto.rs                 # Cryptographic primitives and file encryption helpers
    ├── scheduled.rs              # Scheduled backup and photo cleanup workflows
    └── logger.rs                 # Backend logging and log path handling
```

The filenames above describe the current module responsibilities. Confirm the actual working tree before moving, renaming, or adding anything. Do not create empty placeholder files for modules that do not exist.

### 5.2 Backend boundaries

- `main.rs` is the composition root: initialize the app, build shared state, register commands, and start Tauri. Keep business logic out of it.
- Tauri commands are the IPC boundary. Preserve command names and request/response contracts consumed by `src/js/services/api.js` (currently `src/js/api.js` before migration).
- `residents.rs` owns resident operations and resident-specific rules. Avoid duplicating resident validation or SQL in unrelated modules.
- `db.rs` owns database initialization, connection state, schema work, and the project-local database path contract. Keep SQL parameterized; never interpolate user input into SQL strings.
- `crypto.rs` owns cryptographic primitives and file-encryption mechanics. `encryption.rs` owns application/command workflows that use those primitives. Do not duplicate cryptographic routines.
- `backup.rs` owns backup/restore behavior; `scheduled.rs` owns scheduling and orchestration. Reuse the same safe backup and photo-cleanup functions instead of forking copies.
- `export.rs` owns Excel import/export mechanics. Keep import validation and record integrity checks on the Rust side.
- `id_card.rs` owns preparation of data required by the ID/QR and CR100 print flow, not the HTML template itself.
- `logger.rs` owns backend logging setup and file output. Logging failures must not unexpectedly break normal resident operations.

### 5.3 Optional backend growth structure

Only if a backend module becomes too large, mixes unrelated responsibilities, or becomes hard to test, migrate that feature gradually toward this structure:

```text
src-tauri/src/
├── main.rs
├── commands/                     # Thin Tauri command/IPC adapters
│   ├── mod.rs
│   ├── residents.rs
│   ├── settings.rs
│   ├── backup.rs
│   ├── export.rs
│   └── encryption.rs
├── domain/                       # Core types and rules independent of Tauri/SQLite
│   ├── mod.rs
│   ├── resident.rs
│   └── resident_validation.rs
├── services/                     # Application workflows/orchestration
│   ├── mod.rs
│   ├── resident_service.rs
│   ├── backup_service.rs
│   ├── export_service.rs
│   └── encryption_service.rs
├── repositories/                 # Data persistence and retrieval
│   ├── mod.rs
│   └── resident_repository.rs
└── infrastructure/               # Technical adapters and operating-system concerns
    ├── mod.rs
    ├── database.rs
    ├── crypto.rs
    ├── filesystem.rs
    └── logger.rs
```

This is a future option, not an immediate requirement. Do not create all these layers pre-emptively. Introduce only the layer needed to solve a demonstrated problem, migrate one feature at a time, and remove the old implementation only after all callers and tests use the new one.

When a feature is split:

1. Keep the command adapter thin: parse/validate the IPC input shape, obtain state, call the service, and map the result into the established response type.
2. Put business rules in domain types or services, not in Tauri command registration code.
3. Put SQL and database row mapping in repositories or the existing database module, not in HTML-facing command wrappers.
4. Keep cryptography, filesystem access, and logging behind infrastructure functions with explicit inputs and results.
5. Avoid circular module dependencies. Lower-level modules must not call command handlers.
6. Use Rust visibility deliberately (`pub`, `pub(crate)`, private). Expose only what another module needs.
7. Update `mod.rs` declarations and `main.rs` command registration carefully. `tauri::generate_handler![]` must still register every command expected by the frontend.
8. Keep Rust module names and source filenames in `snake_case`.

Do not introduce a migrations directory unless the database is actually converted to a migration-based workflow. Do not move `Cargo.toml`, `Cargo.lock`, `build.rs`, or `tauri.conf.json` as part of ordinary source organization.

## 6. Root-level and supporting directory policy

Use the following ownership rules. These are intended to clarify existing paths, not to authorize moving existing content.

| Path | Purpose | Policy |
|---|---|---|
| `src/` | Frontend source and static assets | Application source only |
| `src-tauri/` | Rust/Tauri backend and Tauri resources | Backend source/config/build files |
| `tests/` | Automated tests and safe test fixtures | Never use live resident data |
| `tools/` | Developer scripts such as CSS checks | Keep scripts deterministic and documented |
| `docs/` | New developer/architecture documentation, if needed | Do not move existing user documents here without approval |
| `Documents/` | Existing application documents and generated exports such as `Documents/Excel/` | Preserve current runtime/output contract |
| `gathered-md/` | Existing collected Markdown references | Treat as reference material; do not confuse with authoritative project docs |
| `config/` | Local application configuration | Preserve project-local paths and ignore secrets/local state |
| `data/` | SQLite database, resident photos, backups | Preserve all contents; never use live records as test fixtures |
| `logs/` | Runtime logs | Keep generated logs out of version control |
| `tmp/` | Temporary files and WebView2/runtime scratch data | Keep inside project root; do not store durable user data here |
| `src-tauri/target/` | Rust build output | Keep under project root as already configured |
| `.crypto-key/` | Local encryption key material | Sensitive; never commit, print, copy into docs, or expose in logs |
| `.github/workflows/` | CI workflow definitions | Update when checks or project paths change |

Never move project runtime paths outside the repository root. Preserve the existing isolation mandate in `AGENTS.md`. In particular, never relocate, overwrite, reset, or use live `data/residents.db`, `data/photos/`, `data/backups/`, local config, or `.crypto-key/` as disposable test material.

## 7. File naming standards

Use names that tell a reader what the file does. Avoid names that describe when the file was made or whether it was a temporary attempt.

### 7.1 JavaScript

- Existing `app.*.js` feature filenames may be retained during migration to avoid needless churn.
- For new standalone JavaScript files, use lowercase `kebab-case.js` where that matches the established codebase style (for example, `resident-validation.js`, `date-helpers.js`).
- Use `camelCase` for variables and functions: `residentRecord`, `searchResidents()`.
- Use `PascalCase` for constructor/class names where classes are used.
- Use `UPPER_SNAKE_CASE` for true constants: `MAX_PHOTO_SIZE`.
- Do not introduce a second file for the same responsibility with suffixes such as `-new`, `-final`, `-v2`, or `-backup`.

### 7.2 Rust

- Files and modules: `snake_case.rs` / `snake_case`.
- Functions and variables: `snake_case`.
- Structs, enums, traits, and type aliases: `PascalCase`.
- Constants and statics: `SCREAMING_SNAKE_CASE`.
- Prefer names that state the domain action, such as `save_resident`, `encrypt_database`, and `get_dashboard_stats`.

### 7.3 HTML, CSS, tests, and tools

- HTML files: lowercase descriptive names with hyphens, such as `card-template.html`.
- CSS files: lowercase descriptive names; use one principal application stylesheet unless a real separation is needed.
- CSS classes: follow the existing naming scheme and design conventions in `DESIGN.md`; do not invent competing class systems during a file move.
- Tests: use the existing `*.test.js` pattern and name the behavior or area tested, such as `resident-search.test.js`.
- Tools: name by the action they perform, such as `check-css.mjs` or `verify-structure.mjs`.
- Documentation: use clear names such as `PROJECT_STRUCTURE.md`, `DEVELOPMENT_GUIDE.md`, and `TESTING_GUIDE.md`.

### 7.4 Never use ambiguous names

Avoid `misc.js`, `common.js` for unrelated functions, `helpers2.js`, `newfile.rs`, `final-final.js`, `old_code.rs`, `temp.js`, or `test2.js`. Before creating a new file, search for an existing file that already owns the responsibility.

## 8. File headers, labels, and documentation

Important files should be self-explanatory at a glance. Add a concise header when a file has non-obvious responsibilities, meaningful dependencies, exported globals, or side effects. Do not add boilerplate to every trivial file.

### 8.1 JavaScript header pattern

```javascript
/*
 * Module: Resident Search
 * Purpose: Search and filter resident records in the application UI.
 * Owns: Search state, filter selection, pagination behavior.
 * Depends on: Shared app state and the Tauri API service.
 * Publishes: [list actual global names or callbacks, if any].
 * Side effects: [network/IPC calls, timers, DOM changes, or none].
 * Must not: Execute SQL or access the filesystem directly.
 */
```

Use only facts that are true for the file. Keep the header short and update it when responsibility changes.

### 8.2 Rust module and function documentation

Use `//!` module comments to describe a module's purpose and boundary. Use `///` comments for public functions, structs, enums, and traits. For non-trivial functions, document relevant inputs, returned values/errors, and side effects. Mention important invariants where a future maintainer could otherwise break them.

```rust
//! Resident registration and persistence operations.
//!
//! Owns resident-specific command handling and validation rules.
//! Database access must use parameterized SQL through the established
//! database state/connection path.
```

Do not write comments that merely translate a line of code into English. Prefer explaining why a rule exists, what must remain true, or what side effect a caller must account for.

### 8.3 Documentation ownership

When a structural change is completed, update the documentation that owns the affected information:

- Root `AGENTS.md`: permanent agent instructions, actual module inventory, important commands, data paths, test gates, and known environment limitations.
- `README.md`: user-facing project overview, setup, run/build instructions, high-level architecture, and current directory overview.
- `DESIGN.md`: design tokens, components, visual states, and styling rules; read it before UI/CSS work.
- `docs/PROJECT_STRUCTURE.md` (create only if useful): detailed directory tree and file ownership for developers.
- `docs/DEVELOPMENT_GUIDE.md` (create only if useful): local development and debugging workflow.
- `docs/TESTING_GUIDE.md` (create only if useful): test commands, fixtures, platform constraints, and manual verification.

Do not duplicate the entire root `AGENTS.md` in this companion guide or copy all technical history into the README. Keep each document authoritative for one subject.

## 9. Safe restructuring procedure

A restructuring is complete only when the resulting code and references work. A visually tidy tree is not sufficient.

### Phase A — Preflight and baseline

1. Read `AGENTS.md`, this guide, `README.md`, `DESIGN.md`, `package.json`, `.gitignore`, `src/index.html`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, CI workflows, and relevant tests.
2. Inspect `git status --short --branch`. Do not overwrite, discard, stash, or silently absorb the user's uncommitted work. Follow the branch rules in root `AGENTS.md`.
3. List the actual files and directories. Do not rely on an old tree in documentation.
4. Search for every reference to files being moved: `src` paths, script tags, CSS/HTML links, test fixtures, Node scripts, PowerShell/batch scripts, Rust `mod` declarations, `include_*` paths, Tauri configuration, CI YAML, docs, and ignore rules.
5. Record a baseline: current test results, CSS fingerprint status, Rust formatting/metadata status, and any known platform-specific build limitation.
6. Identify data-bearing or sensitive paths. Do not open, copy, print, or modify real resident records or encryption keys as part of a structural audit.
7. Write a short migration plan listing exact moves, all reference updates, test changes, risks, and rollback method before editing.

### Phase B — Design the smallest useful change

1. State the problem the new folder or module solves.
2. Identify its single responsibility and allowed dependencies.
3. Confirm the proposed layout does not introduce cycles or duplicate ownership.
4. Prefer one small, reviewable migration over a large batch of unrelated moves.
5. Keep source logic unchanged when the task is only to reorganize files. Separate behavior changes into their own task or commit.
6. Do not add dependencies or convert the build architecture just to support folder moves.

### Phase C — Move and update references

For each move or rename:

1. Move one logical group at a time.
2. Update all references in the same change. Search again for the old path.
3. Preserve the frontend script order and browser-global behavior.
4. Preserve Rust module paths and every Tauri command registration.
5. Update CSS fingerprint discovery when JavaScript paths become nested.
6. Update tests that load source files by hard-coded path, especially tests that evaluate the real helper source.
7. Update root and feature-level documentation so it points to existing files only.
8. Review `.gitignore` to ensure runtime and sensitive files stay protected. Do not loosen ignore rules just to make a local test pass.

### Phase D — Review and validate

1. Review `git diff --check` and `git status --short`.
2. Inspect the final tree and compare it with the documented structure.
3. Search for stale paths, duplicate implementations, dead references, and accidental generated files.
4. Run the verification checklist in Section 10.
5. Perform manual smoke checks on the supported Windows target for UI startup, registration, search, resident details, status updates, photo handling, backup/restore, Excel import/export, encryption status/actions, and CR100 preview/print as applicable to the changed paths.
6. Report checks that could not run. Never describe an unrun check as passed.

### Phase E — Completion report

The completion report must state:

- What changed and why.
- The final directory structure or the exact changed paths.
- Which references, tests, or documentation were updated.
- Commands run and their actual results.
- Tests or platform checks not run, with the reason.
- Any remaining risks or follow-up work.

Do not claim the structure is fully verified if any required path, test, or command remains unchecked.

## 10. Required verification checklist

Run the checks that are relevant to the change. Follow the root `AGENTS.md` if its requirements are stricter.

### Frontend and repository checks

```bash
npm test
npm run check:css
git diff --check
```

The current `npm test` script runs six suites: `smoke.test.js`, `regression.test.js`, `golden-card.test.js`, `bulk-progress.test.js`, `logic.test.js`, and `structure.test.js`. The root `AGENTS.md` records a baseline of 57 passing tests; verify the current baseline rather than assuming that count is permanently fixed.

If HTML/JavaScript markup or CSS-relevant inputs were intentionally changed and the CSS fingerprint must be refreshed:

```bash
npm run stamp:css
npm run check:css
```

Do not use the stamp command to bypass a failed CSS freshness check.

### Rust checks

```bash
cd src-tauri
cargo fmt --all -- --check
cargo metadata --locked
cargo check
cargo test
cargo clippy --all-targets --all-features
```

Run `cargo audit` if the tool is installed and the task affects Rust dependencies or security posture. Do not install tools or dependencies without need and authorization.

The current root `AGENTS.md` documents native Linux build limitations on the project's development host and CI-based Rust compile/test validation. Re-check the current environment. If `cargo check`, `cargo test`, or Clippy cannot run because required native libraries/toolchains are missing, report the exact error and use the project's CI or supported Windows build path for authoritative compilation. Do not mislabel an environment failure as a Rust source failure or as a passing build.

### Runtime and data-isolation checks

- Confirm the app still starts from the supported Windows launch scripts.
- Confirm the database, photos, backups, logs, configuration, temporary data, WebView2 data, and Rust build output remain within the repository root as required by `AGENTS.md`.
- Confirm `.crypto-key/`, resident database files, private config, logs, and real photos remain ignored or otherwise excluded from version control as intended.
- Confirm no live data was modified, copied into tests, or added to the Git diff.
- Confirm CR100 output remains HTML-based and the print template path resolves.

## 11. Testing and fixture rules

1. Tests must use synthetic resident records, isolated temporary project-local directories, or dedicated fixtures that contain no real personal data.
2. Never use a copy of the live database as a fixture.
3. Never put names, birth dates, addresses, contact details, resident photos, encryption keys, or backups from actual residents in test files, screenshots, logs, or documentation.
4. Add or update tests when a move changes a script path, filesystem path, command contract, or CSS fingerprint input discovery.
5. Keep the test suite independent of network access. The application is designed for offline use.
6. A structural test should check important contracts (required files, script order, expected directories, ignore rules) without hard-coding every incidental file. Update it when an intentional structural change occurs.
7. Keep test helpers separate from production modules. Do not add test-only conditionals to application code unless there is a justified, explicit design.

## 12. Anti-patterns and prohibited shortcuts

Do not:

- Move every file merely to make the directory tree appear more layered.
- Split a small, cohesive Rust module into multiple layers without a concrete maintenance or testing benefit.
- Convert the static frontend to a bundler or ES modules as part of a folder cleanup.
- Change Tauri command names, payloads, or return types during a pure structure task.
- Change SQLite schema, resident ID generation, soft-delete semantics, backup atomicity, encryption format, photo handling, or CR100 print geometry during a pure structure task.
- Modify CSS design or UI behavior when the requested task is only organizational.
- Create duplicate `api`, `logger`, `helpers`, validation, database, or cryptography implementations.
- Hide errors with broad `catch` blocks, `.unwrap()` additions, ignored test failures, or disabled checks.
- Delete a file because it appears unused without checking all references, dynamic usage, configuration, and tests.
- Rename files only for personal preference when the rename adds no clear value.
- Place runtime data, generated artifacts, secrets, or live resident information in source folders or Git history.
- Add empty folders, empty module files, or speculative abstractions with no current caller.
- Update a documentation tree without checking that every listed path exists.

## 13. Definition of done

A repository-structure task is done only when all applicable points are true:

- [ ] The root `AGENTS.md` and this guide were followed.
- [ ] The actual starting structure and user changes were inspected before edits.
- [ ] Each changed or added file has one clear responsibility and a descriptive name.
- [ ] Frontend and backend responsibilities remain separated.
- [ ] Frontend classic-script order and required globals still work, unless an intentional migration was separately approved and tested.
- [ ] Rust module declarations and Tauri command registrations still resolve.
- [ ] All path references, scripts, tests, CI definitions, ignore rules, and docs were updated.
- [ ] The CSS fingerprint tool still covers all project-owned JavaScript files after any nested move.
- [ ] Tests and applicable validation commands pass, or failures are reported precisely.
- [ ] Runtime paths and offline behavior remain inside the project root.
- [ ] No real resident data, encryption key, or other sensitive material was exposed or changed.
- [ ] The final documented tree matches the actual repository.
- [ ] The final report accurately states the changes, checks, and limitations.

---

## 14. First adoption task for an agent

When this guide is first introduced, do not immediately execute the full target restructure. Perform an audit-only pass first:

1. Compare the current repository tree with the frontend and backend structures in Sections 4 and 5.
2. Mark each target path as **already present**, **safe candidate**, **requires reference updates**, **not currently needed**, or **must remain in place**.
3. Identify stale documentation and exact references that would break if any file moved.
4. Inspect `tools/check-css.mjs` and `tests/structure.test.js` for assumptions about flat JavaScript paths.
5. Produce a short, ordered migration plan with risk notes and validation commands.
6. Do not move or rename files during this audit-only pass. Wait for explicit authorization before applying structural changes.

**Guiding rule:** Make the codebase easier to navigate without making it harder to run, test, secure, or understand.
