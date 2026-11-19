//! Desktop-only console detachment. Headless routes retain their console.

use windows_sys::Win32::System::Console::FreeConsole;

// qual:allow(unsafe) reason: "FreeConsole has no pointer arguments and detaches only this process."
pub(crate) fn detach() -> Result<(), String> {
    // SAFETY: FreeConsole has no caller-owned resources or pointer arguments.
    if unsafe { FreeConsole() } == 0 {
        return Err(format!(
            "could not detach the desktop console: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    use windows_sys::Win32::System::{
        Console::GetConsoleProcessList, Threading::CREATE_NEW_CONSOLE,
    };

    // qual:allow(unsafe) reason: "The console API writes to one valid process-ID slot."
    fn attached_processes() -> u32 {
        let mut process = 0;
        // SAFETY: the pointer names one writable u32 for the duration of the call.
        unsafe { GetConsoleProcessList(&mut process, 1) }
    }

    #[test]
    fn desktop_detachment_keeps_the_process_alive_without_a_console() {
        const CHILD: &str = "SUBSPACE_TEST_DESKTOP_CONSOLE_CHILD";
        if std::env::var_os(CHILD).is_some() {
            assert!(
                attached_processes() > 0,
                "the child starts with its own console"
            );
            super::detach().unwrap();
            assert_eq!(
                attached_processes(),
                0,
                "the desktop no longer owns a console"
            );
            assert!(super::detach().is_ok(), "detaching again is harmless");
            return;
        }
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "console::tests::desktop_detachment_keeps_the_process_alive_without_a_console",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .creation_flags(CREATE_NEW_CONSOLE)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "the detached child exits normally: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("running 1 test"),
            "the isolated child ran the detachment regression"
        );
    }
}
