//! Chirp Wallpaper app: window, tray icon and background updates.
//!
//! Start modes:
//! * no arguments: open the window;
//! * `--background`: start hidden (used by autostart); the tray icon opens it;
//! * `--apply-machine`: update the shared picture and the lock screen for all
//!   users, without a window (used by the SYSTEM start-up task);
//! * `--setup-machine`: set up for all users (started elevated by the app).

#![cfg_attr(windows, windows_subsystem = "windows")]

mod machine;
mod model;
#[cfg(windows)]
mod single;
#[cfg(windows)]
mod tray;

use chirp_core::{Config, Outcome, Paths, Resolution};
use chirp_native::{platform, Localizer, NativePlatform};
use model::Snapshot;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

slint::include_modules!();

pub const WEBSITE: &str = "https://wi-wissen.github.io/chirp-wallpaper/";
pub const SOURCE: &str = "https://github.com/wi-wissen/chirp-wallpaper";

thread_local! {
    /// The window, reachable from tray and single-instance events.
    static WINDOW: RefCell<Option<slint::Weak<AppWindow>>> = const { RefCell::new(None) };
    /// Data behind the window (history list, current picture).
    static SNAPSHOT: RefCell<Snapshot> = RefCell::new(Snapshot::default());
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);

    if has("--apply-machine") {
        std::process::exit(machine::apply_machine());
    }
    if has("--setup-machine") {
        std::process::exit(machine::setup_machine(&args));
    }

    #[cfg(windows)]
    if !single::acquire() {
        // Already running: bring that window to the front instead.
        single::signal_existing();
        return;
    }

    if let Err(e) = run(has("--background")) {
        eprintln!("Chirp Wallpaper: {e}");
    }
}

fn run(background: bool) -> Result<(), slint::PlatformError> {
    let l = Rc::new(Localizer::new(None));
    let ui = AppWindow::new()?;
    WINDOW.with(|w| *w.borrow_mut() = Some(ui.as_weak()));

    fill_texts(&ui, &l);
    ui.set_version(format!("Version {}", env!("CARGO_PKG_VERSION")).into());
    ui.set_is_windows(cfg!(windows));
    load_settings(&ui, &l);
    connect(&ui, l.clone());

    #[cfg(windows)]
    let _tray = tray::create(&l);
    #[cfg(windows)]
    single::listen();

    #[cfg(not(windows))]
    ui.window().on_close_requested(|| {
        let _ = slint::quit_event_loop();
        slint::CloseRequestResponse::HideWindow
    });

    if let Some(page) = std::env::var("CHIRP_PAGE")
        .ok()
        .and_then(|p| p.parse().ok())
    {
        ui.set_page(page);
    }
    if std::env::var_os("CHIRP_CONFIRM").is_some() {
        ui.set_confirm_admin(true);
    }
    refresh();
    if !background {
        ui.show()?;
    }

    // First check shortly after start, then every hour.
    let first = slint::Timer::default();
    first.start(slint::TimerMode::SingleShot, Duration::from_secs(3), || {
        update(false)
    });
    let hourly = slint::Timer::default();
    hourly.start(
        slint::TimerMode::Repeated,
        Duration::from_secs(3600),
        || update(false),
    );

    slint::run_event_loop_until_quit()
}

/// Run something with the window, if it still exists.
pub fn with_window(f: impl FnOnce(&AppWindow)) {
    WINDOW.with(|w| {
        if let Some(ui) = w.borrow().as_ref().and_then(|w| w.upgrade()) {
            f(&ui);
        }
    });
}

/// Show the window and bring it to the front.
pub fn show_window() {
    with_window(|ui| {
        let _ = ui.show();
        ui.window().set_minimized(false);
        #[cfg(windows)]
        single::bring_to_front();
    });
}

fn fill_texts(ui: &AppWindow, l: &Localizer) {
    let tr = ui.global::<Tr>();
    macro_rules! tr {
        ($($setter:ident => $id:literal),* $(,)?) => {
            $( tr.$setter(l.t($id, &[]).into()); )*
        };
    }
    tr!(
        set_nav_today => "nav-today",
        set_nav_history => "nav-history",
        set_nav_settings => "nav-settings",
        set_nav_about => "nav-about",
        set_today_update => "today-update",
        set_today_more => "label-more",
        set_today_show_file => "today-show-file",
        set_today_empty => "today-empty",
        set_history_empty => "history-empty",
        set_history_use => "history-use",
        set_settings_general => "settings-general",
        set_settings_autostart => "settings-autostart",
        set_settings_autostart_desc => "settings-autostart-desc",
        set_settings_desktop => "settings-desktop",
        set_settings_lockscreen => "settings-lockscreen",
        set_settings_lockscreen_desc => "settings-lockscreen-desc",
        set_settings_picture => "settings-picture",
        set_settings_region => "settings-region",
        set_settings_region_desc => "settings-region-desc",
        set_settings_resolution => "settings-resolution",
        set_settings_pause => "settings-pause",
        set_settings_pause_desc => "settings-pause-desc",
        set_settings_admin => "settings-admin",
        set_settings_admin_title => "settings-admin-title",
        set_settings_admin_desc => "settings-admin-desc",
        set_settings_admin_button => "settings-admin-button",
        set_admin_confirm_title => "admin-confirm-title",
        set_admin_confirm_intro => "admin-confirm-intro",
        set_admin_step_copy => "admin-step-copy",
        set_admin_step_lock => "admin-step-lock",
        set_admin_step_signin => "admin-step-signin",
        set_admin_step_baseline => "admin-step-baseline",
        set_admin_confirm_uac => "admin-confirm-uac",
        set_admin_confirm_ok => "admin-confirm-ok",
        set_cancel => "cancel",
        set_label_baseline => "label-baseline",
        set_about_text => "about-text",
        set_about_website => "about-website",
        set_about_source => "about-source",
    );
    let mut regions = vec![SharedString::from(l.t("settings-region-auto", &[]))];
    regions.extend(
        model::REGIONS
            .iter()
            .map(|(_, name)| SharedString::from(*name)),
    );
    ui.set_regions(ModelRc::new(VecModel::from(regions)));
    let pauses: Vec<SharedString> = ["pause-off", "pause-day", "pause-week", "pause-month"]
        .iter()
        .map(|id| l.t(id, &[]).into())
        .collect();
    ui.set_pause_options(ModelRc::new(VecModel::from(pauses)));
}

fn user_paths() -> Paths {
    Paths::detect(false)
}

/// Settings of this user only (machine settings are not shown or changed).
fn user_config() -> Config {
    Config::load(&[user_paths().config_file]).unwrap_or_default()
}

fn effective_config() -> Config {
    Config::load(&user_paths().config_files).unwrap_or_default()
}

fn load_settings(ui: &AppWindow, _l: &Localizer) {
    let cfg = effective_config();
    ui.set_desktop(cfg.desktop);
    ui.set_lockscreen(cfg.lockscreen);
    ui.set_region_index(model::region_index(&cfg.market));
    ui.set_resolution_index(match cfg.resolution {
        Resolution::Uhd => 0,
        Resolution::FullHd => 1,
    });
    ui.set_pause_index(model::pause_index(
        cfg.paused_until.as_deref(),
        &chirp_core::today(),
    ));
    ui.set_autostart(
        platform::autostart_command(false).is_some_and(|c| c.contains("--background")),
    );
}

fn save_settings(ui: &AppWindow) {
    let paths = user_paths();
    let mut cfg = user_config();
    cfg.desktop = ui.get_desktop();
    cfg.lockscreen = ui.get_lockscreen();
    cfg.market = model::region_code(ui.get_region_index()).to_string();
    cfg.resolution = if ui.get_resolution_index() == 1 {
        Resolution::FullHd
    } else {
        Resolution::Uhd
    };
    let _ = cfg.save(&paths.config_file);

    let wanted = ui.get_autostart();
    let current = platform::autostart_command(false).is_some_and(|c| c.contains("--background"));
    if wanted != current {
        let result = match std::env::current_exe() {
            Ok(exe) if wanted => {
                platform::set_autostart(false, Some(&platform::command_line(&exe, "--background")))
            }
            Ok(_) => platform::set_autostart(false, None),
            Err(e) => Err(e.into()),
        };
        if result.is_err() {
            ui.set_autostart(current);
        }
    }
}

fn connect(ui: &AppWindow, l: Rc<Localizer>) {
    ui.on_update_now(|| update(true));
    ui.on_open_info(|| {
        SNAPSHOT.with(|s| {
            if let Some(url) = s.borrow().current.as_ref().and_then(|p| p.info_url.clone()) {
                let _ = open::that(url);
            }
        })
    });
    ui.on_show_file(|| {
        let file = SNAPSHOT.with(|s| s.borrow().current_file.clone());
        if let Some(file) = file {
            platform::show_in_folder(&file);
        }
    });
    {
        let l = l.clone();
        ui.on_use_history(move |index| use_history(&l, index as usize));
    }
    {
        let weak = ui.as_weak();
        ui.on_settings_changed(move || {
            if let Some(ui) = weak.upgrade() {
                save_settings(&ui);
            }
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_pause_changed(move || {
            if let Some(ui) = weak.upgrade() {
                let paths = user_paths();
                let mut cfg = user_config();
                cfg.paused_until = model::pause_until(ui.get_pause_index(), &chirp_core::today());
                let _ = cfg.save(&paths.config_file);
                refresh();
            }
        });
    }
    {
        let l = l.clone();
        ui.on_setup_machine(move || machine::start_setup(&l));
    }
    ui.on_open_website(|| {
        let _ = open::that(WEBSITE);
    });
    ui.on_open_source(|| {
        let _ = open::that(SOURCE);
    });
}

/// Load state and pictures from disk in the background, then show them.
pub fn refresh() {
    let paths = user_paths();
    std::thread::spawn(move || {
        let data = model::load(&paths);
        let _ = slint::invoke_from_event_loop(move || {
            let l = Rc::new(Localizer::new(None));
            show_snapshot(&l, data);
        });
    });
}

fn show_snapshot(l: &Localizer, data: model::Loaded) {
    with_window(|ui| {
        let cfg = effective_config();
        let state = &data.state;
        match &state.current {
            Some(p) => {
                ui.set_pic_title(p.title.clone().into());
                ui.set_pic_caption(
                    if p.caption != p.title {
                        p.caption.clone()
                    } else {
                        String::new()
                    }
                    .into(),
                );
                ui.set_pic_meta(model::meta_line(p).into());
                ui.set_pic_copyright(p.copyright.clone().into());
                ui.set_pic_description(p.description.clone().unwrap_or_default().into());
                ui.set_has_info_url(p.info_url.is_some());
            }
            None => {
                ui.set_pic_title(SharedString::default());
                ui.set_pic_caption(SharedString::default());
                ui.set_pic_meta(SharedString::default());
                ui.set_pic_copyright(SharedString::default());
                ui.set_pic_description(SharedString::default());
                ui.set_has_info_url(false);
            }
        }
        match data.picture {
            Some(buf) => {
                ui.set_picture(slint::Image::from_rgb8(buf));
                ui.set_has_picture(true);
            }
            None => ui.set_has_picture(false),
        }
        let (status, is_error) = model::status_line(l, &cfg, state, &chirp_core::today());
        ui.set_status_text(status.into());
        ui.set_status_is_error(is_error);
        ui.set_baseline_text(
            data.baseline
                .as_ref()
                .map(|p| format!("{}: {} ({})", l.t("label-baseline", &[]), p.title, p.date))
                .unwrap_or_default()
                .into(),
        );
        let items: Vec<HistoryItem> = data
            .history
            .iter()
            .enumerate()
            .map(|(i, (c, thumb))| HistoryItem {
                thumb: thumb
                    .clone()
                    .map(slint::Image::from_rgb8)
                    .unwrap_or_default(),
                title: c.picture.title.clone().into(),
                date: c.picture.date.clone().into(),
                col: (i % 3) as i32,
                row: (i / 3) as i32,
            })
            .collect();
        ui.set_history(ModelRc::new(VecModel::from(items)));
        ui.set_pause_index(model::pause_index(
            cfg.paused_until.as_deref(),
            &chirp_core::today(),
        ));
        SNAPSHOT.with(|s| {
            *s.borrow_mut() = Snapshot {
                current: state.current.clone(),
                current_file: state.current_file.clone(),
                history: data.history.into_iter().map(|(c, _)| c).collect(),
            }
        });
    });
}

/// Check for a new picture in the background.
pub fn update(manual: bool) {
    let mut already_busy = false;
    with_window(|ui| {
        already_busy = ui.get_busy();
        ui.set_busy(true);
    });
    if already_busy {
        return;
    }
    let paths = user_paths();
    let cfg = effective_config();
    let market = cfg.resolved_market(sys_locale::get_locale().as_deref());
    std::thread::spawn(move || {
        let outcome = chirp_core::apply(&cfg, &paths, &NativePlatform::new(false), &market);
        let _ = slint::invoke_from_event_loop(move || {
            let l = Rc::new(Localizer::new(None));
            with_window(|ui| ui.set_busy(false));
            if manual {
                let msg = match &outcome {
                    Ok(Outcome::Updated { picture, .. }) => {
                        l.t("updated", &[("title", &picture.title)])
                    }
                    Ok(Outcome::AlreadyCurrent { picture, .. }) => {
                        l.t("already-current", &[("title", &picture.title)])
                    }
                    Ok(Outcome::Paused { until }) => l.t("paused", &[("date", until)]),
                    Ok(Outcome::Unchanged { reason, .. }) => {
                        l.t("unchanged", &[("reason", reason)])
                    }
                    Err(e) => l.t("error", &[("reason", &e.to_string())]),
                };
                toast(&msg);
            }
            refresh();
        });
    });
}

fn use_history(l: &Rc<Localizer>, index: usize) {
    let item = SNAPSHOT.with(|s| s.borrow().history.get(index).cloned());
    let Some(item) = item else { return };
    let paths = user_paths();
    let cfg = effective_config();
    match chirp_core::use_cached(&cfg, &paths, &NativePlatform::new(false), &item) {
        Ok(_) => {
            // Keep the chosen picture for a week instead of replacing it tomorrow.
            let mut user = user_config();
            user.paused_until = model::pause_until(2, &chirp_core::today());
            let _ = user.save(&paths.config_file);
            toast(&l.t("history-used", &[("title", &item.picture.title)]));
        }
        Err(e) => toast(&l.t("error", &[("reason", &e.to_string())])),
    }
    refresh();
}

thread_local! {
    static TOAST_TIMER: slint::Timer = slint::Timer::default();
}

/// Show a short message at the bottom of the window.
pub fn toast(text: &str) {
    with_window(|ui| ui.set_toast(text.into()));
    TOAST_TIMER.with(|t| {
        t.start(slint::TimerMode::SingleShot, Duration::from_secs(5), || {
            with_window(|ui| ui.set_toast(SharedString::default()))
        })
    });
}
