@echo off
REM Barangay ID System - thin double-click wrapper around canonical run.ps1 (source of truth).
REM Flags are forwarded: run.bat -Logs | -Fix | -NoInstall | -Help | -Audit
REM Runtime features (no flags needed): get_cards_batch bulk print, E1 duplicate-person check,
REM E2 optimize_database (7-day cadence), E4 10s photo budget. See run.ps1 .DESCRIPTION.
setlocal
cd /d "%~dp0"

REM ---- Portable env pins (mirrors run.ps1; apply before any npm call) ----
set "NPM_CONFIG_CACHE=%CD%\.npm-cache"
set "CARGO_TARGET_DIR=%CD%\src-tauri\target"
set "TMP=%CD%\tmp"
set "TEMP=%CD%\tmp"
set "TMPDIR=%CD%\tmp"
set "WEBVIEW2_USER_DATA_FOLDER=%CD%\tmp\webview2"
if not exist "logs" mkdir "logs" 2>nul
if not exist "tmp\webview2" mkdir "tmp\webview2" 2>nul
if not exist ".npm-cache" mkdir ".npm-cache" 2>nul
where powershell >nul 2>nul
if errorlevel 1 goto :NOPS

REM Delegate to canonical launcher, forwarding all flags (-Logs -Fix -NoInstall -Help -Audit)
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0run.ps1" %*
exit /b %errorlevel%

:NOPS
REM Minimal fallback: ps1 is source of truth - do not duplicate dev logic here.
echo [Barangay ID] PowerShell not found - install PowerShell 5.1+ https://aka.ms/wmf5download
echo [Barangay ID] Then re-run run.bat. Flags (-Logs/-Fix/-NoInstall/-Help/-Audit) need run.ps1.
pause
endlocal
exit /b 1
