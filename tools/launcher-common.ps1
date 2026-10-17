# =============================================================================
# launcher-common.ps1 — shared launcher primitives for Barangay ID System
#
# Dot-sourced by run.ps1 and build.ps1 so `Invoke-SafeNative` + portable env
# pins live in ONE place (DRY). Byte-identical logic in both scripts reduces
# drift risk when fixing the `npm notice` / `PipelineStopped` quirks.
#
# Portable contract (never writes outside the project folder):
#   NPM_CONFIG_CACHE      = .\ .npm-cache
#   CARGO_TARGET_DIR      = .\src-tauri\target
#   TMP/TEMP/TMPDIR       = .\tmp
#   WEBVIEW2_USER_DATA_FOLDER = .\tmp\webview2
# =============================================================================

<#
.SYNOPSIS
    Shared helper module for Barangay ID System launchers.
.DESCRIPTION
    Provides Invoke-SafeNative (native-call safety wrapper that filters npm
    notice stderr so it is not logged as ERROR under $ErrorActionPreference='Stop')
    and Set-PortableEnvPins (pins all caches/temp dirs inside the project).
    Dot-source from run.ps1 / build.ps1:
        . $PSScriptRoot\tools\launcher-common.ps1
.NOTES
    All pins are relative to $Root (script root of the dot-sourcing script).
    Restore-EnvPins reverts overrides on exit so the parent shell is not leaked.
#>

# ---- Native call safety helper (shared, byte-identical in both scripts) ----
# Every native invocation that may emit 'npm notice' (npx, npm, node, cargo, rustc, rustup)
# MUST run with EAP Continue and filter npm notice so transcript does not log it as ERROR.
# On Windows npx/npm are shimmed as npx.cmd/npm.cmd; using .cmd explicitly avoids the
# PowerShell .ps1 shim which re-emits stderr as ErrorRecord under EAP Stop.
function Invoke-SafeNative {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Exe,
        [Parameter(ValueFromRemainingArguments=$true)][string[]]$Arguments
    )
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $actual = $Exe
        $isWin = $false
        try { if ($IsWindows) { $isWin = $true } } catch {}
        if (-not $isWin -and $env:OS -like '*Windows*') { $isWin = $true }
        if (-not $isWin -and $PSVersionTable.PSEdition -eq 'Desktop') { $isWin = $true } # Windows PowerShell 5.1 has no $IsWindows
        if ($isWin) {
            if ($Exe -eq 'npx' -and (Get-Command 'npx.cmd' -ErrorAction SilentlyContinue)) { $actual = 'npx.cmd' }
            elseif ($Exe -eq 'npm' -and (Get-Command 'npm.cmd' -ErrorAction SilentlyContinue)) { $actual = 'npm.cmd' }
        }
        # Inject --yes for npx tauri to suppress npm notice fetching; --silent for npm ci/install
        $effectiveArgs = $Arguments
        if ($Exe -eq 'npx' -and $Arguments -contains 'tauri' -and $Arguments -notcontains '--yes') {
            $effectiveArgs = @('--yes') + $Arguments
        }
        if ($Exe -eq 'npm' -and ($Arguments -contains 'ci' -or $Arguments -contains 'install') -and $Arguments -notcontains '--silent' -and $Arguments -notcontains '--loglevel') {
            $effectiveArgs = $Arguments + @('--silent')
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

# ---- Portable env pins (shared, MUST precede any npm/cargo/node call) ----
# Overrides prior values so nested dot-sourcing doesn't accumulate.
function Set-PortableEnvPins {
    param([string]$Root)
    $script:ORIG_NPM_CONFIG_CACHE    = $env:NPM_CONFIG_CACHE
    $script:ORIG_CARGO_TARGET_DIR    = $env:CARGO_TARGET_DIR
    $script:ORIG_TMP                 = $env:TMP
    $script:ORIG_TEMP                = $env:TEMP
    $script:ORIG_TMPDIR              = $env:TMPDIR
    $script:ORIG_WEBVIEW2_USER_DATA  = $env:WEBVIEW2_USER_DATA_FOLDER
    $script:ORIG_RUST_LOG            = $env:RUST_LOG

    $env:NPM_CONFIG_CACHE          = Join-Path $Root '.npm-cache'
    $env:CARGO_TARGET_DIR          = Join-Path $Root 'src-tauri\target'
    $env:TMP                       = Join-Path $Root 'tmp'
    $env:TEMP                      = Join-Path $Root 'tmp'
    $env:TMPDIR                    = Join-Path $Root 'tmp'
    $env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $Root 'tmp\webview2'
    if (-not $env:RUST_LOG) { $env:RUST_LOG = 'info' }

    # Ensure isolated dirs exist
    foreach ($d in @('logs', 'tmp', 'tmp\webview2', '.npm-cache')) {
        $p = Join-Path $Root $d
        if (-not (Test-Path $p)) { New-Item -ItemType Directory -Path $p | Out-Null }
    }
}

# ---- Restore env pins to pre-launcher values (call in finally / trap) ----
function Restore-EnvPins {
    $env:NPM_CONFIG_CACHE          = $script:ORIG_NPM_CONFIG_CACHE
    $env:CARGO_TARGET_DIR          = $script:ORIG_CARGO_TARGET_DIR
    $env:TMP                       = $script:ORIG_TMP
    $env:TEMP                      = $script:ORIG_TEMP
    $env:TMPDIR                    = $script:ORIG_TMPDIR
    $env:WEBVIEW2_USER_DATA_FOLDER = $script:ORIG_WEBVIEW2_USER_DATA
    $env:RUST_LOG                  = $script:ORIG_RUST_LOG
}

# ---- Ensure data dir exists (run.ps1 and build.ps1 both call it) ----
function Ensure-DataDir {
    param([string]$Root)
    $p = Join-Path $Root 'data'
    if (-not (Test-Path $p)) { New-Item -ItemType Directory -Path $p | Out-Null }
}
