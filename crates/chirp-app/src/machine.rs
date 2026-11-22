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
    match chirp_core::apply(&cfg, &paths, &NativePlatform::new(true), &market) {
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
    let _ = chirp_core::apply(&run_cfg, &paths, &NativePlatform::new(true), &market);
    let _ = seal(&paths);
    Ok(())
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
            crate::with_window(|ui| ui.set_busy(false));
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
