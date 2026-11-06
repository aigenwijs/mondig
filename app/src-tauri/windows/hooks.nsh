; NSIS-hooks voor de Mondig-installer (ADR 0012).
;
; Bij een upgrade kan het werkproces mondig-engine.exe nog draaien. Het bestand zit dan
; op slot en de installatie laat stilletjes een oude versie naast de nieuwe app staan.
; Daarom stopt de installer het werkproces eerst, en breekt hij af als dat niet lukt.
;
; Volgorde: eerst de gewone Tauri-controle of Mondig zelf nog draait (met de vraag aan
; de gebruiker om Mondig te sluiten), pas daarna het werkproces stoppen. Kiest de
; gebruiker Annuleren, dan blijven app en werkproces dus samen draaien. De template
; voert dezelfde controle na de hook nog een keer uit; dan draait Mondig niet meer en
; gebeurt er niets. In stille en passieve modus stelt de controle geen vraag maar sluit
; hij Mondig direct (macro CheckIfAppIsRunning in utils.nsh van Tauri). Twee keer
; invoegen in dezelfde sectie geeft geen dubbele labels: UniqueID is ${__LINE__}, en
; dat is per aanroepplek anders (gecontroleerd met makensis -V4, 2-11-2026).
;
; Patroon: macro StopSidecarBeforeInstall in desktop/src-tauri/vc_redist.nsh van Vibe
; (MIT, https://github.com/thewh1teagle/vibe).

!macro StopWerkprocesVoorInstallatie WERKPROCES_NAAM
    ${If} ${FileExists} "$INSTDIR\${WERKPROCES_NAAM}"
        ; taskkill zit in Windows en werkt ook als PowerShell-scripts uitstaan.
        ; Alleen bij een upgrade (het bestand bestaat al), zodat een verse installatie
        ; nooit een ander proces met dezelfde naam stopt.
        nsExec::ExecToStack '"$SYSDIR\taskkill.exe" /F /T /IM "${WERKPROCES_NAAM}"'
        Pop $0
        Pop $1
        DetailPrint "taskkill ${WERKPROCES_NAAM}: $0 $1"
        Sleep 500

        ; Het oude bestand verwijderen bewijst dat het slot weg is. Anders afbreken in
        ; plaats van een oud, niet-passend werkproces te laten staan.
        Delete "$INSTDIR\${WERKPROCES_NAAM}"
        ${If} ${FileExists} "$INSTDIR\${WERKPROCES_NAAM}"
            MessageBox MB_OK|MB_ICONSTOP "Mondig kan ${WERKPROCES_NAAM} niet vervangen omdat het nog in gebruik is. Sluit Mondig af (ook via het pictogram bij de klok), start daarna het installatieprogramma opnieuw." /SD IDOK
            Abort "Het bestaande ${WERKPROCES_NAAM} is nog in gebruik"
        ${EndIf}
    ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
    ; Een bestand dat niet te overschrijven is, mag de gebruiker niet meer overslaan:
    ; standaard biedt NSIS dan "Negeren" aan en blijft er stilletjes een oud bestand staan.
    ; Met deze vlag kan de gebruiker alleen opnieuw proberen of afbreken (NSIS-handleiding,
    ; AllowSkipFiles, geraadpleegd 1-11-2026).
    AllowSkipFiles off
    !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
    !insertmacro StopWerkprocesVoorInstallatie "mondig-engine.exe"
!macroend

; Bij verwijderen kan het werkproces ook nog draaien; stop het zodat de map leeg kan.
; Ook hier eerst de controle op de app zelf, zodat Annuleren niets half afbreekt.
!macro NSIS_HOOK_PREUNINSTALL
    !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
    nsExec::ExecToStack '"$SYSDIR\taskkill.exe" /F /T /IM "mondig-engine.exe"'
    Pop $0
    Pop $1
    Sleep 500
!macroend
