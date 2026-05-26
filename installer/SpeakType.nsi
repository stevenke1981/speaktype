Unicode true
Name "SpeakType"
OutFile "${PROJECT_ROOT}\dist\packages\SpeakType-Setup.exe"
InstallDir "$PROGRAMFILES64\SpeakType"
RequestExecutionLevel admin

!define APP_EXE "speaktype.exe"
!define RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"

Page components
Page directory
Page instfiles

UninstPage uninstConfirm
UninstPage instfiles

Section "SpeakType" SEC_APP
  SectionIn RO
  SetOutPath "$INSTDIR"
  File "${PROJECT_ROOT}\dist\release\speaktype.exe"
  File "${PROJECT_ROOT}\PACKAGING.md"

  CreateDirectory "$SMPROGRAMS\SpeakType"
  CreateShortcut "$SMPROGRAMS\SpeakType\SpeakType.lnk" "$INSTDIR\${APP_EXE}"
  CreateShortcut "$DESKTOP\SpeakType.lnk" "$INSTDIR\${APP_EXE}"

  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\SpeakType" "DisplayName" "SpeakType"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\SpeakType" "UninstallString" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\SpeakType" "InstallLocation" "$INSTDIR"
SectionEnd

Section "Start with Windows in tray" SEC_STARTUP
  WriteRegStr HKCU "${RUN_KEY}" "SpeakType" '"$INSTDIR\${APP_EXE}" --tray'
SectionEnd

Section "Uninstall"
  DeleteRegValue HKCU "${RUN_KEY}" "SpeakType"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\SpeakType"

  Delete "$DESKTOP\SpeakType.lnk"
  Delete "$SMPROGRAMS\SpeakType\SpeakType.lnk"
  RMDir "$SMPROGRAMS\SpeakType"

  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\PACKAGING.md"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
