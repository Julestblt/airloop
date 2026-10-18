; AirPlay receivers connect back to the sender (NTP timing requests and
; retransmit requests arrive as inbound UDP), so Airloop needs an inbound
; firewall rule (all network profiles: home networks are often marked Public).
; Removed on uninstall.
!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="Airloop"'
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="Airloop" dir=in action=allow program="$INSTDIR\airloop.exe" enable=yes profile=any'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="Airloop"'
!macroend
