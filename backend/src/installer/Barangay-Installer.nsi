; ------------------------------------------------------------
; Barangay ID System - Custom NSIS Installer
; with WebView2 Runtime detection and conditional installation
; ------------------------------------------------------------

!include "MUI2.nsh"
!include "x64.nsh"

!define APP_NAME "Barangay ID System"
!define APP_PRODUCT_NAME "Barangay-ID-Registration-System"
!define APP_EXE "Barangay-ID-Registration-System.exe"
!ifndef APP_VERSION
!define APP_VERSION "1.0.22"
!endif
!define PACKAGE_NAME "com.barangay.idsystem"
!define INSTALLER_ICON "../../../barangay.ico"

RequestExecutionLevel admin
InstallDir "$PROGRAMFILES64\${APP_NAME}"
OutFile "..\target\x86_64-pc-windows-msvc\release\bundle\nsis\${APP_NAME} Setup ${APP_VERSION}.exe"

; WebView2 Runtime constants
!define WEBVIEW2_GUID "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
!define WEBVIEW2_REG_KEY "SOFTWARE\Microsoft\EdgeUpdate\Clients\${WEBVIEW2_GUID}"
!define WEBVIEW2_REG_KEY_WOW64 "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\${WEBVIEW2_GUID}"
!define WEBVIEW2_BOOTSTRAPPER_URL "https://go.microsoft.com/fwlink/p/?linkid=2124703"
!define WEBVIEW2_INSTALLER "$TEMP\WebView2Installer.exe"

; Runtime state variables
Var IsWebView2Installed
Var HasInternet

; ------------------------------------------------------------
; Interface
; ------------------------------------------------------------
!define MUI_ABORTWARNING
!define MUI_ICON "${INSTALLER_ICON}"
!define MUI_UNICON "${INSTALLER_ICON}"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "../../../LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_WELCOME
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

!insertmacro MUI_LANGUAGE "English"

; ------------------------------------------------------------
; Sections
; ------------------------------------------------------------

Section "WebView2 Runtime" WebView2Sec
    DetailPrint "Checking WebView2 Runtime presence..."

    ${If} $IsWebView2Installed == true
        DetailPrint "WebView2 Runtime already installed - skipping WebView2 installation."
    ${Else}
        ${If} $HasInternet == true
            DetailPrint "WebView2 Runtime not found. Internet available - downloading WebView2 bootstrapper..."

            ; Download WebView2 bootstrapper using PowerShell
            FileOpen $R1 "$TEMP\Download-WebView2.ps1" w
            FileWrite $R1 "$$client = New-Object System.Net.WebClient$\r$\n$$client.DownloadFile('${WEBVIEW2_BOOTSTRAPPER_URL}', '${WEBVIEW2_INSTALLER}')$\r$\n"
            FileClose $R1

            ExecWait 'powershell -NoProfile -ExecutionPolicy Bypass -File "$TEMP\Download-WebView2.ps1"' $R0
            ${If} $R0 != 0
                MessageBox MB_OK|MB_ICONEXCLAMATION "Failed to download WebView2 Runtime installer. Please check your internet connection and try again." IDOK AbortInstall
            ${EndIf}

            DetailPrint "Installing WebView2 Runtime..."
            ExecWait '"${WEBVIEW2_INSTALLER}" /silent /install' $R0
            ${If} $R0 != 0
                MessageBox MB_OK|MB_ICONEXCLAMATION "WebView2 Runtime installation failed (exit code $R0). The application requires WebView2 to run." IDOK AbortInstall
            ${EndIf}

            Delete "${WEBVIEW2_INSTALLER}"
        ${Else}
            ; No internet - prompt user
            MessageBox MB_YESNO|MB_ICONEXCLAMATION "WebView2 Runtime is not installed and no internet connection was detected.$\r$\n$\r$\nBarangay ID System requires WebView2 to render its user interface.$\r$\nWithout WebView2, the application may not start or function correctly.$\r$\n$\r$\nDo you want to continue installation without WebView2?" IDYES ContinueWithoutWebView IDNO AbortInstall

ContinueWithoutWebView:
            DetailPrint "User chose to continue without WebView2 Runtime."
            Goto WebView2SecEnd
        ${EndIf}
    ${EndIf}

AbortInstall:
    SetErrorLevel 730

WebView2SecEnd:
SectionEnd

Section "Application" AppSec
    SetOutPath "$INSTDIR"
    File "..\target\x86_64-pc-windows-msvc\release\${APP_EXE}"

    ; Create shortcuts
    CreateShortCut "$SMPROGRAMS\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}" 0
    CreateShortCut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}" 0

    ; Write uninstaller
    WriteUninstaller "$INSTDIR\Uninstall.exe"

    ; Register uninstaller
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PACKAGE_NAME}" "DisplayName" "${APP_NAME}"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PACKAGE_NAME}" "UninstallString" "$INSTDIR\Uninstall.exe"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PACKAGE_NAME}" "DisplayVersion" "${APP_VERSION}"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PACKAGE_NAME}" "Publisher" "Barangay ID System"
SectionEnd

Section -Post
    ; Clean up temporary files
    Delete "$TEMP\Download-WebView2.ps1"
    Delete "${WEBVIEW2_INSTALLER}"
SectionEnd

; ------------------------------------------------------------
; Uninstall
; ------------------------------------------------------------
Section "Uninstall"
    Delete "$INSTDIR\${APP_EXE}"
    Delete "$INSTDIR\Uninstall.exe"
    Delete "$SMPROGRAMS\${APP_NAME}.lnk"
    Delete "$DESKTOP\${APP_NAME}.lnk"
    RMDir "$INSTDIR"
    DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PACKAGE_NAME}"
SectionEnd

; ------------------------------------------------------------
; Functions
; ------------------------------------------------------------

Function .onInit
    ; Initialise state variables
    StrCpy $IsWebView2Installed false
    StrCpy $HasInternet false

    ; --- WebView2 detection ---
    ; Check for WebView2Loader.dll in system directory
    IfFileExists "$SYSDIR\WebView2Loader.dll" FoundSysDll CheckReg
FoundSysDll:
    StrCpy $IsWebView2Installed true
    Goto DoneWebView2
CheckReg:
    ; Check 64-bit registry (NSIS is 32-bit, switch to 64-bit view)
    SetRegView 64
    ReadRegStr $R0 HKLM "${WEBVIEW2_REG_KEY}" "pv"
    SetRegView 32
    ${If} $R0 != ""
        StrCpy $IsWebView2Installed true
    ${Else}
        ; Fall back to 32-bit registry view on 64-bit systems
        ReadRegStr $R0 HKLM "${WEBVIEW2_REG_KEY_WOW64}" "pv"
        ${If} $R0 != ""
            StrCpy $IsWebView2Installed true
        ${Else}
            StrCpy $IsWebView2Installed false
        ${EndIf}
    ${EndIf}
DoneWebView2:

    ; --- Internet connectivity check ---
    ; Ping a reliable Microsoft endpoint with short timeout
    ExecWait 'ping -n 1 -w 3000 www.microsoft.com >nul 2>&1' $R0
    ${If} $R0 == 0
        StrCpy $HasInternet true
    ${Else}
        ; Ping failed - try DNS resolution as fallback
        ExecWait 'nslookup www.microsoft.com >nul 2>&1' $R0
        ${If} $R0 == 0
            StrCpy $HasInternet true
        ${Else}
            StrCpy $HasInternet false
        ${EndIf}
    ${EndIf}
FunctionEnd
