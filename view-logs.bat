@echo off
REM =============================================================================
REM Barangay ID System - log viewer (thin wrapper -> run.ps1 -Logs + Explorer open).
REM Source of truth: run.ps1 -Logs (source of truth); this wrapper adds explorer logs.
REM =============================================================================
REM Portable contract: all artifacts stay inside this folder.
REM   NPM_CONFIG_CACHE=.npm-cache   CARGO_TARGET_DIR=src-tauri\target
REM   TMP/TEMP/TMPDIR=tmp           WEBVIEW2_USER_DATA_FOLDER=tmp\webview2
REM =============================================================================
chcp 65001 >nul
setlocal EnableExtensions EnableDelayedExpansion
cd /d "%~dp0"

REM ---- Portable env pins (same as run.ps1/run.bat; read-only use) ----
set "NPM_CONFIG_CACHE=%CD%\.npm-cache"
set "CARGO_TARGET_DIR=%CD%\src-tauri\target"
set "TMP=%CD%\tmp"
set "TEMP=%CD%\tmp"
set "TMPDIR=%CD%\tmp"
set "WEBVIEW2_USER_DATA_FOLDER=%CD%\tmp\webview2"
if not exist "logs" mkdir "logs" 2>nul

REM ---- Detect PowerShell 7+ (pwsh) before legacy powershell ----
where pwsh >nul 2>&1
if not errorlevel 1 (
    set "PS=pwsh"
) else (
    where powershell >nul 2>&1
    if errorlevel 1 goto :NOPS
    set "PS=powershell"
)

"%PS%" -NoProfile -ExecutionPolicy Bypass -File "%~dp0run.ps1" -Logs
set "PSERR=%errorlevel%"
REM Only open Explorer if PowerShell succeeded (avoids opening empty logs on failure).
if %PSERR% equ 0 explorer "logs" 2>nul
pause
endlocal & exit /b %PSERR%

:NOPS
echo [Barangay ID] PowerShell not found - install PowerShell 5.1+ https://aka.ms/wmf5download
echo [Barangay ID] Then re-run view-logs.bat (or run.ps1 -Logs).
if exist "logs" explorer "logs" 2>nul
pause
endlocal
exit /b 1
