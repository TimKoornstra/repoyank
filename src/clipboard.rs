use anyhow::Result;
use arboard::Clipboard;
#[cfg(target_os = "linux")]
use arboard::SetExtLinux;
#[cfg(target_os = "linux")]

pub const DAEMON_FLAG: &str = "__clipboard_daemon";

#[cfg(target_os = "linux")]
fn run_daemon_mode() -> Result<()> {
    let text = std::io::read_to_string(std::io::stdin())?;

    let mut clipboard = Clipboard::new()?;
    let result = clipboard.set().wait().text(text); // Keep the waiter alive

    match result {
        Ok(_waiter) => {
            // The _waiter needs to be kept alive.
            // By returning it, the caller (check_and_run_daemon_if_requested) would need to hold it.
            // Or, we park the thread here.
            std::thread::park(); // Keep the process alive so the clipboard stays valid
            unreachable!("Daemon should park indefinitely");
        }
        Err(e) => Err(anyhow::Error::from(e)),
    }
}

/// Checks if the DAEMON_FLAG is present in args. If so, runs in daemon mode and exits.
/// Returns Ok(true) if daemon mode was run (and exited), Ok(false) otherwise.
pub fn check_and_run_daemon_if_requested() -> Result<bool> {
    if std::env::args().any(|a| a == DAEMON_FLAG) {
        #[cfg(target_os = "linux")]
        {
            run_daemon_mode()?;
            return Ok(true);
        }
        #[cfg(not(target_os = "linux"))]
        {
            // Daemon flag on non-Linux is unexpected, could be an error or no-op
            eprintln!(
                "Warning: {} flag used on non-Linux system. Ignoring.",
                DAEMON_FLAG
            );
            std::process::exit(0);
        }
    }
    Ok(false)
}

pub fn copy_text_to_clipboard(text: String) -> Result<()> {
    #[cfg(not(target_os = "linux"))]
    {
        let mut clipboard = Clipboard::new()?;
        clipboard.set_text(text)?;
    }

    #[cfg(target_os = "linux")]
    {
        use std::io::Write;
        use std::process::{Command, Stdio};

        // Check if running inside a Flatpak sandbox where direct X11/Wayland access might be restricted
        // or if a portal is preferred. `arboard` tries to handle this, but for the daemon approach,
        // we are manually forking.

        let mut child = Command::new(std::env::current_exe()?)
            .arg(DAEMON_FLAG)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .current_dir("/")
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
            stdin.flush()?;
        } else {
            return Err(anyhow::anyhow!("Failed to get stdin for clipboard daemon"));
        }
    }
    Ok(())
}
