//! Platform backends (Windows, GNOME) and translations shared by the
//! command line program and the app.

pub mod i18n;
pub mod platform;

pub use i18n::Localizer;
pub use platform::{InstallError, NativePlatform};
