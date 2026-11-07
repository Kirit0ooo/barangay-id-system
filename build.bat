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
REM Single exit path: every branch sets RC and goes to :DONE, so the console code
REM page this script changes is always put back and the exit code is always real.
REM Both hard stops pause: they are only reachable when the build cannot start at
REM all, and a double-clicked window would otherwise close before the reason shows.
REM No delayed expansion on purpose - a "!" anywhere in the path or the forwarded
REM arguments would otherwise be eaten silently.
REM =============================================================================
setlocal EnableExtensions
cd /d "%~dp0"
set "RC=0"

REM ---- Remember the console code page so :DONE can put it back ----
for /f "tokens=2 delims=:" %%C in ('chcp') do set "OLDCP=%%C"
set "OLDCP=%OLDCP: =%"

REM ---- UTF-8 console for the build output ----
chcp 65001 >nul

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

REM ---- build.ps1 holds the whole build; refuse to run without it ----
if not exist "%~dp0build.ps1" (
    echo [Barangay ID] build.ps1 is missing from "%~dp0"
    echo [Barangay ID] That file holds the entire build - restore it, then re-run build.bat.
    set "RC=1"
    pause
    goto :DONE
)

REM ---- Detect PowerShell 7+ (pwsh) before legacy powershell ----
set "PS="
where pwsh >nul 2>&1
if not errorlevel 1 (
    set "PS=pwsh"
) else (
    where powershell >nul 2>&1
    if not errorlevel 1 set "PS=powershell"
)
if not defined PS (
    echo [Barangay ID] PowerShell not found - install PowerShell 5.1+ https://aka.ms/wmf5download
    REM No parentheses in this echo: a literal ")" inside an if-block closes it early.
    echo [Barangay ID] Then re-run build.bat. Flags: -Help -Audit -NoBump -NoClean -EnableVerboseLogging, all handled by build.ps1.
    set "RC=1"
    pause
    goto :DONE
)

"%PS%" -NoProfile -ExecutionPolicy Bypass -File "%~dp0build.ps1" %*
set "RC=%errorlevel%"

:DONE
REM ---- Put the console code page back (anything but a plain number is skipped) ----
if not defined OLDCP goto :SKIPCP
echo %OLDCP%| findstr /r /c:"^[0-9][0-9]*$" >nul 2>&1
if errorlevel 1 goto :SKIPCP
chcp %OLDCP% >nul

:SKIPCP
endlocal & exit /b %RC%
