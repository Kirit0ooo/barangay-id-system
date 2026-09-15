#Requires -Version 5.1
<#
.SYNOPSIS
    Barangay ID System - portable release build (Tauri MSI/NSIS).
.DESCRIPTION
    Run:
      powershell -ExecutionPolicy Bypass -File .\build.ps1
    Output stays isolated: src-tauri\target\x86_64-pc-windows-msvc\release\bundle\ (msi/ + nsis/) + portable exe
    Transcript: logs\build-YYYYMMDD-HHmmss.log
    Portable pins (same as run.ps1) before any npm/cargo:
      NPM_CONFIG_CACHE=./.npm-cache CARGO_TARGET_DIR=./src-tauri/target TMP/TEMP/TMPDIR=./tmp
    Build includes recent runtime commands: get_cards_batch (bulk print) + optimize_database
    (DB optimize, 7-day cadence) - plus E1 duplicate-person check and E4 10s photo budget.
#>
[CmdletBinding()]
param([switch]$Help, [switch]$Audit)

$ErrorActionPreference = 'Stop'
$Root = $PSScriptRoot
if (-not $Root) { $Root = Split-Path -Parent $MyInvocation.MyCommand.Path }
Set-Location $Root

# ---- Native call safety helper (DRY: keep byte-identical logic in run.ps1/build.ps1) ----
function Invoke-SafeNative {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Exe,
        [Parameter(ValueFromRemainingArguments=$true)][string[]]$Args
    )
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $actual = $Exe
        # On Windows use npx.cmd/npm.cmd to avoid the PowerShell .ps1 shim that re-throws stderr as TerminatingError under EAP Stop.
        # On non-Windows (Linux/macOS) the .cmd shim is not executable (Exec format error) - use plain npx/npm.
        $isWin = $false
        try { if ($IsWindows) { $isWin = $true } } catch {}
        if (-not $isWin -and $env:OS -like '*Windows*') { $isWin = $true }
        if (-not $isWin -and $PSVersionTable.PSEdition -eq 'Desktop') { $isWin = $true } # Windows PowerShell 5.1 has no $IsWindows
        if ($isWin) {
            if ($Exe -eq 'npx' -and (Get-Command 'npx.cmd' -ErrorAction SilentlyContinue)) { $actual = 'npx.cmd' }
            elseif ($Exe -eq 'npm' -and (Get-Command 'npm.cmd' -ErrorAction SilentlyContinue)) { $actual = 'npm.cmd' }
        }
        # Inject --yes for npx tauri to suppress npm notice fetching; --silent for npm ci/install
        $effectiveArgs = $Args
        if ($Exe -eq 'npx' -and $Args -contains 'tauri' -and $Args -notcontains '--yes') {
            $effectiveArgs = @('--yes') + $Args
        }
        if ($Exe -eq 'npm' -and ($Args -contains 'ci' -or $Args -contains 'install') -and $Args -notcontains '--silent' -and $Args -notcontains '--loglevel') {
            $effectiveArgs = $Args + @('--silent')
        }
        try {
            & $actual @effectiveArgs 2>&1 | ForEach-Object {
                $line = if ($_ -is [System.Management.Automation.ErrorRecord]) { $_.Exception.Message } else { [string]$_ }
                if ($line -match 'npm notice') { return } # filter - do not pollute transcript as ERROR
                $_
            }
        } catch [System.Management.Automation.PipelineStoppedException] {
            # Ctrl+C / watcher exit tears down Out-Host pipeline - expected stop, not an error.
        } catch {
            if ($_.FullyQualifiedErrorId -match 'PipelineStopped' -or "$($_.Exception.Message)" -match 'pipeline.*stopped') { return }
            throw
        }
    } finally {
        $ErrorActionPreference = $prevEap
    }
}

if ($Help) {
    Write-Host @"
Barangay ID System - build.ps1 (portable release build)
Usage:
  powershell -ExecutionPolicy Bypass -File .\build.ps1
  powershell -ExecutionPolicy Bypass -File .\build.ps1 -Audit   # dependency audit only: cargo audit + npm audit (isolated caches, never fails the build)
  build.bat             # thin wrapper -> build.ps1 %* (same env pins)
  build.bat -Help       # forwarded to build.ps1
Output: src-tauri/target/x86_64-pc-windows-msvc/release/bundle/ (msi/ + nsis/) + portable exe
Transcript: logs/build-YYYYMMDD-HHmmss.log (isolated)
Portable pins BEFORE any npm/cargo: NPM_CONFIG_CACHE=./.npm-cache CARGO_TARGET_DIR=./src-tauri/target TMP/TEMP/TMPDIR=./tmp
Build includes get_cards_batch (bulk print) + optimize_database (E2) - no extra flags needed.
For comment-based help: Get-Help .\build.ps1 -Detailed
"@
    try { Get-Help $MyInvocation.MyCommand.Path -Detailed -ErrorAction SilentlyContinue | Out-String | Write-Host } catch {}
    exit 0
}

# ---- Portable env pins (MUST precede any npm/cargo call) ----
$env:NPM_CONFIG_CACHE          = Join-Path $Root '.npm-cache'
$env:CARGO_TARGET_DIR          = Join-Path $Root 'src-tauri\target'
$env:TMP                       = Join-Path $Root 'tmp'
$env:TEMP                      = Join-Path $Root 'tmp'
$env:TMPDIR                    = Join-Path $Root 'tmp'
$env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $Root 'tmp\webview2'
if (-not $env:RUST_LOG) { $env:RUST_LOG = 'info' }

foreach ($d in @('logs', 'tmp', 'tmp\webview2', '.npm-cache')) {
    $p = Join-Path $Root $d
    if (-not (Test-Path $p)) {
        New-Item -ItemType Directory -Path $p | Out-Null
    }
}

if ($Audit) {
    # P2: dependency audit — isolated caches, warn-only (never fails).
    Write-Host '[Barangay ID] Dependency audit (isolated caches)...' -ForegroundColor Cyan
    $prevEap = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    try {
        if (Get-Command 'cargo' -ErrorAction SilentlyContinue) {
            try { Invoke-SafeNative cargo audit --file (Join-Path $Root 'src-tauri\Cargo.lock') | Out-Host }
            catch { Write-Host '[audit] cargo-audit not installed — run: cargo install cargo-audit' -ForegroundColor Yellow }
        } else { Write-Host '[audit] cargo not found — skipping Rust audit' -ForegroundColor Yellow }
        Invoke-SafeNative npm audit --audit-level=moderate | Out-Host
        if ($LASTEXITCODE -ne 0) { Write-Host '[audit] npm audit reported issues at/above moderate (see above) — warn-only, build not failed' -ForegroundColor Yellow }
    } finally { $ErrorActionPreference = $prevEap }
    Write-Host '[Barangay ID] Audit done (warn-only).' -ForegroundColor Green
    exit 0
}

$logFile = Join-Path $Root ("logs\build-" + (Get-Date -Format 'yyyyMMdd-HHmmss') + ".log")
Write-Host "[Barangay ID] Building release installer (log: $logFile)..." -ForegroundColor Cyan
try { Start-Transcript -Path $logFile -Append | Out-Null } catch { Write-Host "[WARN] Start-Transcript failed" -ForegroundColor Yellow }

try {
    # Robust prereq checks (EAP Continue + npm notice filter via Invoke-SafeNative)
    Write-Host '--- Prereq: Node ---' -ForegroundColor Yellow
    try { $v = (Invoke-SafeNative node --version | Out-String).Trim(); if ($LASTEXITCODE -ne 0) { throw "exit $LASTEXITCODE" }; Write-Host "node $v" -ForegroundColor Green } catch { throw 'Node/npm not found - https://nodejs.org' }
    try { $v = (Invoke-SafeNative npm --version | Out-String).Trim(); if ($LASTEXITCODE -ne 0) { throw "exit $LASTEXITCODE" }; Write-Host "npm $v (cache $env:NPM_CONFIG_CACHE)" -ForegroundColor Green } catch { throw 'npm not found - reinstall Node.js' }
    Write-Host '--- Prereq: Rust ---' -ForegroundColor Yellow
    try { $v = (Invoke-SafeNative rustc --version | Out-String).Trim(); if ($LASTEXITCODE -ne 0) { throw "exit $LASTEXITCODE" }; Write-Host "$v" -ForegroundColor Green } catch { throw 'Rust not found - https://rustup.rs (needed for tauri build MSI)' }
    $prevEap=$ErrorActionPreference; $ErrorActionPreference='Continue'
    # Skip the network install when the target is already present (fail-open:
    # any check failure falls through to the install, preserving old behavior).
    $needTarget = $true
    try {
        $installedTargets = (Invoke-SafeNative rustup target list --installed | Out-String)
        if ($LASTEXITCODE -eq 0 -and $installedTargets -match 'x86_64-pc-windows-msvc') {
            $needTarget = $false
            Write-Host '[Barangay ID] target x86_64-pc-windows-msvc already installed - skipping rustup install' -ForegroundColor Green
        }
    } catch {}
    if ($needTarget) {
        try { Invoke-SafeNative rustup target add x86_64-pc-windows-msvc | Out-Host } catch { Write-Host '[WARN] rustup target add failed - continuing (target may already be installed)' -ForegroundColor Yellow } finally { $ErrorActionPreference=$prevEap }
    } else { $ErrorActionPreference=$prevEap }

    $tauriCmd = Join-Path $Root 'node_modules\.bin\tauri.cmd'
    if (-not (Test-Path $tauriCmd)) {
        Write-Host '[Barangay ID] Installing npm deps (isolated cache .\.npm-cache)...' -ForegroundColor Yellow
        $lockExists = Test-Path (Join-Path $Root 'package-lock.json')
        if ($lockExists) {
            Invoke-SafeNative npm ci | Out-Host
            if ($LASTEXITCODE -ne 0) {
                Write-Host '[Barangay ID] npm ci failed - falling back to npm install...' -ForegroundColor Yellow
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
    if (-not (Test-Path $tauriCmd)) { throw 'tauri.cmd still missing after install. Try: .\run.ps1 -Fix' }

    $prevEap=$ErrorActionPreference; $ErrorActionPreference='Continue'
    try {
        Invoke-SafeNative npx tauri build --target x86_64-pc-windows-msvc | Out-Host
    } catch [System.Management.Automation.PipelineStoppedException] {
        Write-Host 'Build stopped by user (Ctrl+C) - normal exit.' -ForegroundColor Yellow
        exit 0
    } catch {
        if ($_.FullyQualifiedErrorId -match 'PipelineStopped' -or "$($_.Exception.Message)" -match 'pipeline.*stopped') {
            Write-Host 'Build stopped by user (Ctrl+C) - normal exit.' -ForegroundColor Yellow
            exit 0
        }
        throw
    } finally { $ErrorActionPreference=$prevEap }
    if ($LASTEXITCODE -ne 0) { throw "tauri build (x86_64-pc-windows-msvc) failed (exit $LASTEXITCODE)" }

    Write-Host "`n[Barangay ID] x64 release outputs (portable exe + msi/nsis bundles):" -ForegroundColor Green
    Get-ChildItem (Join-Path $Root 'src-tauri\target\x86_64-pc-windows-msvc\release') -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Extension -in @('.exe', '.msi') } | Select-Object Name, Length, LastWriteTime | Format-Table -AutoSize

    Write-Host "`n[Barangay ID] Bundles at: src-tauri\target\x86_64-pc-windows-msvc\release\bundle\ (msi/ + nsis/)" -ForegroundColor Green
    Get-ChildItem (Join-Path $Root 'src-tauri\target\x86_64-pc-windows-msvc\release\bundle') -Recurse -File -ErrorAction SilentlyContinue |
        Select-Object FullName, Length, LastWriteTime | Format-Table -AutoSize
} catch [System.Management.Automation.PipelineStoppedException] {
    Write-Host 'Stopped by user (Ctrl+C) - normal exit.' -ForegroundColor Yellow
    exit 0
} catch {
    if ($_.FullyQualifiedErrorId -match 'PipelineStopped' -or "$($_.Exception.Message)" -match 'pipeline.*stopped') {
        Write-Host 'Stopped by user (Ctrl+C) - normal exit.' -ForegroundColor Yellow
        exit 0
    }
    Write-Host "ERROR: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
} finally {
    try { Stop-Transcript | Out-Null } catch {}
    Write-Host "Build log saved to $logFile" -ForegroundColor Green
}
