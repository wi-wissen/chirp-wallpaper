//! Setting desktop and lock screen, autostart and scheduled updates.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(not(windows))]
mod linux;
#[cfg(not(windows))]
pub use self::linux::*;

/// Errors from registering or removing autostart and tasks.
#[derive(Debug)]
pub enum InstallError {
    NeedsAdmin,
    #[cfg_attr(windows, allow(dead_code))]
    NotSupported,
    Failed(String),
}

impl From<std::io::Error> for InstallError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            InstallError::NeedsAdmin
        } else {
            InstallError::Failed(e.to_string())
        }
    }
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::NeedsAdmin => f.write_str("administrator rights needed"),
            InstallError::NotSupported => f.write_str("not supported on this system"),
            InstallError::Failed(s) => f.write_str(s),
        }
    }
}

/// A program and its arguments, quoted for the system's command line.
pub fn command_line(exe: &std::path::Path, args: &str) -> String {
    if args.is_empty() {
        format!("\"{}\"", exe.display())
    } else {
        format!("\"{}\" {args}", exe.display())
    }
}

/// Command line program: autostart plus a periodic task.
pub fn install(machine: bool) -> Result<(), InstallError> {
    let exe = std::env::current_exe()?;
    if machine {
        set_autostart(true, Some(&command_line(&exe, "apply --quiet")))?;
        register_machine_tasks(&command_line(&exe, "apply --machine --quiet"))
    } else {
        set_autostart(false, Some(&command_line(&exe, "apply --quiet")))?;
        register_user_task(&command_line(&exe, "apply --quiet"))
    }
}

/// Undo [`install`].
pub fn uninstall(machine: bool) -> Result<(), InstallError> {
    set_autostart(machine, None)?;
    if machine {
        remove_machine_tasks()
    } else {
        remove_user_task()
    }
}
