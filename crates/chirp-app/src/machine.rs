//! Setting up for all users (reset school computers) and the SYSTEM task.
//!
//! The app copies itself to Program Files, writes the machine-wide settings,
//! registers a start-up task running as SYSTEM (`--apply-machine`: download
//! into ProgramData, set the lock screen for everyone) and starts itself
//! hidden at every user's sign-in (`--background`: set that user's desktop
//! from the shared download). Finally it loads today's picture and keeps it as
//! baseline for days without internet.

use chirp_core::apply::seal;
use chirp_core::{Config, Paths, Resolution};
use chirp_native::{platform, Localizer, NativePlatform};

/// Run by the SYSTEM task. Returns the process exit code.
pub fn apply_machine() -> i32 {
    let paths = Paths::machine();
    let Ok(mut cfg) = Config::load(&paths.config_files) else {
        return 1;
    };
    cfg.desktop = false;
    let market = cfg.resolved_market(sys_locale::get_locale().as_deref());
    match chirp_core::apply(
        &cfg,
        &paths,
        &NativePlatform::new(true),
        &market,
        chirp_core::Trigger::Automatic,
    ) {
        Ok(chirp_core::Outcome::Unchanged { .. }) => 2,
        Ok(_) => 0,
        Err(_) => 1,
    }
}

/// Run elevated: `--setup-machine [--market X] [--resolution uhd|1080p]`.
pub fn setup_machine(args: &[String]) -> i32 {
    let value = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    match setup(value("--market"), value("--resolution")) {
        Ok(()) => 0,
        Err(e) => {
            let _ = std::fs::write(std::env::temp_dir().join("chirp-setup-error.txt"), e);
            1
        }
    }
}

fn setup(market: Option<String>, resolution: Option<String>) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = platform::machine_program_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = dir.join(
        exe.file_name()
            .unwrap_or_else(|| "ChirpWallpaper.exe".as_ref()),
    );
    if target != exe {
        std::fs::copy(&exe, &target).map_err(|e| e.to_string())?;
    }

    let paths = Paths::machine();
    let mut cfg = Config::load(&[&paths.config_file]).unwrap_or_default();
    if let Some(m) = market {
        cfg.market = m;
    }
    if let Some(r) = resolution {
        cfg.resolution = if r == "1080p" {
            Resolution::FullHd
        } else {
            Resolution::Uhd
        };
    }
    cfg.save(&paths.config_file).map_err(|e| e.to_string())?;

    platform::register_machine_tasks(&platform::command_line(&target, "--apply-machine"))
        .map_err(|e| e.to_string())?;
    platform::set_autostart(true, Some(&platform::command_line(&target, "--background")))
        .map_err(|e| e.to_string())?;

    // Load today's picture now and keep it as baseline.
    let mut run_cfg = cfg.clone();
    run_cfg.desktop = false;
    let market = run_cfg.resolved_market(sys_locale::get_locale().as_deref());
    let _ = chirp_core::apply(
        &run_cfg,
        &paths,
        &NativePlatform::new(true),
        &market,
        chirp_core::Trigger::Manual,
    );
    let _ = seal(&paths);
    Ok(())
}

/// Whether this computer was set up for all users.
pub fn is_set_up() -> bool {
    Paths::machine().data_dir.is_dir() || platform::autostart_command(true).is_some()
}

/// Run elevated: undo [`setup_machine`].
pub fn remove_machine() -> i32 {
    let _ = platform::remove_machine_tasks();
    let _ = platform::set_autostart(true, None);
    let _ = platform::remove_machine_lockscreen();
    let _ = std::fs::remove_dir_all(Paths::machine().data_dir);
    let dir = platform::machine_program_dir();
    let running_from_there = std::env::current_exe().is_ok_and(|e| e.starts_with(&dir));
    if !running_from_there {
        let _ = std::fs::remove_dir_all(&dir);
    }
    i32::from(is_set_up())
}

/// Remove everything this app stored for the signed-in user: pictures,
/// history, settings and autostart. The current background stays, Windows
/// keeps its own copy of it.
pub fn clear_user_data() {
    let paths = Paths::detect(false);
    let _ = platform::set_autostart(false, None);
    let _ = platform::remove_user_task();
    let _ = std::fs::remove_dir_all(&paths.data_dir);
    let _ = std::fs::remove_file(&paths.config_file);
    if let Some(dir) = paths.config_file.parent() {
        // Only removes the folder when it is empty, i.e. our own folder.
        let _ = std::fs::remove_dir(dir);
    }
}

/// Start the elevated removal from the window and report the result.
pub fn start_remove(l: &Localizer) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    crate::with_window(|ui| ui.set_busy(true));
    crate::toast(&l.t("admin-running", &[]));
    std::thread::spawn(move || {
        let ok = run_elevated(&exe, &["--remove-machine"]);
        let _ = slint::invoke_from_event_loop(move || {
            let l = Localizer::new(None);
            crate::with_window(|ui| {
                ui.set_busy(false);
                ui.set_machine_setup(is_set_up());
            });
            crate::toast(&l.t(if ok { "unsetup-done" } else { "admin-failed" }, &[]));
        });
    });
}

/// Start the elevated setup from the window and report the result.
pub fn start_setup(l: &Localizer) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let cfg = crate::effective_config();
    let resolution = match cfg.resolution {
        Resolution::Uhd => "uhd",
        Resolution::FullHd => "1080p",
    };
    let market = cfg.market.clone();
    crate::with_window(|ui| ui.set_busy(true));
    crate::toast(&l.t("admin-running", &[]));
    std::thread::spawn(move || {
        let ok = run_elevated(
            &exe,
            &[
                "--setup-machine",
                "--market",
                &market,
                "--resolution",
                resolution,
            ],
        );
        let _ = slint::invoke_from_event_loop(move || {
            let l = Localizer::new(None);
            crate::with_window(|ui| {
                ui.set_busy(false);
                ui.set_machine_setup(is_set_up());
            });
            crate::toast(&l.t(if ok { "admin-done" } else { "admin-failed" }, &[]));
        });
    });
}

#[cfg(windows)]
fn run_elevated(exe: &std::path::Path, args: &[&str]) -> bool {
    runas::Command::new(exe)
        .args(args)
        .gui(true)
        .show(false)
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(not(windows))]
fn run_elevated(_exe: &std::path::Path, _args: &[&str]) -> bool {
    false
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn clear_user_data_removes_everything_of_this_user() {
        // A private home, so the test never touches real files.
        let home = std::env::temp_dir().join(format!("chirp-clear-{}", std::process::id()));
        let data = home.join("data");
        let config = home.join("config");
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_DATA_HOME", &data);
        std::env::set_var("XDG_CONFIG_HOME", &config);

        let paths = Paths::user();
        std::fs::create_dir_all(paths.images_dir()).unwrap();
        std::fs::write(paths.images_dir().join("x.jpg"), b"x").unwrap();
        std::fs::write(paths.state_file(), b"{}").unwrap();
        Config::default().save(&paths.config_file).unwrap();
        platform::set_autostart(false, Some("chirp --background")).unwrap();
        assert!(platform::autostart_command(false).is_some());

        clear_user_data();

        assert!(!paths.data_dir.exists());
        assert!(!paths.config_file.exists());
        assert!(platform::autostart_command(false).is_none());
        std::fs::remove_dir_all(home).unwrap();
    }
}
