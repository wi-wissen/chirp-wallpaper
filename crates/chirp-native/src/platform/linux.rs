//! GNOME (and other desktops reading the GNOME settings).
//!
//! The desktop background is set with `gsettings`. GNOME's lock screen shows
//! the desktop background (blurred), so it follows automatically; we also set
//! the older screensaver key for desktops that still read it. The login
//! screen (GDM) has no supported setting and is left alone.
//!
//! Autostart uses an XDG autostart entry, periodic updates a systemd user timer.

use super::InstallError;
use chirp_core::{Error, Fit, Layout, Platform, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct NativePlatform;

impl NativePlatform {
    pub fn new(_machine: bool) -> Self {
        NativePlatform
    }
}

impl Platform for NativePlatform {
    fn set_desktop(&self, image: &Path, layout: &Layout) -> Result<()> {
        let uri = file_uri(image);
        gsettings("org.gnome.desktop.background", "picture-uri", &uri)?;
        // Since GNOME 42 dark mode reads its own key; missing on older versions.
        let _ = gsettings("org.gnome.desktop.background", "picture-uri-dark", &uri);
        let options = match layout.fit {
            Fit::Fill => "zoom",
            Fit::Fit => "scaled",
            Fit::Stretch => "stretched",
            Fit::Center => "centered",
            Fit::Span => "spanned",
        };
        let _ = gsettings("org.gnome.desktop.background", "picture-options", options);
        Ok(())
    }

    fn set_lockscreen(&self, image: &Path) -> Result<()> {
        // Current GNOME shows the blurred background; older versions read this key.
        let _ = gsettings(
            "org.gnome.desktop.screensaver",
            "picture-uri",
            &file_uri(image),
        );
        Ok(())
    }
}

fn gsettings(schema: &str, key: &str, value: &str) -> Result<()> {
    let out = Command::new("gsettings")
        .args(["set", schema, key, value])
        .output()
        .map_err(|e| Error::Platform(format!("gsettings: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(Error::Platform(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ))
    }
}

/// `file://` URI with percent-encoding for characters outside the safe set.
pub fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for b in path.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            uri.push(b as char);
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    uri
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
}

fn autostart_file(machine: bool) -> PathBuf {
    if machine {
        PathBuf::from("/etc/xdg/autostart/chirp-wallpaper.desktop")
    } else {
        config_home().join("autostart/chirp-wallpaper.desktop")
    }
}

/// Start at sign-in (`Some(command)`) or not (`None`).
pub fn set_autostart(
    machine: bool,
    command: Option<&str>,
) -> std::result::Result<(), InstallError> {
    let file = autostart_file(machine);
    match command {
        Some(cmd) => {
            if let Some(dir) = file.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(
                &file,
                format!(
                    "[Desktop Entry]\nType=Application\nName=Chirp Wallpaper\nExec={cmd}\n\
                     Icon=chirp-wallpaper\nNoDisplay=true\nX-GNOME-Autostart-enabled=true\n"
                ),
            )?;
        }
        None => {
            if file.exists() {
                std::fs::remove_file(&file)?;
            }
        }
    }
    Ok(())
}

/// The autostart command, if one is registered.
pub fn autostart_command(machine: bool) -> Option<String> {
    let text = std::fs::read_to_string(autostart_file(machine)).ok()?;
    text.lines()
        .find_map(|l| l.strip_prefix("Exec="))
        .map(str::to_string)
}

pub fn register_machine_tasks(_command: &str) -> std::result::Result<(), InstallError> {
    Err(InstallError::NotSupported)
}

pub fn remove_machine_tasks() -> std::result::Result<(), InstallError> {
    Err(InstallError::NotSupported)
}

fn systemd_dir() -> PathBuf {
    config_home().join("systemd/user")
}

/// systemd user timer: shortly after login and every hour.
pub fn register_user_task(command: &str) -> std::result::Result<(), InstallError> {
    let dir = systemd_dir();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join("chirp-wallpaper.service"),
        format!(
            "[Unit]\nDescription=Chirp Wallpaper: daily picture\n\
             After=graphical-session.target network-online.target\n\n\
             [Service]\nType=oneshot\nExecStart={command}\n"
        ),
    )?;
    std::fs::write(
        dir.join("chirp-wallpaper.timer"),
        "[Unit]\nDescription=Chirp Wallpaper: check for a new picture\n\n\
         [Timer]\nOnStartupSec=1min\nOnUnitActiveSec=1h\nPersistent=true\n\n\
         [Install]\nWantedBy=timers.target\n",
    )?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", "--now", "chirp-wallpaper.timer"])
}

pub fn remove_user_task() -> std::result::Result<(), InstallError> {
    let _ = systemctl(&["disable", "--now", "chirp-wallpaper.timer"]);
    let dir = systemd_dir();
    let _ = std::fs::remove_file(dir.join("chirp-wallpaper.timer"));
    let _ = std::fs::remove_file(dir.join("chirp-wallpaper.service"));
    let _ = systemctl(&["daemon-reload"]);
    Ok(())
}

fn systemctl(args: &[&str]) -> std::result::Result<(), InstallError> {
    let out = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .map_err(|e| InstallError::Failed(format!("systemctl: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(InstallError::Failed(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ))
    }
}

/// Where the app is placed when it is set up for all users.
pub fn machine_program_dir() -> PathBuf {
    PathBuf::from("/opt/chirp-wallpaper")
}

/// Open the folder containing a file.
pub fn show_in_folder(file: &Path) {
    if let Some(dir) = file.parent() {
        let _ = Command::new("xdg-open").arg(dir).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_is_encoded() {
        assert_eq!(
            file_uri(Path::new("/home/a b/Bild ä.jpg")),
            "file:///home/a%20b/Bild%20%C3%A4.jpg"
        );
    }
}
