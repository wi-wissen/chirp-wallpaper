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
use chirp_core::{Error, Fit, Layout, Platform, Result, Screens};
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
    fn set_desktop(&self, image: &Path, layout: &Layout) -> Result<()> {
        if self.machine {
            // SYSTEM has no desktop; each user's sign-in sets it from the shared cache.
            return Ok(());
        }
        // Keep 4K pictures sharp (Windows re-compresses them by default).
        if let Ok((desk, _)) =
            RegKey::predef(HKEY_CURRENT_USER).create_subkey(r"Control Panel\Desktop")
        {
            let _ = desk.set_value("JPEGImportQuality", &100u32);
        }
        // IDesktopWallpaper knows placement and single screens. The classic
        // call is only a fallback when all screens get the picture anyway;
        // for "main screen only" an error is reported instead of silently
        // changing every screen.
        match set_with_desktop_wallpaper(image, layout) {
            Ok(()) => Ok(()),
            Err(e) if layout.screens == Screens::Primary && layout.fit != Fit::Span => {
                Err(Error::Platform(e.message().to_string()))
            }
            Err(_) => set_classic(image, layout),
        }
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

fn set_with_desktop_wallpaper(image: &Path, layout: &Layout) -> windows::core::Result<()> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::System::Com::*;
    use windows::Win32::UI::Shell::*;
    unsafe {
        // Fine if COM is already initialised on this thread.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let wallpaper: IDesktopWallpaper = CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL)?;
        let position = match layout.fit {
            Fit::Fill => DWPOS_FILL,
            Fit::Fit => DWPOS_FIT,
            Fit::Stretch => DWPOS_STRETCH,
            Fit::Center => DWPOS_CENTER,
            Fit::Span => DWPOS_SPAN,
        };
        let path = HSTRING::from(image.as_os_str());
        if layout.screens == Screens::All || layout.fit == Fit::Span {
            wallpaper.SetPosition(position)?;
            return wallpaper.SetWallpaper(PCWSTR::null(), &path);
        }

        // Only the main screen. Windows has one background type for all
        // screens: as soon as one screen gets a picture, the others get it
        // too unless they are given their own. So first note what each other
        // screen shows now (its picture, or the solid colour), then set ours,
        // then give the others back what they had.
        let solid_colour = current_background_is_colour();
        let count = wallpaper.GetMonitorDevicePathCount()?;
        let mut primary_id = None;
        let mut others = Vec::new();
        for i in 0..count {
            let id = wallpaper.GetMonitorDevicePathAt(i)?;
            let id_text = id.to_string().unwrap_or_default();
            let is_primary = wallpaper
                .GetMonitorRECT(PCWSTR(id.0))
                .is_ok_and(|r| r.left == 0 && r.top == 0);
            if is_primary {
                primary_id = Some(id_text);
            } else {
                let keep = if solid_colour {
                    None
                } else {
                    wallpaper.GetWallpaper(PCWSTR(id.0)).ok().and_then(|p| {
                        let text = p.to_string().unwrap_or_default();
                        CoTaskMemFree(Some(p.0 as *const core::ffi::c_void));
                        (!text.is_empty() && Path::new(&text).is_file()).then_some(text)
                    })
                };
                others.push((id_text, keep));
            }
            CoTaskMemFree(Some(id.0 as *const core::ffi::c_void));
        }
        let Some(primary_id) = primary_id else {
            return Err(windows::core::Error::new(
                windows::Win32::Foundation::E_FAIL,
                "main screen not found",
            ));
        };

        wallpaper.SetPosition(position)?;
        wallpaper.SetWallpaper(&HSTRING::from(primary_id.as_str()), &path)?;
        for (id, keep) in others {
            let picture = match keep {
                Some(p) => p,
                None => {
                    // The other screen showed a solid colour: give it a picture of that colour.
                    let colour = wallpaper.GetBackgroundColor().map(|c| c.0).unwrap_or(0);
                    match colour_picture(colour) {
                        Some(p) => p.to_string_lossy().to_string(),
                        None => continue,
                    }
                }
            };
            wallpaper.SetWallpaper(
                &HSTRING::from(id.as_str()),
                &HSTRING::from(picture.as_str()),
            )?;
        }
        Ok(())
    }
}

/// Undo the machine-wide lock screen picture.
pub fn remove_machine_lockscreen() -> std::result::Result<(), InstallError> {
    if let Ok(key) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(CSP_KEY, KEY_WRITE) {
        for value in [
            "LockScreenImagePath",
            "LockScreenImageUrl",
            "LockScreenImageStatus",
        ] {
            let _ = key.delete_value(value);
        }
    }
    Ok(())
}

/// Whether "Background: Solid colour" is selected in the Windows settings.
fn current_background_is_colour() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Control Panel\Desktop")
        .and_then(|k| k.get_value::<String, _>("WallPaper"))
        .map(|v| v.trim().is_empty())
        .unwrap_or(false)
}

/// A small picture filled with a COLORREF (0x00BBGGRR) colour, kept in the app's folder.
fn colour_picture(colorref: u32) -> Option<PathBuf> {
    let r = (colorref & 0xFF) as u8;
    let g = ((colorref >> 8) & 0xFF) as u8;
    let b = ((colorref >> 16) & 0xFF) as u8;
    let dir = chirp_core::Paths::user().data_dir;
    std::fs::create_dir_all(&dir).ok()?;
    let file = dir.join(format!("colour-{r:02x}{g:02x}{b:02x}.bmp"));
    if !file.is_file() {
        std::fs::write(&file, bmp(r, g, b)).ok()?;
    }
    Some(file)
}

/// An 8×8 24-bit BMP of one colour (no image library needed).
fn bmp(r: u8, g: u8, b: u8) -> Vec<u8> {
    const SIDE: u32 = 8;
    let row = SIDE * 3; // already a multiple of 4
    let size = 54 + row * SIDE;
    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(SIDE as i32).to_le_bytes());
    out.extend_from_slice(&(SIDE as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    for _ in 0..SIDE * SIDE {
        out.extend_from_slice(&[b, g, r]);
    }
    out
}

fn set_classic(image: &Path, layout: &Layout) -> Result<()> {
    let (style, tile) = match layout.fit {
        Fit::Fill => ("10", "0"),
        Fit::Fit => ("6", "0"),
        Fit::Stretch => ("2", "0"),
        Fit::Center => ("0", "0"),
        Fit::Span => ("22", "0"),
    };
    if let Ok((desk, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(r"Control Panel\Desktop")
    {
        let _ = desk.set_value("WallpaperStyle", &style);
        let _ = desk.set_value("TileWallpaper", &tile);
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
