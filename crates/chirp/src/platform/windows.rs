//! Windows 10/11.
//!
//! * Desktop: `SystemParametersInfoW`, per user, no administrator rights.
//! * Lock screen, user scope: WinRT `LockScreen.SetImageFileAsync`.
//! * Lock screen, machine scope: the `PersonalizationCSP` registry values,
//!   which apply to every user and to the screen before sign-in. Needs
//!   administrator rights; used by the start-up task on school machines.
//! * Automatic updates: an entry in the user's `Run` key plus a daily task
//!   (user), or a start-up task running as SYSTEM plus a `Run` entry for all
//!   users (machine).

use super::InstallError;
use chirp_core::{Error, Platform, Result};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::process::Command;
use winreg::enums::*;
use winreg::RegKey;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "ChirpWallpaper";
const CSP_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\PersonalizationCSP";
const TASK_USER: &str = "Chirp Wallpaper";
const TASK_MACHINE: &str = "Chirp Wallpaper (System)";

pub struct NativePlatform {
    machine: bool,
}

impl NativePlatform {
    pub fn new(machine: bool) -> Self {
        Self { machine }
    }
}

impl Platform for NativePlatform {
    fn set_desktop(&self, image: &Path) -> Result<()> {
        if self.machine {
            // SYSTEM has no desktop; each user's sign-in sets it from the shared cache.
            return Ok(());
        }
        // Fill the screen and keep 4K pictures sharp (Windows re-compresses by default).
        if let Ok((desk, _)) =
            RegKey::predef(HKEY_CURRENT_USER).create_subkey(r"Control Panel\Desktop")
        {
            let _ = desk.set_value("WallpaperStyle", &"10");
            let _ = desk.set_value("TileWallpaper", &"0");
            let _ = desk.set_value("JPEGImportQuality", &100u32);
        }
        let mut wide: Vec<u16> = image.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::*;
            SystemParametersInfoW(
                SPI_SETDESKWALLPAPER,
                0,
                Some(wide.as_mut_ptr().cast()),
                SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
            )
        }
        .map_err(|e| Error::Platform(e.message().to_string()))
    }

    fn set_lockscreen(&self, image: &Path) -> Result<()> {
        if self.machine {
            return set_machine_lockscreen(image);
        }
        use windows::core::HSTRING;
        use windows::Storage::StorageFile;
        use windows::System::UserProfile::LockScreen;
        let path = HSTRING::from(image.as_os_str());
        let run = || -> windows::core::Result<()> {
            let file = StorageFile::GetFileFromPathAsync(&path)?.get()?;
            LockScreen::SetImageFileAsync(&file)?.get()
        };
        run().map_err(|e| Error::Platform(e.message().to_string()))
    }
}

fn set_machine_lockscreen(image: &Path) -> Result<()> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let (key, _) = hklm
        .create_subkey(CSP_KEY)
        .map_err(|e| Error::Platform(format!("PersonalizationCSP: {e}")))?;
    let path = image.to_string_lossy().to_string();
    key.set_value("LockScreenImagePath", &path)
        .and_then(|_| key.set_value("LockScreenImageUrl", &path))
        .and_then(|_| key.set_value("LockScreenImageStatus", &1u32))
        .map_err(|e| Error::Platform(format!("PersonalizationCSP: {e}")))
}

fn quoted_exe() -> std::result::Result<String, InstallError> {
    Ok(format!("\"{}\"", std::env::current_exe()?.display()))
}

pub fn install(machine: bool) -> std::result::Result<(), InstallError> {
    let exe = quoted_exe()?;
    if machine {
        // Every user: set the desktop at sign-in from the shared cache.
        let (run, _) = RegKey::predef(HKEY_LOCAL_MACHINE).create_subkey(RUN_KEY)?;
        run.set_value(RUN_VALUE, &format!("{exe} apply --quiet"))?;
        // At start-up as SYSTEM: download and set the lock screen for everyone.
        schtasks(&[
            "/Create",
            "/F",
            "/TN",
            TASK_MACHINE,
            "/SC",
            "ONSTART",
            "/DELAY",
            "0000:30",
            "/RU",
            "SYSTEM",
            "/RL",
            "HIGHEST",
            "/TR",
            &format!("{exe} apply --machine --quiet"),
        ])?;
        // And again every hour, in case the machine was offline at start-up.
        schtasks(&[
            "/Create",
            "/F",
            "/TN",
            &format!("{TASK_MACHINE} hourly"),
            "/SC",
            "HOURLY",
            "/RU",
            "SYSTEM",
            "/TR",
            &format!("{exe} apply --machine --quiet"),
        ])?;
    } else {
        let (run, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY)?;
        run.set_value(RUN_VALUE, &format!("{exe} apply --quiet"))?;
        schtasks(&[
            "/Create",
            "/F",
            "/TN",
            TASK_USER,
            "/SC",
            "HOURLY",
            "/TR",
            &format!("{exe} apply --quiet"),
        ])?;
    }
    Ok(())
}

pub fn uninstall(machine: bool) -> std::result::Result<(), InstallError> {
    if machine {
        let run = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(RUN_KEY, KEY_WRITE)?;
        let _ = run.delete_value(RUN_VALUE);
        let _ = schtasks(&["/Delete", "/F", "/TN", TASK_MACHINE]);
        let _ = schtasks(&["/Delete", "/F", "/TN", &format!("{TASK_MACHINE} hourly")]);
    } else {
        if let Ok(run) =
            RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_WRITE)
        {
            let _ = run.delete_value(RUN_VALUE);
        }
        let _ = schtasks(&["/Delete", "/F", "/TN", TASK_USER]);
    }
    Ok(())
}

fn schtasks(args: &[&str]) -> std::result::Result<(), InstallError> {
    let out = Command::new("schtasks.exe")
        .args(args)
        .output()
        .map_err(|e| InstallError::Failed(format!("schtasks: {e}")))?;
    if out.status.success() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&out.stderr).to_string();
    if text.contains("0x80070005") || text.to_lowercase().contains("access") {
        Err(InstallError::NeedsAdmin)
    } else {
        Err(InstallError::Failed(text.trim().to_string()))
    }
}
