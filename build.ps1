#Requires -Version 5.1
<#
.SYNOPSIS
    Barangay ID System - portable release build (Tauri NSIS).
.DESCRIPTION
    Run:
      powershell -ExecutionPolicy Bypass -File .\build.ps1
    Output stays isolated: backend/src/target/x86_64-pc-windows-msvc/release/bundle/nsis\ + portable exe
    Transcript: logs\build-YYYYMMDD-HHmmss.log
    Portable pins before any npm/cargo:
      NPM_CONFIG_CACHE=./.npm-cache CARGO_TARGET_DIR=./backend/src/target TMP/TEMP/TMPDIR=./tmp
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
    function Ensure-DataDir { param([string]$Root) $d = Join-Path $Root 'data'; if (-not (Test-Path $d)) { New-Item -ItemType Directory -Path $d -Force | Out-Null } }
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
    # Single-quoted here-string on purpose: the text mentions $Prereqs, and under
    # Set-StrictMode -Version Latest a double-quoted here-string threw
    # "The variable '$Prereqs' cannot be retrieved because it has not been set".
    Write-Host @'
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
Cargo.lock / frontend/src/index.html / README.md (see -NoBump) so the compiled binary
and installers carry the new version. The highest
version found in those files is the base and the others are synced to it, so a tree whose manifests
disagree repairs itself instead of aborting the build.
Output: backend/src/target/x86_64-pc-windows-msvc/release/bundle/nsis/ and bundle/msi/ + portable exe
Transcript: logs/build-YYYYMMDD-HHmmss.log (isolated)
Prereqs (fail-fast gate): Node 20+, npm, Rust MSVC, MSVC linker. Optional (warn-only, no action
needed): NSIS 3 (Tauri downloads it into backend/src\target\.tauri on first build) and rustup.
Future deps = one row in the $Prereqs table (Name/Level/Test/Fix). The MSI leg also downloads the WebView2 offline installer at build time (network needed).
Portable pins BEFORE any npm/cargo: NPM_CONFIG_CACHE=./.npm-cache CARGO_TARGET_DIR=./backend/src/target TMP/TEMP/TMPDIR=./tmp
Build includes get_cards_batch (bulk print) + optimize_database (E2) - no extra flags needed.
For comment-based help: Get-Help .\build.ps1 -Detailed
'@
    try { Get-Help $MyInvocation.MyCommand.Path -Detailed -ErrorAction SilentlyContinue | Out-String | Write-Host } catch {}
    Restore-EnvPins
    exit 0
}

# ---- Manifest version bump -------------------------------------------------
# Each version slot is located by its own SHAPE, never by the previous number,
# so a manifest that has already drifted is repaired instead of skipped. The
# lockfile's root "version" line ends with a comma, which is why the previous
# `"$`-anchored pattern silently never matched it (logs/build-20261106-*.log).
# All slots are resolved before the first write, and Cargo.toml is anchored to
# column 0 so a dependency pin can never be rewritten.
function Invoke-PatchVersionBump {
    param([string]$Root)

    $noBom = New-Object System.Text.UTF8Encoding $false
    $manifests = @(
        @{ File = 'package.json'; Label = 'package.json'
           Read = '(?m)^  "version": "([^"]+)"'
           Write = '(?m)^(  "version": ")[^"]+(")' },
        @{ File = 'package-lock.json'; Label = 'package-lock.json (root)'
           Read = '(?m)^  "version": "([^"]+)"'
           Write = '(?m)^(  "version": ")[^"]+(")' },
        @{ File = 'package-lock.json'; Label = 'package-lock.json (packages entry)'
           Read = '"name": "barangay-id-system",[ \t]*\r?\n[ \t]*"version": "([^"]+)"'
           Write = '("name": "barangay-id-system",[ \t]*\r?\n[ \t]*"version": ")[^"]+(")' },
        @{ File = 'backend/src/tauri.conf.json'; Label = 'backend/src/tauri.conf.json'
           Read = '"version": "([^"]+)"'
           Write = '("version": ")[^"]+(")' },
        @{ File = 'backend/src/Cargo.toml'; Label = 'backend/src/Cargo.toml'
           Read = '(?m)^version = "([^"]+)"'
           Write = '(?m)^(version = ")[^"]+(")' },
        @{ File = 'backend/src/Cargo.lock'; Label = 'backend/src/Cargo.lock'
           Read = 'name = "barangay-id-system"\r?\nversion = "([^"]+)"'
           Write = '(name = "barangay-id-system"\r?\nversion = ")[^"]+(")' },
        @{ File = 'frontend/src/index.html'; Label = 'frontend/src/index.html (About Version)'
           Read = 'Version</div><div class="about-kv__value font-mono">([^<]+)</div>'
           Write = '(Version</div><div class="about-kv__value font-mono">)[^<]+(</div>)' },
        @{ File = 'README.md'; Label = 'README.md (About Version)'
           Read = '\*\*Version:\*\* ([^ ]+)'
           # The About line is '**Version:** 1.0.10 · Identifier: ...' — the
           # version token is followed by ' ·', not '**', so match only the
           # token (what Read captures) and replace it in place.
           Write = '(\*\*Version:\*\* )[^ ]+' }
    )

    # 1. Resolve every slot. Highest version found is the base, so a tree whose
    #    manifests disagree is synced up instead of skipped.
    $slots = @()
    foreach ($m in $manifests) {
        $fp = Join-Path $Root $m.File
        if (-not (Test-Path $fp)) { throw "Auto-version: $($m.File) not found at $fp" }
        $text = [IO.File]::ReadAllText($fp, [System.Text.Encoding]::UTF8)
        $hit = [regex]::Match($text, $m.Read)
        if (-not $hit.Success) { throw "Auto-version: no version slot in $($m.Label) - pattern drift, fix the `$manifests table in Invoke-PatchVersionBump" }
        if ($hit.Groups[1].Value -notmatch '^\d+\.\d+\.\d+$') { throw "Auto-version: $($m.Label) version '$($hit.Groups[1].Value)' is not x.y.z - fix it manually" }
        $slots += @{ File = $m.File; Label = $m.Label; Version = $hit.Groups[1].Value; Text = $text; Write = $m.Write }
    }
    $base = ($slots | ForEach-Object { [version]$_.Version } | Sort-Object | Select-Object -Last 1)
    $newVer = "$($base.Major).$($base.Minor).$($base.Build + 1)"
    Write-Host "[Barangay ID] Auto-version: $($base.ToString(3)) -> $newVer" -ForegroundColor Cyan
    $versions = @($slots | ForEach-Object { $_.Version } | Select-Object -Unique)
    if ($versions.Count -gt 1) {
        Write-Host "[Barangay ID] Manifest versions disagreed ($($versions -join ', ')) - syncing every slot to $newVer" -ForegroundColor DarkGray
        foreach ($s in $slots) { Write-Host "  $($s.Label) was $($s.Version)" -ForegroundColor DarkGray }
    }

    # 2. Every slot must be writable before the first write, so a pattern
    #    mismatch can never leave a half-bumped tree behind.
    $unwritable = @($slots | Where-Object { -not [regex]::IsMatch($_.Text, $_.Write) })
    if ($unwritable.Count -gt 0) {
        throw ("Auto-version: cannot rewrite these slots (pattern drift, fix the `$manifests table in Invoke-PatchVersionBump):`n" +
               (($unwritable | ForEach-Object { "  - $($_.Label)" }) -join "`n"))
    }

    # 3. Write each slot from the file's CURRENT text (package-lock.json holds
    #    two slots), then re-read and confirm the new version stuck.
    foreach ($s in $slots) {
        $fp = Join-Path $Root $s.File
        $current = [IO.File]::ReadAllText($fp, [System.Text.Encoding]::UTF8)
        # Only slots whose Write pattern defines a 2nd capture group get '${2}':
        # README's About line only replaces the version token in place (1 group),
        # everything else preserves a closing delimiter (2 groups). Skipping '${2}'
        # for a 1-group pattern avoids the literal '${2}' leak .NET emits.
        $mCount = [regex]::Match($s.Text, $s.Write)
        $hasGroup2 = ($mCount.Success -and $mCount.Groups.Count -ge 3)
        $suffix = if ($hasGroup2) { '${2}' } else { '' }
        $replacement = '${1}' + $newVer + $suffix
        [IO.File]::WriteAllText($fp, [regex]::Replace($current, $s.Write, $replacement), $noBom)
    }
    $wrong = @()
    foreach ($m in $manifests) {
        $hit = [regex]::Match([IO.File]::ReadAllText((Join-Path $Root $m.File), [System.Text.Encoding]::UTF8), $m.Read)
        $actual = if ($hit.Success) { $hit.Groups[1].Value } else { '<no match>' }
        if ($actual -ne $newVer) { $wrong += "  - $($m.Label) is $actual, expected $newVer" }
    }
    if ($wrong.Count -gt 0) { throw ("Auto-version: verification failed after write:`n" + ($wrong -join "`n")) }
}

# ---- -Audit: dependency audit, warn-only (parity with dev launcher) ----
if ($Audit) {
    Write-Host '[Barangay ID] Dependency audit (isolated caches)...' -ForegroundColor Cyan
    $prevEap = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    try {
        if (Get-Command 'cargo' -ErrorAction SilentlyContinue) {
            try { Invoke-SafeNative cargo audit --file (Join-Path $Root 'backend/src\Cargo.lock') | Out-Host }
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
    # WiX/NSIS stay 'warn' on purpose: Tauri v1 provisions its own copies
    # (tauri.bundle.useLocalToolsDir) and never uses a system install.
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
        @{ Name = 'NSIS makensis (installer leg)'; Level = 'warn'; Fix = 'Tauri provisions this itself on the first build - no manual install needed';
           Test = { if (Test-Path (Join-Path $Root 'backend/src\target\.tauri\NSIS\makensis.exe')) { return $true }
                    if (Get-Command 'makensis.exe' -ErrorAction SilentlyContinue) { return $true }
                    return (Test-Path 'C:\Program Files (x86)\NSIS\makensis.exe') } },
        @{ Name = 'rustup (auto-adds the x86_64-pc-windows-msvc target below)'; Level = 'warn'; Fix = 'https://rustup.rs';
           Test = { [bool](Get-Command 'rustup' -ErrorAction SilentlyContinue) } }
    )
    $missing = @()
    foreach ($p in $Prereqs) {
        $ok = $false
        try { $ok = [bool](& $p.Test) } catch { $ok = $false }
        if ($ok) { Write-Host "ok: $($p.Name)" -ForegroundColor Green }
        elseif ($p.Level -eq 'warn') { Write-Host "optional: $($p.Name) not found - $($p.Fix)" -ForegroundColor Yellow }
        else { $missing += "  - $($p.Name): $($p.Fix)" }
    }
    if ($missing.Count -gt 0) { throw ("Missing build dependencies:`n" + ($missing -join "`n") + "`nInstall the above, then re-run .\build.ps1") }
    Write-Host '[note] The installer uses the WebView2 bootstrapper (network needed the first time it installs; the app itself is offline-first)' -ForegroundColor DarkGray
    try { $v = (Invoke-SafeNative node --version | Out-String).Trim(); if ($LASTEXITCODE -ne 0) { throw "exit $LASTEXITCODE" }; Write-Host "node $v" -ForegroundColor Green
        $m = [regex]::Match($v, 'v(\d+)'); if ($m.Success -and [int]$m.Groups[1].Value -lt 20) { Write-Host '[WARN] Node < 20: npm test needs Node 20.11+ (import.meta.dirname)' -ForegroundColor Yellow } } catch { throw 'Node/npm not found - https://nodejs.org' }
    try { $v = (Invoke-SafeNative npm --version | Out-String).Trim(); if ($LASTEXITCODE -ne 0) { throw "exit $LASTEXITCODE" }; Write-Host "npm $v (cache $env:NPM_CONFIG_CACHE)" -ForegroundColor Green } catch { throw 'npm not found - reinstall Node.js' }
    try { $v = (Invoke-SafeNative rustc --version | Out-String).Trim(); if ($LASTEXITCODE -ne 0) { throw "exit $LASTEXITCODE" }; Write-Host "$v" -ForegroundColor Green } catch { throw 'Rust not found - https://rustup.rs (needed for tauri build)' }
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
    # NSIS-only now: stale .msi references gone with the MSI target removal above.
    if (-not $NoClean) {
        if ($PSCmdlet.ShouldProcess('bundle/', 'Remove stale bundle outputs before build')) {
            $bundleDir = Join-Path $Root 'backend/src\target\x86_64-pc-windows-msvc\release\bundle'
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
    # -NoBump skips (use for rebuilds of the same version). The ShouldProcess
    # gate stays here in script scope, so -WhatIf behaves exactly as before;
    # the version work itself lives in Invoke-PatchVersionBump, defined above.
    if (-not $NoBump) {
        if ($PSCmdlet.ShouldProcess('manifests', 'Auto-bump patch version x.y.z -> x.y.z+1 across all manifests')) {
            Invoke-PatchVersionBump -Root $Root
            # index.html changed (About Version), so restamp the CSS fingerprint
            # to keep `npm run check:css` green after every build.
            Write-Host '[Barangay ID] Restamping CSS fingerprint (index.html version changed)...' -ForegroundColor Cyan
            & node tools/check-css.mjs --update | Out-Host
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
    if (-not (Test-Path $tauriCmd)) { throw 'tauri.cmd still missing after install. Run npm install manually or use -NoBump to skip version bump.' }

    $prevEap=$ErrorActionPreference; $ErrorActionPreference='Continue'
    # Tauri CLI must run from backend/src (where tauri.conf.json lives).
    Push-Location (Join-Path $Root 'backend\src') | Out-Null
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
    } finally {
        $ErrorActionPreference=$prevEap
        Pop-Location | Out-Null # always return to project root
    }
    if ($LASTEXITCODE -ne 0) { throw "tauri build (x86_64-pc-windows-msvc) failed (exit $LASTEXITCODE)" }

    # NSIS-only output: drop the standalone portable EXE so the only
    # deliverable artifact is the NSIS installer EXE under bundle/nsis/.
    $portableExe = Join-Path $Root 'backend/src\target\x86_64-pc-windows-msvc\release\barangay-id-system.exe'
    if (Test-Path $portableExe) {
        Remove-Item $portableExe -Force -ErrorAction Stop
        Write-Host '[Barangay ID] Removed portable EXE (NSIS-only output).' -ForegroundColor Green
    }

    Write-Host "`n[Barangay ID] NSIS installer EXE output (bundle/nsis/):" -ForegroundColor Green
    Get-ChildItem (Join-Path $Root 'backend/src\target\x86_64-pc-windows-msvc\release\bundle') -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Extension -eq '.exe' } | Select-Object FullName, Length, LastWriteTime | Format-Table -AutoSize
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
