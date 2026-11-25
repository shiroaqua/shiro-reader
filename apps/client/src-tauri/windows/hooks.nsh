!macro NSIS_HOOK_POSTINSTALL
  ${If} $LANGUAGE = ${LANG_SIMPCHINESE}
    WriteRegStr SHCTX "${UNINSTKEY}" "DisplayName" "白读"

    !if "${STARTMENUFOLDER}" != ""
      ${If} ${FileExists} "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
        Delete "$SMPROGRAMS\$AppStartMenuFolder\白读.lnk"
        Rename "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$SMPROGRAMS\$AppStartMenuFolder\白读.lnk"
        !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\$AppStartMenuFolder\白读.lnk"
      ${EndIf}
    !else
      ${If} ${FileExists} "$SMPROGRAMS\${PRODUCTNAME}.lnk"
        Delete "$SMPROGRAMS\白读.lnk"
        Rename "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$SMPROGRAMS\白读.lnk"
        !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\白读.lnk"
      ${EndIf}
    !endif

    ${If} ${FileExists} "$DESKTOP\${PRODUCTNAME}.lnk"
      Delete "$DESKTOP\白读.lnk"
      Rename "$DESKTOP\${PRODUCTNAME}.lnk" "$DESKTOP\白读.lnk"
      !insertmacro SetLnkAppUserModelId "$DESKTOP\白读.lnk"
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    !insertmacro MUI_STARTMENU_GETFOLDER Application $AppStartMenuFolder
    !insertmacro IsShortcutTarget "$SMPROGRAMS\$AppStartMenuFolder\白读.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      !insertmacro UnpinShortcut "$SMPROGRAMS\$AppStartMenuFolder\白读.lnk"
      Delete "$SMPROGRAMS\$AppStartMenuFolder\白读.lnk"
      RMDir "$SMPROGRAMS\$AppStartMenuFolder"
    ${EndIf}
    !insertmacro IsShortcutTarget "$SMPROGRAMS\白读.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      !insertmacro UnpinShortcut "$SMPROGRAMS\白读.lnk"
      Delete "$SMPROGRAMS\白读.lnk"
    ${EndIf}
    !insertmacro IsShortcutTarget "$DESKTOP\白读.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      !insertmacro UnpinShortcut "$DESKTOP\白读.lnk"
      Delete "$DESKTOP\白读.lnk"
    ${EndIf}
  ${EndIf}
!macroend
