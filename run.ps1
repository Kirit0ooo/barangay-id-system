#Requires -Version 5.1
<#
.SYNOPSIS
    Barangay ID System - canonical portable Windows launcher (Tauri dev).
.DESCRIPTION
    Double-click or run:
      powershell -ExecutionPolicy Bypass -File .\run.ps1
    Flags:
      .\run.ps1 -Logs        # tail isolated logs (app.log.* + frontend.log + latest run/build) and exit - supersedes view-logs.bat logic; wrapper remains
      .\run.ps1 -Fix         # deep clean: remove node_modules + .npm-cache contents, npm ci->install, verify tauri.cmd (historical fix-tauri.bat folded in)
      .\run.ps1 -NoInstall   # skip auto npm install, fail fast if tauri.cmd missing
      .\run.ps1 -Help        # usage
      .\run.ps1 -Audit       # warn-only cargo audit + npm audit (parity with build.ps1 -Audit)
      .\run.ps1 -EnableVerboseLogging  # verbose console: RUST_LOG=debug + frontend debug
    Thin wrappers (same env pins, forward %*): run.bat (dev), build.bat (build.ps1), view-logs.bat (run.ps1 -Logs + explorer).
    Portable contract: every artifact (npm cache, cargo target, tmp,
    WebView2 data, logs, SQLite) stays inside this project folder.
    No writes to %APPDATA%, %TEMP%, drive root, or home.
    Env pins are set BEFORE any npm/cargo/node call:
      NPM_CONFIG_CACHE=./.npm-cache  CARGO_TARGET_DIR=./src-tauri/target
      TMP/TEMP/TMPDIR=./tmp  WEBVIEW2_USER_DATA_FOLDER=./tmp/webview2
    R5 awareness: scheduled backup (wal_checkpoint -> data/backups/scheduled_*) + photo GC
    (orphan scan under data/photos) run at Rust runtime via scheduled.rs + frontend 24h interval
    - no launcher action needed, but npm install stays reproducible via package-lock.json so new
    scheduled/photo_gc/DOB helpers are picked up on next auto-install.
    R6: CSP carries no 'unsafe-eval' (removed in M1 prebuilt-CSS pass; pinned by tests) - launcher does not touch CSP.
    P1-4: get_cards_batch Tauri command serves bulk print (one IPC round-trip for many IDs).
    E1: duplicate-person detection runs at runtime on create/update - no launcher flag needed.
    E2: optimize_database Tauri command (PRAGMA optimize + incremental_vacuum, 7-day cadence)
    is available at runtime - no launcher flag needed.
    E4: photo processing (normalize/validate/identify) has a 10s timeout budget at runtime.
    helpers.js holds extracted pure helper functions for testability (picked up via normal npm install).
.NOTES
    Isolation verified by tests/smoke.test.js + tests/regression.test.js - no global cache.
    Transcript: logs/run-YYYYMMDD-HHmmss.log (isolated, never %TEMP%)
#>
[CmdletBinding()]
param(
    [switch]$Logs,
    [switch]$NoInstall,
    [switch]$Fix,
    [switch]$Audit,
    [switch]$EnableVerboseLogging,
    [switch]$Help
)

# ---- StrictMode + error policy (catch typos, fail fast) ----
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# ---- Dot-source shared launcher primitives (Invoke-SafeNative + env pins) ----
$CommonModule = Join-Path $PSScriptRoot 'tools\launcher-common.ps1'
if (-not (Test-Path $CommonModule)) {
    Write-Host "[WARN] launcher-common.ps1 not found at $CommonModule - inline fallbacks only" -ForegroundColor Yellow
    # Inline fallbacks so unconditional calls below never hard-crash under StrictMode.
    function Set-PortableEnvPins { param([string]$Root) Write-Host '[fallback] Set-PortableEnvPins stub (launcher-common.ps1 missing)' -ForegroundColor Yellow }
    function Restore-EnvPins { Write-Host '[fallback] Restore-EnvPins stub (launcher-common.ps1 missing)' -ForegroundColor Yellow }
    function Invoke-SafeNative { param([string]$Exe, [Parameter(ValueFromRemainingArguments=$true)][string[]]$Arguments) & $Exe @Arguments }
} else {
    . $CommonModule
}

$Root = $PSScriptRoot
if (-not $Root) { $Root = Split-Path -Parent $MyInvocation.MyCommand.Path }
Set-Location $Root

# ---- Capture original env for restore-on-exit (never leak pins to parent shell) ----
# Restore-EnvPins is called in every catch + finally block below.

# ---- Portable env pins (MUST precede any npm/cargo/node call) ----
Set-PortableEnvPins $Root
if ($EnableVerboseLogging) {
    $env:RUST_LOG = 'debug'
    $env:BARANGAY_VERBOSE = '1'
    Write-Verbose '[Barangay ID] Verbose: RUST_LOG=debug + BARANGAY_VERBOSE=1'
    Write-Host '[Barangay ID] Verbose: RUST_LOG=debug + BARANGAY_VERBOSE=1 (console + app.log + frontend console)' -ForegroundColor Magenta
}
Write-Verbose '[Barangay ID] Portable env pins applied (isolated caches, no %TEMP% writes)'

# Ensure data dir exists for SQLite (first-run tauri dev may need it).
Ensure-DataDir $Root

if ($Help) {
    Write-Host @"
Barangay ID System - run.ps1 (canonical portable launcher)
Usage:
  powershell -ExecutionPolicy Bypass -File .\run.ps1            # dev (auto npm ci/install, tauri dev)
  .\run.ps1 -Logs        # tail isolated logs (app.log.* + frontend.log + latest run/build) and exit - replaces view-logs.bat
  .\run.ps1 -Fix         # deep clean node_modules + .npm-cache, npm ci->install, verify tauri.cmd - historical fix-tauri.bat folded in
  .\run.ps1 -NoInstall   # fail fast if tauri.cmd missing
  .\run.ps1 -EnableVerboseLogging     # verbose console: RUST_LOG=debug + frontend debug (same as `$env:RUST_LOG='debug'; .\run.ps1`)
  .\run.ps1 -Audit       # warn-only cargo audit + npm audit (parity with build.ps1 -Audit)
  .\run.ps1 -Help        # this help
  run.bat [flags]        # thin wrapper -> run.ps1 %* (same env pins); build.bat -> build.ps1; view-logs.bat -> run.ps1 -Logs

Portable contract: all artifacts stay inside this folder (no writes to %APPDATA%/%TEMP%).
Env pins BEFORE any npm/cargo: NPM_CONFIG_CACHE=./.npm-cache CARGO_TARGET_DIR=./src-tauri/target TMP/TEMP/TMPDIR=./tmp WEBVIEW2_USER_DATA_FOLDER=./tmp/webview2
R5: scheduled backup (wal_checkpoint -> data/backups/scheduled_*) + photo GC run inside app (Rust runtime, 24h interval) - launcher just ensures deps via package-lock.json.
Runtime features (no launcher flags needed): get_cards_batch (P1 bulk print), duplicate-person check (E1),
optimize_database (E2, 7-day cadence), 10s photo processing budget (E4).
Transcript: logs/run-YYYYMMDD-HHmmss.log (isolated, never %TEMP%)
Use -Logs to tail: .\run.ps1 -Logs   |   view-logs.bat adds Explorer open.
Verbose: .\run.ps1 -EnableVerboseLogging  (or `$env:RUST_LOG='debug'; .\run.ps1)
For comment-based help: Get-Help .\run.ps1 -Detailed
"@
    try { Get-Help $MyInvocation.MyCommand.Path -Detailed -ErrorAction SilentlyContinue | Out-String | Write-Host } catch {}
    Restore-EnvPins
    exit 0
}

# ---- -Logs flag: tail isolated logs and exit (merges view-logs.bat) ----
if ($Logs) {
    Write-Host "=== Isolated Logs: $Root\logs ===" -ForegroundColor Cyan
    $logsDir = Join-Path $Root 'logs'
    if (Test-Path $logsDir) {
        Get-ChildItem $logsDir -File -ErrorAction SilentlyContinue |
            Select-Object Name, Length, LastWriteTime |
            Format-Table -AutoSize
    } else {
        Write-Host '(logs folder missing - run the app first)' -ForegroundColor Yellow
    }

    # frontend.log (last 40 lines)
    $frontendLog = Join-Path $Root 'logs\frontend.log'
    Write-Host "`n--- logs/frontend.log (last 40 lines) ---" -ForegroundColor Yellow
    if (Test-Path $frontendLog) { Get-Content $frontendLog -Tail 40 -ErrorAction SilentlyContinue } else { Write-Host '(missing - run the app first)' }

    # app.log.* - pick latest rolling file (app.log.YYYY-MM-DD) or legacy app.log
    Write-Host "`n--- logs/app.log.* (last 40 lines, latest file) ---" -ForegroundColor Yellow
    $appLogs = @(Get-ChildItem (Join-Path $Root 'logs\app.log*') -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^app\.log' } |
        Sort-Object LastWriteTime -Descending)
    if ($appLogs -and $appLogs.Count -gt 0) {
        $latest = $appLogs[0]
        Write-Host "File: $($latest.Name) (LastWriteTime: $($latest.LastWriteTime))" -ForegroundColor DarkGray
        Get-Content $latest.FullName -Tail 40 -ErrorAction SilentlyContinue
        if ($appLogs.Count -gt 1) {
            Write-Host "`nOther app.log files: $($appLogs.Count - 1) older (see logs folder)" -ForegroundColor DarkGray
        }
    } else {
        Write-Host '(no app.log* found - run the app first)'
    }

    # latest run-*.log transcript
    $latestRun = Get-ChildItem (Join-Path $Root 'logs\run-*.log') -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($latestRun) {
        Write-Host "`n--- $($latestRun.Name) (last 40 lines) ---" -ForegroundColor Yellow
        Get-Content $latestRun.FullName -Tail 40 -ErrorAction SilentlyContinue
    } else {
        Write-Host "`n--- run-*.log: (no transcript yet - run without -Logs once) ---" -ForegroundColor DarkGray
    }

    # latest build-*.log transcript
    $latestBuild = Get-ChildItem (Join-Path $Root 'logs\build-*.log') -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($latestBuild) {
        Write-Host "`n--- $($latestBuild.Name) (last 40 lines) ---" -ForegroundColor Yellow
        Get-Content $latestBuild.FullName -Tail 40 -ErrorAction SilentlyContinue
    } else {
        Write-Host "`n--- build-*.log: (no build transcript yet) ---" -ForegroundColor DarkGray
    }

    Write-Host "`nFull logs folder: $Root\logs" -ForegroundColor Cyan
    Write-Host "Tip: view-logs.bat (if present) also opens Explorer; or: explorer logs" -ForegroundColor DarkGray
    Restore-EnvPins
    exit 0
}

# ---- -Audit: dependency audit, warn-only (parity with build.ps1 -Audit) ----
if ($Audit) {
    Write-Host '[Barangay ID] Dependency audit (isolated caches, warn-only)...' -ForegroundColor Cyan
    $prevEap = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    try {
        if (Get-Command 'cargo' -ErrorAction SilentlyContinue) {
            try { Invoke-SafeNative cargo audit --file (Join-Path $Root 'src-tauri\Cargo.lock') | Out-Host }
            catch { Write-Host '[audit] cargo-audit not installed - run: cargo install cargo-audit' -ForegroundColor Yellow }
        } else { Write-Host '[audit] cargo not found - skipping Rust audit' -ForegroundColor Yellow }
        Invoke-SafeNative npm audit --audit-level=moderate | Out-Host
        if ($LASTEXITCODE -ne 0) { Write-Host '[audit] npm audit reported issues at/above moderate (see above) - warn-only' -ForegroundColor Yellow }
    } finally { $ErrorActionPreference = $prevEap }
    Write-Host '[Barangay ID] Audit done (warn-only).' -ForegroundColor Green
    Restore-EnvPins
    exit 0
}

# ---- Transcript (isolated, never %TEMP%) ----
$logFile = Join-Path $Root ("logs\run-" + (Get-Date -Format 'yyyyMMdd-HHmmss') + ".log")
Write-Host "[Barangay ID] Logging to $logFile (isolated)" -ForegroundColor Cyan
Write-Host "[Barangay ID] Working dir: $Root" -ForegroundColor Cyan
Write-Host "[Barangay ID] Data dir:   $Root\data" -ForegroundColor Cyan
Write-Host "[Barangay ID] Logs dir:   $Root\logs" -ForegroundColor Cyan
Write-Host "[Barangay ID] npm-cache:  $($env:NPM_CONFIG_CACHE)" -ForegroundColor Cyan
Write-Host "[Barangay ID] cargo target: $($env:CARGO_TARGET_DIR)" -ForegroundColor Cyan
try { Start-Transcript -Path $logFile -Append | Out-Null } catch { Write-Host "[WARN] Start-Transcript failed: $($_.Exception.Message)" -ForegroundColor Yellow }

$failed = $false
try {
    # ---- Robust prereq checks with clear messages (EAP Continue + npm notice filter) ----
    Write-Host '--- Prereq: Node ---' -ForegroundColor Yellow
    $nodeOk = $false
    try {
        $nodeVer = (Invoke-SafeNative node --version | Out-String)
        $nodeVer = $nodeVer.Trim()
        if ($LASTEXITCODE -eq 0 -and $nodeVer) {
            Write-Host "node $nodeVer" -ForegroundColor Green
            $nodeOk = $true
        } else { throw "node --version exit $LASTEXITCODE" }
    } catch {
        Write-Host 'Node.js not found - install LTS 20+ from https://nodejs.org' -ForegroundColor Red
        Write-Host "  Detail: $($_.Exception.Message)" -ForegroundColor DarkGray
        $failed = $true
    }

    Write-Host '--- Prereq: npm ---' -ForegroundColor Yellow
    $npmOk = $false
    try {
        $npmVer = (Invoke-SafeNative npm --version | Out-String)
        $npmVer = $npmVer.Trim()
        if ($LASTEXITCODE -eq 0 -and $npmVer) {
            Write-Host "npm $npmVer (cache: $env:NPM_CONFIG_CACHE)" -ForegroundColor Green
            $npmOk = $true
        } else { throw "npm --version exit $LASTEXITCODE" }
    } catch {
        Write-Host 'npm not found - reinstall Node.js (includes npm) from https://nodejs.org' -ForegroundColor Red
        Write-Host "  Detail: $($_.Exception.Message)" -ForegroundColor DarkGray
        $failed = $true
    }

    Write-Host '--- Prereq: Rust ---' -ForegroundColor Yellow
    $rustOk = $false
    try {
        $rustVer = (Invoke-SafeNative rustc --version | Out-String)
        $rustVer = $rustVer.Trim()
        if ($LASTEXITCODE -eq 0 -and $rustVer) {
            Write-Host "$rustVer" -ForegroundColor Green
            $rustOk = $true
        } else { throw "rustc --version exit $LASTEXITCODE" }
    } catch {
        Write-Host 'Rust not found - install from https://rustup.rs (needed for Tauri)' -ForegroundColor Red
        Write-Host "  Detail: $($_.Exception.Message)" -ForegroundColor DarkGray
        $failed = $true
    }

    Write-Host '--- Prereq: MSVC linker (Rust compile) ---' -ForegroundColor Yellow
    $msvcOk = $false
    try {
        # tauri dev compiles the Rust backend: link.exe must exist (x64 Native Tools
        # prompt works; plain PowerShell needs VS Build Tools on PATH or installed).
        $linkHit = $false
        try { $null = Get-Command link.exe -ErrorAction Stop; $linkHit = $true } catch {}
        if (-not $linkHit) {
            $vsWhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
            if (Test-Path $vsWhere) {
                $vsPath = (& $vsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null | Out-String).Trim()
                if ($vsPath -and (Test-Path (Join-Path $vsPath 'VC\Tools\MSVC'))) { $linkHit = $true }
            }
        }
        if ($linkHit) { Write-Host 'MSVC linker present' -ForegroundColor Green; $msvcOk = $true }
        else { throw 'link.exe not found and no VS VC.Tools install detected' }
    } catch {
        Write-Host 'MSVC Build Tools not found - install from https://visualstudio.microsoft.com/visual-cpp-build-tools/ (Desktop C++ workload; needed to compile Tauri)' -ForegroundColor Red
        Write-Host "  Detail: $($_.Exception.Message)" -ForegroundColor DarkGray
        $failed = $true
    }

    if ($failed) { throw 'Missing prerequisites - see messages above. Install missing tools and retry.' }
    if (-not $nodeOk -or -not $npmOk -or -not $rustOk -or -not $msvcOk) { throw 'Prereq check incomplete - aborting.' }

    # ---- -Fix: deep clean reinstall (historical fix-tauri.bat folded in) ----
    if ($Fix) {
        if ($NoInstall) { throw '-Fix and -NoInstall are mutually exclusive.' }
        Write-Host '[Barangay ID] -Fix: deep clean reinstall (isolated cache, keeps package-lock.json)...' -ForegroundColor Yellow
        $nmPath = Join-Path $Root 'node_modules'
        if (Test-Path $nmPath) {
            Write-Host "[Fix] Removing $nmPath ..." -ForegroundColor Yellow
            Remove-Item -Recurse -Force $nmPath -ErrorAction SilentlyContinue
        }
        $npmCachePath = Join-Path $Root '.npm-cache'
        if (Test-Path $npmCachePath) {
            Write-Host "[Fix] Clearing npm cache contents at $npmCachePath ..." -ForegroundColor Yellow
            Get-ChildItem $npmCachePath -Force -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
            try { Invoke-SafeNative npm cache verify | Out-String | Write-Host -ForegroundColor DarkGray } catch { Write-Host "[Fix] npm cache verify failed (non-fatal)" -ForegroundColor Yellow }
        }
        Write-Host "[Fix] Running npm install (isolated cache $npmCachePath)..." -ForegroundColor Yellow
        $lockExists = Test-Path (Join-Path $Root 'package-lock.json')
        if ($lockExists) {
            Write-Host "[Fix] package-lock.json found - trying npm ci (reproducible)..." -ForegroundColor Cyan
            Invoke-SafeNative npm ci | Tee-Object -Variable _fixOut | Out-Host
            if ($LASTEXITCODE -ne 0) {
                Write-Host "[Fix] npm ci failed (exit $LASTEXITCODE) - falling back to npm install..." -ForegroundColor Yellow
                Invoke-SafeNative npm install | Out-Host
                if ($LASTEXITCODE -ne 0) { throw "npm install failed after ci fallback (exit $LASTEXITCODE)" }
            }
        } else {
            Invoke-SafeNative npm install | Out-Host
            if ($LASTEXITCODE -ne 0) { throw "npm install failed (exit $LASTEXITCODE)" }
        }
        $tauriCmdAfterFix = Join-Path $Root 'node_modules\.bin\tauri.cmd'
        if (Test-Path $tauriCmdAfterFix) {
            Write-Host "[Fix] OK: node_modules\.bin\tauri.cmd exists" -ForegroundColor Green
            $prevEap=$ErrorActionPreference; $ErrorActionPreference='Continue'
            try { Invoke-SafeNative npx tauri --version | Write-Host -ForegroundColor Cyan } catch {} finally { $ErrorActionPreference=$prevEap }
        } else {
            throw 'ERROR: node_modules\.bin\tauri.cmd still missing after -Fix reinstall. Try: npm install @tauri-apps/cli@^1.5 --save-dev'
        }
        Write-Host '[Fix] Fix complete - continuing to tauri dev...' -ForegroundColor Green
    }

    # ---- Auto-install if tauri.cmd missing (non-destructive; historical fix-tauri.bat logic folded in) ----
    $tauriCmd = Join-Path $Root 'node_modules\.bin\tauri.cmd'
    if (-not (Test-Path $tauriCmd)) {
        if ($NoInstall) { throw 'tauri.cmd missing and -NoInstall set. Run without -NoInstall once, or use -Fix for a clean reinstall.' }
        Write-Host '[Barangay ID] Installing npm deps (isolated cache .\.npm-cache)...' -ForegroundColor Yellow
        $lockExists = Test-Path (Join-Path $Root 'package-lock.json')
        if ($lockExists) {
            Write-Host '[Barangay ID] package-lock.json found - trying npm ci (reproducible, includes R5 scheduled/photo_gc deps)...' -ForegroundColor Cyan
            Invoke-SafeNative npm ci | Out-Host
            if ($LASTEXITCODE -ne 0) {
                Write-Host "[Barangay ID] npm ci failed (exit $LASTEXITCODE) - falling back to npm install..." -ForegroundColor Yellow
                Invoke-SafeNative npm install | Out-Host
                if ($LASTEXITCODE -ne 0) { throw "npm install failed (exit $LASTEXITCODE)" }
            }
        } else {
            Invoke-SafeNative npm install | Out-Host
            if ($LASTEXITCODE -ne 0) { throw "npm install failed (exit $LASTEXITCODE)" }
        }
    } else {
        Write-Host '[Barangay ID] npm deps already installed' -ForegroundColor Green
    }
    if (-not (Test-Path $tauriCmd)) { throw 'ERROR: node_modules\.bin\tauri.cmd still missing after install. Try: .\run.ps1 -Fix  or  npm install' }

    Write-Host '[Barangay ID] tauri version:' -ForegroundColor Cyan
    $prevEap=$ErrorActionPreference; $ErrorActionPreference='Continue'
    try {
        Invoke-SafeNative npx tauri --version | Write-Host -ForegroundColor Cyan
        if ($LASTEXITCODE -ne 0) { throw "npx tauri --version failed (exit $LASTEXITCODE). Try: .\run.ps1 -Fix" }
    } catch {
        throw "npx tauri --version failed: $($_.Exception.Message). Try: .\run.ps1 -Fix"
    } finally { $ErrorActionPreference=$prevEap }

    Write-Host '[Barangay ID] Starting Tauri dev... (RUST_LOG=$env:RUST_LOG; verbose: $env:RUST_LOG="debug" .\run.ps1)' -ForegroundColor Cyan
    Write-Host '[Barangay ID] R5 scheduled backup/photo GC run inside app (24h interval) - no launcher flag needed.' -ForegroundColor DarkGray
    # NOTE: Excel uses pure-Rust rust_xlsxwriter - no libclang/LLVM needed.
    $prevEap=$ErrorActionPreference; $ErrorActionPreference='Continue'
    try {
        Invoke-SafeNative npx tauri dev | Out-Host
    } catch [System.Management.Automation.PipelineStoppedException] {
        # Ctrl+C / watcher teardown: expected user stop, not a failure. Transcript still closes in finally.
        Write-Host 'Tauri dev stopped by user (Ctrl+C) - normal exit.' -ForegroundColor Yellow
        Restore-EnvPins
        exit 0
    } catch {
        if ($_.FullyQualifiedErrorId -match 'PipelineStopped' -or "$($_.Exception.Message)" -match 'pipeline.*stopped') {
            Write-Host 'Tauri dev stopped by user (Ctrl+C) - normal exit.' -ForegroundColor Yellow
            Restore-EnvPins
            exit 0
        }
        throw
    } finally { $ErrorActionPreference=$prevEap }
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Tauri dev exited with $LASTEXITCODE. Logs: logs/app.log.*, logs/frontend.log, $logFile" -ForegroundColor Red
        Write-Host 'Try: npx tauri info  or  .\run.ps1 -Fix  (clean reinstall)' -ForegroundColor Yellow
        Write-Host "Tail isolated logs: .\run.ps1 -Logs" -ForegroundColor Yellow
        Restore-EnvPins
        exit $LASTEXITCODE
    }
    Write-Host "Tauri dev exited normally. Tail hint: .\run.ps1 -Logs  (or view-logs.bat if present)" -ForegroundColor Green
} catch [System.Management.Automation.PipelineStoppedException] {
    # Ctrl+C during any pipeline (tauri dev watch, Out-Host): expected stop, exit 0 so transcript shows clean end.
    Write-Host 'Stopped by user (Ctrl+C) - normal exit.' -ForegroundColor Yellow
    Restore-EnvPins
    exit 0
} catch {
    if ($_.FullyQualifiedErrorId -match 'PipelineStopped' -or "$($_.Exception.Message)" -match 'pipeline.*stopped') {
        Write-Host 'Stopped by user (Ctrl+C) - normal exit.' -ForegroundColor Yellow
        Restore-EnvPins
        exit 0
    }
    Write-Host "ERROR: $($_.Exception.Message)" -ForegroundColor Red
    Restore-EnvPins
    exit 1
} finally {
    try { Stop-Transcript | Out-Null } catch {}
    Restore-EnvPins
    Write-Host "Run log saved to $logFile" -ForegroundColor Green
}
