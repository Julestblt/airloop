; AirPlay receivers connect back to the sender (NTP timing requests and
; retransmit requests arrive as inbound UDP), so Airloop needs an inbound
; firewall rule. Added for private/domain networks only; removed on uninstall.
!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="Airloop"'
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="Airloop" dir=in action=allow program="$INSTDIR\airloop.exe" enable=yes profile=private,domain'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="Airloop"'
!macroend
