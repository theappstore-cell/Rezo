!macro NSIS_HOOK_POSTINSTALL
  nsExec::Exec 'netsh advfirewall firewall delete rule name="Rezo"'
  nsExec::Exec 'netsh advfirewall firewall add rule name="Rezo" dir=in action=allow program="$INSTDIR\${MAINBINARYNAME}.exe" enable=yes profile=private'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec 'netsh advfirewall firewall delete rule name="Rezo"'
!macroend
