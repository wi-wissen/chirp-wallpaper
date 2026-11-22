//! GNOME (and other desktops reading the GNOME settings).
//!
//! The desktop background is set with `gsettings`. GNOME's lock screen shows
//! the desktop background (blurred), so it follows automatically; we also set
//! the older screensaver key for desktops that still read it. The login
//! screen (GDM) has no supported setting and is left alone.

use super::InstallError;
use chirp_core::{Error, Platform, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct NativePlatform;

impl NativePlatform {
    pub fn new(_machine: bool) -> Self {
        NativePlatform
    }
}

impl Platform for NativePlatform {
    fn set_desktop(&self, image: &Path) -> Result<()> {
        let uri = file_uri(image);
        gsettings("org.gnome.desktop.background", "picture-uri", &uri)?;
        // Since GNOME 42 dark mode reads its own key; missing on older versions.
        let _ = gsettings("org.gnome.desktop.background", "picture-uri-dark", &uri);
        let _ = gsettings("org.gnome.desktop.background", "picture-options", "zoom");
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

fn systemd_dir() -> PathBuf {
    dirs_config().join("systemd/user")
}

fn dirs_config() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
}

/// Register a systemd user timer: at login and every hour.
pub fn install(machine: bool) -> std::result::Result<(), InstallError> {
    if machine {
        return Err(InstallError::NotSupported);
    }
    let exe = std::env::current_exe()?;
    let dir = systemd_dir();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join("chirp-wallpaper.service"),
        format!(
            "[Unit]\nDescription=Chirp Wallpaper: daily picture\nAfter=graphical-session.target network-online.target\n\n\
             [Service]\nType=oneshot\nExecStart=\"{}\" apply --quiet\n",
            exe.display()
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

pub fn uninstall(machine: bool) -> std::result::Result<(), InstallError> {
    if machine {
        return Err(InstallError::NotSupported);
    }
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
