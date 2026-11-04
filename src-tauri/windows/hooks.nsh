; Hooks NSIS Tauri : enregistre / retire l'entrée du menu contextuel Windows 11.
!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\register-menu.ps1" -Dir "$INSTDIR"'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -Command "Get-AppxPackage -Name DCleaner | Remove-AppxPackage"'
!macroend
