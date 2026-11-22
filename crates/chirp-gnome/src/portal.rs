//! Inside a Flatpak the app cannot touch the desktop settings directly.
//! It asks the desktop through XDG portals instead:
//! * Wallpaper portal: set the picture as background (GNOME then also shows
//!   it, blurred, on the lock screen). GNOME may ask once for permission.
//! * Background portal: start at sign-in with `--background`.
//!
//! Outside a Flatpak the regular backends (gsettings, XDG autostart) are used.

use ashpd::desktop::background::Background;
use ashpd::desktop::wallpaper::{SetOn, WallpaperRequest};
use chirp_core::{Error, Platform, Result};
use std::path::{Path, PathBuf};

/// True when running inside a Flatpak sandbox.
pub fn in_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

pub struct PortalPlatform;

impl Platform for PortalPlatform {
    fn set_desktop(&self, image: &Path) -> Result<()> {
        let file = std::fs::File::open(image)?;
        pollster::block_on(async {
            WallpaperRequest::default()
                .set_on(SetOn::Both)
                .show_preview(false)
                .build_file(&file)
                .await?
                .response()
        })
        .map_err(|e: ashpd::Error| Error::Platform(format!("wallpaper portal: {e}")))
    }

    fn set_lockscreen(&self, _image: &Path) -> Result<()> {
        // Covered by `SetOn::Both` above.
        Ok(())
    }
}

/// Ask the desktop to start (or stop starting) the app at sign-in.
pub fn request_autostart(enable: bool, reason: &str) -> std::result::Result<(), String> {
    pollster::block_on(async {
        Background::request()
            .reason(reason)
            .auto_start(enable)
            .command(&["chirp-wallpaper", "--background"])
            .dbus_activatable(false)
            .send()
            .await?
            .response()
    })
    .map(|_| ())
    .map_err(|e| e.to_string())?;
    let marker = marker_file();
    if enable {
        if let Some(dir) = marker.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&marker, "");
    } else {
        let _ = std::fs::remove_file(&marker);
    }
    Ok(())
}

/// The portal cannot be asked whether autostart is on, so remember it.
pub fn autostart_enabled() -> bool {
    marker_file().exists()
}

fn marker_file() -> PathBuf {
    chirp_core::Paths::user().data_dir.join("autostart")
}
