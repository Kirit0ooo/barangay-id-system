@echo off
REM =============================================================================
REM Barangay ID System - portable release build launcher (thin wrapper around build.ps1).
REM Source of truth: build.ps1 (canonical). This batch only sets env pins + forwards.
REM Flags forwarded: -Help | -Audit | -NoBump | -NoClean | -EnableVerboseLogging
REM =============================================================================
REM Portable contract: all artifacts stay inside this folder (no %APPDATA%/%TEMP% writes).
REM   NPM_CONFIG_CACHE=.npm-cache   CARGO_TARGET_DIR=src-tauri\target
REM   TMP/TEMP/TMPDIR=tmp           WEBVIEW2_USER_DATA_FOLDER=tmp\webview2
REM =============================================================================
chcp 65001 >nul
setlocal EnableExtensions EnableDelayedExpansion
cd /d "%~dp0"

REM ---- Portable env pins (mirrors build.ps1) ----
set "NPM_CONFIG_CACHE=%CD%\.npm-cache"
set "CARGO_TARGET_DIR=%CD%\src-tauri\target"
set "TMP=%CD%\tmp"
set "TEMP=%CD%\tmp"
set "TMPDIR=%CD%\tmp"
set "WEBVIEW2_USER_DATA_FOLDER=%CD%\tmp\webview2"
if not exist "logs" mkdir "logs" 2>nul
if not exist "tmp\webview2" mkdir "tmp\webview2" 2>nul
if not exist ".npm-cache" mkdir ".npm-cache" 2>nul

REM ---- Detect PowerShell 7+ (pwsh) before legacy powershell ----
where pwsh >nul 2>&1
if not errorlevel 1 (
    set "PS=pwsh"
) else (
    where powershell >nul 2>&1
    if errorlevel 1 goto :NOPS
    set "PS=powershell"
)

REM Early -Help passthrough
if "%~1"=="-Help" (
    "%PS%" -NoProfile -ExecutionPolicy Bypass -File "%~dp0build.ps1" -Help
    exit /b %errorlevel%
)

"%PS%" -NoProfile -ExecutionPolicy Bypass -File "%~dp0build.ps1" %*
exit /b %errorlevel%

:NOPS
REM Minimal fallback: ps1 is source of truth - do not duplicate build logic here.
echo [Barangay ID] PowerShell not found - install PowerShell 5.1+ https://aka.ms/wmf5download
echo [Barangay ID] Then re-run build.bat. Flags (-Help/-Audit/-NoBump/-NoClean/-EnableVerboseLogging) need build.ps1.
pause
endlocal
exit /b 1
