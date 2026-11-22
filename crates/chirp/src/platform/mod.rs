//! Setting desktop and lock screen, and registering automatic updates.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(not(windows))]
mod linux;
#[cfg(not(windows))]
pub use self::linux::*;

/// Errors from installing or removing automatic updates.
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
