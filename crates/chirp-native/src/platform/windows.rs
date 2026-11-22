//! Windows 10/11.
//!
//! * Desktop: `SystemParametersInfoW`, per user, no administrator rights.
//! * Lock screen, user scope: WinRT `LockScreen.SetImageFileAsync`.
//! * Lock screen, machine scope: the `PersonalizationCSP` registry values,
//!   which apply to every user and to the screen before sign-in. Needs
//!   administrator rights; used by the start-up task on school machines.
//! * Autostart: the `Run` key of the user (or of the machine, for everyone).
//! * Tasks: Task Scheduler, via `schtasks.exe`.

use super::InstallError;
use chirp_core::{Error, Platform, Result};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use winreg::enums::*;
use winreg::RegKey;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "ChirpWallpaper";
const CSP_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\PersonalizationCSP";
const TASK_USER: &str = "Chirp Wallpaper";
const TASK_MACHINE: &str = "Chirp Wallpaper (System)";
const TASK_MACHINE_HOURLY: &str = "Chirp Wallpaper (System, hourly)";

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

fn run_root(machine: bool) -> RegKey {
    RegKey::predef(if machine {
        HKEY_LOCAL_MACHINE
    } else {
        HKEY_CURRENT_USER
    })
}

/// Start at sign-in (`Some(command)`) or not (`None`).
pub fn set_autostart(
    machine: bool,
    command: Option<&str>,
) -> std::result::Result<(), InstallError> {
    match command {
        Some(cmd) => {
            let (run, _) = run_root(machine).create_subkey(RUN_KEY)?;
            run.set_value(RUN_VALUE, &cmd.to_string())?;
        }
        None => {
            if let Ok(run) = run_root(machine).open_subkey_with_flags(RUN_KEY, KEY_WRITE) {
                let _ = run.delete_value(RUN_VALUE);
            }
        }
    }
    Ok(())
}

/// The autostart command, if one is registered.
pub fn autostart_command(machine: bool) -> Option<String> {
    run_root(machine)
        .open_subkey(RUN_KEY)
        .ok()?
        .get_value::<String, _>(RUN_VALUE)
        .ok()
}

/// Start-up task and hourly task, both as SYSTEM.
pub fn register_machine_tasks(command: &str) -> std::result::Result<(), InstallError> {
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
        command,
    ])?;
    schtasks(&[
        "/Create",
        "/F",
        "/TN",
        TASK_MACHINE_HOURLY,
        "/SC",
        "HOURLY",
        "/RU",
        "SYSTEM",
        "/TR",
        command,
    ])
}

pub fn remove_machine_tasks() -> std::result::Result<(), InstallError> {
    let _ = schtasks(&["/Delete", "/F", "/TN", TASK_MACHINE]);
    let _ = schtasks(&["/Delete", "/F", "/TN", TASK_MACHINE_HOURLY]);
    Ok(())
}

/// Hourly task for the signed-in user (no administrator rights needed).
pub fn register_user_task(command: &str) -> std::result::Result<(), InstallError> {
    schtasks(&[
        "/Create", "/F", "/TN", TASK_USER, "/SC", "HOURLY", "/TR", command,
    ])
}

pub fn remove_user_task() -> std::result::Result<(), InstallError> {
    let _ = schtasks(&["/Delete", "/F", "/TN", TASK_USER]);
    Ok(())
}

fn schtasks(args: &[&str]) -> std::result::Result<(), InstallError> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = Command::new("schtasks.exe")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
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

/// Where the app is placed when it is set up for all users.
pub fn machine_program_dir() -> PathBuf {
    std::env::var_os("ProgramFiles")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"))
        .join("Chirp Wallpaper")
}

/// Show a file selected in Explorer.
pub fn show_in_folder(file: &Path) {
    let _ = Command::new("explorer.exe")
        .arg(format!("/select,{}", file.display()))
        .spawn();
}
