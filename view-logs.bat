@echo off
REM Barangay ID System — thin double-click log viewer -> run.ps1 -Logs + Explorer open.
REM Tail logic lives in run.ps1 -Logs (source of truth); this wrapper only adds `explorer logs`.
REM Tails app.log.* + frontend.log + latest run/build transcripts in isolated logs\ folder.
setlocal
cd /d "%~dp0"

REM ---- Portable env pins (same as run.ps1/run.bat; read-only use) ----
set "NPM_CONFIG_CACHE=%CD%\.npm-cache"
set "CARGO_TARGET_DIR=%CD%\src-tauri\target"
set "TMP=%CD%\tmp"
set "TEMP=%CD%\tmp"
set "TMPDIR=%CD%\tmp"
set "WEBVIEW2_USER_DATA_FOLDER=%CD%\tmp\webview2"
if not exist "logs" mkdir "logs" 2>nul

where powershell >nul 2>nul
if errorlevel 1 goto :NOPS

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0run.ps1" -Logs
set "PSERR=%errorlevel%"
explorer "logs" 2>nul
pause
endlocal & exit /b %PSERR%

:NOPS
echo [Barangay ID] PowerShell not found - install PowerShell 5.1+ https://aka.ms/wmf5download
if exist "logs" explorer "logs" 2>nul
pause
endlocal
exit /b 1
