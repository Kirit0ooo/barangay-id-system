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
[CmdletBinding(SupportsShouldProcess)]
param([switch]$Help, [switch]$Audit, [switch]$NoBump, [switch]$NoClean, [switch]$EnableVerboseLogging)

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
trap {
    Restore-EnvPins
    Write-Host "ERROR: terminating error in build.ps1 - $_" -ForegroundColor Red
    exit 1
}

# ---- Portable env pins (MUST precede any npm/cargo call) ----
Set-PortableEnvPins $Root
if ($EnableVerboseLogging) {
    Write-Verbose '[Barangay ID] Verbose build mode enabled'
    $env:RUST_LOG = 'debug'
    Write-Host '[Barangay ID] Verbose: RUST_LOG=debug' -ForegroundColor Magenta
}
Write-Verbose '[Barangay ID] Portable env pins applied (isolated caches, no %TEMP% writes)'

# Ensure data dir exists for SQLite (first-run tauri dev/build may need it).
Ensure-DataDir $Root

if ($Help) {
    Write-Host @"
Barangay ID System - build.ps1 (portable release build)
Usage:
  powershell -ExecutionPolicy Bypass -File .\build.ps1
  powershell -ExecutionPolicy Bypass -File .\build.ps1 -Audit   # dependency audit only: cargo audit + npm audit (isolated caches, never fails the build)
  build.bat             # thin wrapper -> build.ps1 %* (same env pins)
  build.bat -Help       # forwarded to build.ps1
  build.bat -NoBump     # skip auto patch-version increment
  build.bat -NoClean    # keep previous bundle/ outputs
Pre-build: stale bundle/ outputs removed (see -NoClean); patch version auto-increments
x.y.z -> x.y.z+1 across package.json / package-lock.json / tauri.conf.json / Cargo.toml /
Cargo.lock (see -NoBump) so the compiled binary and installers carry the new version.
Output: src-tauri/target/x86_64-pc-windows-msvc/release/bundle/ (msi/ + nsis/) + portable exe
Transcript: logs/build-YYYYMMDD-HHmmss.log (isolated)
Prereqs (fail-fast $Prereqs gate): Node 20+, npm, Rust MSVC, MSVC linker, WiX v3.11, NSIS, rustup (warn-only).
Future deps = one $Prereqs row (Name/Level/Test/Fix). MSI also downloads the WebView2 offline installer at build time (network needed).
Portable pins BEFORE any npm/cargo: NPM_CONFIG_CACHE=./.npm-cache CARGO_TARGET_DIR=./src-tauri/target TMP/TEMP/TMPDIR=./tmp
Build includes get_cards_batch (bulk print) + optimize_database (E2) - no extra flags needed.
For comment-based help: Get-Help .\build.ps1 -Detailed
"@
    try { Get-Help $MyInvocation.MyCommand.Path -Detailed -ErrorAction SilentlyContinue | Out-String | Write-Host } catch {}
    Restore-EnvPins
    exit 0
}

# ---- -Audit: dependency audit, warn-only (parity with run.ps1 -Audit) ----
if ($Audit) {
    Write-Host '[Barangay ID] Dependency audit (isolated caches)...' -ForegroundColor Cyan
    $prevEap = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    try {
        if (Get-Command 'cargo' -ErrorAction SilentlyContinue) {
            try { Invoke-SafeNative cargo audit --file (Join-Path $Root 'src-tauri\Cargo.lock') | Out-Host }
            catch { Write-Host '[audit] cargo-audit not installed - run: cargo install cargo-audit' -ForegroundColor Yellow }
        } else { Write-Host '[audit] cargo not found - skipping Rust audit' -ForegroundColor Yellow }
        Invoke-SafeNative npm audit --audit-level=moderate | Out-Host
        if ($LASTEXITCODE -ne 0) { Write-Host '[audit] npm audit reported issues at/above moderate (see above) - warn-only, build not failed' -ForegroundColor Yellow }
    } finally { $ErrorActionPreference = $prevEap }
    Write-Host '[Barangay ID] Audit done (warn-only).' -ForegroundColor Green
    Restore-EnvPins
    exit 0
}

$logFile = Join-Path $Root ("logs\build-" + (Get-Date -Format 'yyyyMMdd-HHmmss') + ".log")
Write-Host "[Barangay ID] Building release installer (log: $logFile)..." -ForegroundColor Cyan
try { Start-Transcript -Path $logFile -Append | Out-Null } catch { Write-Host "[WARN] Start-Transcript failed" -ForegroundColor Yellow }

try {
    # Dependency gate (fail-fast): every tool the release build needs, in one
    # table so FUTURE deps are a one-row addition (Name/Level/Test/Fix).
    # Level 'required' throws once listing ALL missing tools; 'warn' prints
    # yellow and continues. Tests must be side-effect free (Get-Command /
    # Test-Path only) - version echoes come after the gate passes.
    $Prereqs = @(
        @{ Name = 'Node.js 20+'; Level = 'required'; Fix = 'https://nodejs.org (LTS 20+; CI pins 20, npm test needs import.meta.dirname)';
           Test = { [bool](Get-Command 'node' -ErrorAction SilentlyContinue) } },
        @{ Name = 'npm (ships with Node.js)'; Level = 'required'; Fix = 'reinstall Node.js from https://nodejs.org';
           Test = { [bool](Get-Command 'npm' -ErrorAction SilentlyContinue) } },
        @{ Name = 'Rust toolchain (rustc)'; Level = 'required'; Fix = 'https://rustup.rs (stable, x64 MSVC host)';
           Test = { [bool](Get-Command 'rustc' -ErrorAction SilentlyContinue) } },
        @{ Name = 'MSVC linker (link.exe - cargo needs it to link the exe)'; Level = 'required';
           Fix = 'Visual Studio Build Tools 2022 + "Desktop development with C++": https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022';
           Test = { if (Get-Command 'link.exe' -ErrorAction SilentlyContinue) { return $true }
                    $vsWhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
                    if (Test-Path $vsWhere) {
                        $vsPath = (& $vsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null | Out-String).Trim()
                        if ($vsPath -and (Test-Path (Join-Path $vsPath 'VC\Tools\MSVC'))) { return $true }
                    }
                    return (Test-Path (Join-Path ${env:ProgramFiles} 'Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC')) } },
        @{ Name = 'WiX Toolset v3.11 (MSI leg; Tauri v1 rejects v4+)'; Level = 'required'; Fix = 'https://wixtoolset.org/releases/ (v3.11)';
           Test = { if (Test-Path (Join-Path ${env:ProgramFiles(x86)} 'WiX Toolset v3.11\bin\candle.exe')) { return $true }
                    if (Get-Command 'candle.exe' -ErrorAction SilentlyContinue) {
                        $ver = (& candle.exe -version 2>$null | Out-String).Trim()
                        if ($ver -match 'v3\.11') { return $true }
                        Write-Host "[wix] candle.exe on PATH is v4+ (Tauri v1 needs v3.11): $ver" -ForegroundColor Yellow
                    }
                    return $false } },
        @{ Name = 'NSIS makensis (nsis leg)'; Level = 'required'; Fix = 'https://nsis.sourceforge.io/Download';
           Test = { if (Get-Command 'makensis.exe' -ErrorAction SilentlyContinue) { return $true }
                    return (Test-Path 'C:\Program Files (x86)\NSIS\makensis.exe') } },
        @{ Name = 'rustup (auto-adds the x86_64-pc-windows-msvc target below)'; Level = 'warn'; Fix = 'https://rustup.rs';
           Test = { [bool](Get-Command 'rustup' -ErrorAction SilentlyContinue) } }
    )
    $missing = @()
    foreach ($p in $Prereqs) {
        $ok = $false
        try { $ok = [bool](& $p.Test) } catch { $ok = $false }
        if ($ok) { Write-Host "ok: $($p.Name)" -ForegroundColor Green }
        elseif ($p.Level -eq 'warn') { Write-Host "missing (warn-only): $($p.Name) - $($p.Fix)" -ForegroundColor Yellow }
        else { $missing += "  - $($p.Name): $($p.Fix)" }
    }
    if ($missing.Count -gt 0) { throw ("Missing build dependencies:`n" + ($missing -join "`n") + "`nInstall the above, then re-run .\build.ps1") }
    Write-Host '[note] MSI embeds the WebView2 offline installer (~150MB download at build time - network required even though the app itself is offline-first)' -ForegroundColor DarkGray
    try { $v = (Invoke-SafeNative node --version | Out-String).Trim(); if ($LASTEXITCODE -ne 0) { throw "exit $LASTEXITCODE" }; Write-Host "node $v" -ForegroundColor Green
        $m = [regex]::Match($v, 'v(\d+)'); if ($m.Success -and [int]$m.Groups[1].Value -lt 20) { Write-Host '[WARN] Node < 20: npm test needs Node 20.11+ (import.meta.dirname)' -ForegroundColor Yellow } } catch { throw 'Node/npm not found - https://nodejs.org' }
    try { $v = (Invoke-SafeNative npm --version | Out-String).Trim(); if ($LASTEXITCODE -ne 0) { throw "exit $LASTEXITCODE" }; Write-Host "npm $v (cache $env:NPM_CONFIG_CACHE)" -ForegroundColor Green } catch { throw 'npm not found - reinstall Node.js' }
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

    # Stale-output clean: drop previous bundle/ installers so an outdated
    # .msi/.nsis can never be mistaken for the fresh build. -NoClean skips.
    # Only bundle/ is removed (never release/ - that would force a full recompile).
    if (-not $NoClean) {
        if ($PSCmdlet.ShouldProcess('bundle/', 'Remove stale bundle outputs before build')) {
            $bundleDir = Join-Path $Root 'src-tauri\target\x86_64-pc-windows-msvc\release\bundle'
            if (Test-Path $bundleDir) {
                $stale = @(Get-ChildItem $bundleDir -Recurse -File -ErrorAction SilentlyContinue)
                if ($stale.Count -gt 0) {
                    Write-Host "[Barangay ID] Removing $($stale.Count) stale bundle file(s)..." -ForegroundColor Yellow
                    Remove-Item (Join-Path $bundleDir '*') -Recurse -Force -ErrorAction Stop
                } else {
                    Write-Host '[Barangay ID] Bundle dir already clean' -ForegroundColor Green
                }
            }
        }
    }

    # Auto-version: bump patch x.y.z -> x.y.z+1 across every manifest BEFORE
    # tauri build, so the compiled binary and installers carry the new version.
    # -NoBump skips (use for rebuilds of the same version).
    if (-not $NoBump) {
        if ($PSCmdlet.ShouldProcess('manifests', 'Auto-bump patch version x.y.z -> x.y.z+1 across all manifests')) {
            $pkgJsonPath = Join-Path $Root 'package.json'
            $oldVer = (Get-Content $pkgJsonPath -Raw | ConvertFrom-Json).version
            if ($oldVer -notmatch '^\d+\.\d+\.\d+$') { throw "Auto-version: package.json version '$oldVer' is not x.y.z - fix it manually" }
            $vp = $oldVer -split '\.'
            $newVer = "$($vp[0]).$($vp[1]).$([int]$vp[2] + 1)"
            Write-Host "[Barangay ID] Auto-version: $oldVer -> $newVer" -ForegroundColor Cyan
            $oldRx = [regex]::Escape($oldVer)
            $noBom = New-Object System.Text.UTF8Encoding $false
            $bumpStages = @(
                @{ File = 'package.json'; Pattern = '"version": "' + $oldRx + '"'; Replacement = '"version": "' + $newVer + '"' },
                @{ File = 'package-lock.json'; Pattern = '(?m)^  "version": "' + $oldRx + '"$'; Replacement = '  "version": "' + $newVer + '"' },
                @{ File = 'package-lock.json'; Pattern = '"name": "barangay-id-system",(\r?\n)      "version": "' + $oldRx + '"'; Replacement = '"name": "barangay-id-system",$1      "version": "' + $newVer + '"' },
                @{ File = 'src-tauri\tauri.conf.json'; Pattern = '"version": "' + $oldRx + '"'; Replacement = '"version": "' + $newVer + '"' },
                @{ File = 'src-tauri\Cargo.toml'; Pattern = 'version = "' + $oldRx + '"'; Replacement = 'version = "' + $newVer + '"' },
                @{ File = 'src-tauri\Cargo.lock'; Pattern = 'name = "barangay-id-system"(\r?\n)version = "' + $oldRx + '"'; Replacement = 'name = "barangay-id-system"$1version = "' + $newVer + '"' }
            )
            foreach ($stage in $bumpStages) {
                $fp = Join-Path $Root $stage.File
                $txt = [IO.File]::ReadAllText($fp, [System.Text.Encoding]::UTF8)
                $txt = [regex]::Replace($txt, $stage.Pattern, $stage.Replacement)
                [IO.File]::WriteAllText($fp, $txt, $noBom)
                $check = [IO.File]::ReadAllText($fp, [System.Text.Encoding]::UTF8)
                if ($check -notmatch [regex]::Escape($newVer)) { throw "Auto-version: $($stage.File) still pins $oldVer (pattern drift) - update build.ps1 bump stages" }
            }
        }
    }

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
        Restore-EnvPins
        exit 0
    } catch {
        if ($_.FullyQualifiedErrorId -match 'PipelineStopped' -or "$($_.Exception.Message)" -match 'pipeline.*stopped') {
            Write-Host 'Build stopped by user (Ctrl+C) - normal exit.' -ForegroundColor Yellow
            Restore-EnvPins
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
    Write-Host "Build log saved to $logFile" -ForegroundColor Green
}
