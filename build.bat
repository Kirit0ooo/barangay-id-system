@echo off
REM Barangay ID System — portable release build. Thin wrapper around canonical build.ps1.
REM All artifacts stay inside this folder: src-tauri\target\x86_64-pc-windows-msvc\release\bundle\ + logs\build-*.log
REM Build includes get_cards_batch (bulk print) + optimize_database (E2). See build.ps1 .DESCRIPTION.
setlocal
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

where powershell >nul 2>nul
if errorlevel 1 goto :NOPS

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0build.ps1" %*
exit /b %errorlevel%

:NOPS
REM Minimal fallback: ps1 is source of truth - do not duplicate build logic here.
echo [Barangay ID] PowerShell not found - install PowerShell 5.1+ https://aka.ms/wmf5download
echo [Barangay ID] Then re-run build.bat. Flags (-Help/-Audit) need build.ps1.
pause
endlocal
exit /b 1
