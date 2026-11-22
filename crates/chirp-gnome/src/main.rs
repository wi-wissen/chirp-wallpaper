//! Chirp Wallpaper for GNOME: a native GTK 4 / libadwaita app.
//!
//! GNOME has no tray icons, so background operation follows GNOME practice:
//! with "start automatically" on, an autostart entry launches the app with
//! `--background`; it keeps running without a window, checks every hour, and
//! starting it again (e.g. from the app grid) shows the window. Only one
//! instance runs (GApplication).

#[cfg(target_os = "linux")]
mod app;
#[cfg(target_os = "linux")]
mod portal;

#[cfg(target_os = "linux")]
fn main() -> gtk::glib::ExitCode {
    app::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("The GNOME app runs on Linux only. On Windows, use ChirpWallpaper.exe.");
}
