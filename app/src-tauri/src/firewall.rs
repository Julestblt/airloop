//! Windows Firewall repair.
//!
//! AirPlay receivers send timing (NTP) and retransmit requests back to the
//! sender as inbound UDP. If Windows Firewall blocks Airloop — no rule on a
//! Public network, or a block rule created when someone clicked "Cancel" on
//! the first-run firewall prompt — the speaker never finishes setting up the
//! stream. This removes Airloop's block rules and adds an allow rule, through
//! an elevated PowerShell that Windows asks the user to approve (UAC).

#[cfg(windows)]
pub fn allow_airloop() -> Result<(), String> {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // Single quotes inside a PowerShell string are escaped by doubling them.
    let exe = exe.display().to_string().replace('\'', "''");
    let script = format!(
        "$p='{exe}'; \
         Get-NetFirewallApplicationFilter -Program $p -ErrorAction SilentlyContinue | \
           Get-NetFirewallRule | Where-Object Action -eq 'Block' | Remove-NetFirewallRule; \
         Get-NetFirewallRule -DisplayName 'Airloop' -ErrorAction SilentlyContinue | Remove-NetFirewallRule; \
         New-NetFirewallRule -DisplayName 'Airloop' -Description 'AirPlay timing and retransmit requests from speakers' \
           -Direction Inbound -Program $p -Action Allow -Profile Any | Out-Null"
    );
    let params = HSTRING::from(format!(
        "-NoProfile -NonInteractive -ExecutionPolicy Bypass -Command \"{script}\""
    ));

    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: w!("powershell.exe"),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    // SAFETY: `info` is fully initialised; the strings outlive the call.
    unsafe { ShellExecuteExW(&mut info) }.map_err(|e| {
        format!(
            "Windows did not run the firewall update ({e}). Was the administrator prompt declined?"
        )
    })?;
    if info.hProcess.is_invalid() {
        return Err("Windows did not start the firewall update.".into());
    }
    // SAFETY: valid process handle from ShellExecuteExW, closed below.
    unsafe {
        let waited = WaitForSingleObject(info.hProcess, 60_000);
        let mut code = 1u32;
        let _ = GetExitCodeProcess(info.hProcess, &mut code);
        let _ = CloseHandle(info.hProcess);
        if waited != WAIT_OBJECT_0 {
            return Err("The firewall update did not finish in time.".into());
        }
        if code != 0 {
            return Err(format!("The firewall update failed (exit code {code})."));
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn allow_airloop() -> Result<(), String> {
    Err("Firewall repair is only available on Windows.".into())
}
