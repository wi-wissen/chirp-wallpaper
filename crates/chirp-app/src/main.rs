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

use chirp_core::{Config, Outcome, Paths, Resolution, Screens};
use chirp_native::{platform, Localizer, NativePlatform};
use model::Snapshot;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

slint::include_modules!();

pub const WEBSITE: &str = "https://chirp-wallpaper.wi7.net/";
pub const SOURCE: &str = "https://github.com/wi-wissen/chirp-wallpaper";

thread_local! {
    /// When to look for a new picture while running.
    static SCHEDULE: RefCell<chirp_core::Schedule> =
        RefCell::new(chirp_core::Schedule::new(std::time::SystemTime::now()));
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
    if has("--remove-machine") {
        std::process::exit(machine::remove_machine());
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
    ui.set_machine_setup(machine::is_set_up());
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
    // CHIRP_WINDOW=760x520 opens the window at that size (layout tests).
    if let Some((w, h)) = std::env::var("CHIRP_WINDOW")
        .ok()
        .and_then(|v| {
            v.split_once('x')
                .map(|(w, h)| (w.parse::<f32>(), h.parse::<f32>()))
        })
        .and_then(|(w, h)| Some((w.ok()?, h.ok()?)))
    {
        ui.window().set_size(slint::LogicalSize::new(w, h));
    }
    match std::env::var("CHIRP_CONFIRM").as_deref() {
        Ok("remove") => ui.set_confirm_remove(1),
        Ok(_) => ui.set_confirm_admin(true),
        Err(_) => {}
    }
    refresh();
    if !background {
        ui.show()?;
    }

    // First check shortly after start, then every hour.
    // CHIRP_DEMO=1 shows the stored pictures without loading new ones (screenshots).
    let demo = std::env::var_os("CHIRP_DEMO").is_some();
    let first = slint::Timer::default();
    let hourly = slint::Timer::default();
    if !demo {
        let has_picture = chirp_core::State::load(&user_paths().state_file())
            .current
            .is_some();
        let delay = if has_picture { 3000 } else { 300 };
        first.start(
            slint::TimerMode::SingleShot,
            Duration::from_millis(delay),
            || update(false),
        );
        // Every minute: check when a new day began, after sleep or hibernation,
        // or when the last check is an hour old.
        hourly.start(slint::TimerMode::Repeated, Duration::from_secs(60), || {
            let due = SCHEDULE.with(|s| s.borrow_mut().tick(std::time::SystemTime::now()));
            if due {
                update(false);
            }
        });
    }

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

/// Open the web search about today's picture.
pub fn open_info() {
    SNAPSHOT.with(|s| {
        if let Some(url) = s.borrow().current.as_ref().and_then(|p| p.info_url.clone()) {
            let _ = open::that(url);
        }
    })
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
        set_admin_confirm_uac => "admin-confirm-uac",
        set_admin_confirm_ok => "admin-confirm-ok",
        set_cancel => "cancel",
        set_label_baseline => "label-baseline",
        set_about_text => "about-text",
        set_about_website => "about-website",
        set_about_source => "about-source",
        set_today_loading => "today-loading",
        set_today_waiting => "today-waiting",
        set_today_updating => "today-updating",
        set_history_open => "history-open",
        set_settings_appearance => "settings-appearance",
        set_settings_fit => "settings-fit",
        set_settings_screens => "settings-screens",
        set_settings_caption => "settings-caption",
        set_settings_caption_desc => "settings-caption-desc",
        set_settings_caption_position => "settings-caption-position",
        set_settings_save => "settings-save",
        set_settings_data => "settings-data",
        set_settings_clear => "settings-clear",
        set_settings_clear_desc => "settings-clear-desc",
        set_settings_clear_button => "settings-clear-button",
        set_clear_confirm_title => "clear-confirm-title",
        set_clear_confirm_text => "clear-confirm-text",
        set_settings_unsetup => "settings-unsetup",
        set_settings_unsetup_desc => "settings-unsetup-desc",
        set_unsetup_confirm_title => "unsetup-confirm-title",
        set_unsetup_confirm_text => "unsetup-confirm-text",
        set_remove_button => "remove-button",
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
    let list = |ids: &[&str]| -> ModelRc<SharedString> {
        let items: Vec<SharedString> = ids.iter().map(|id| l.t(id, &[]).into()).collect();
        ModelRc::new(VecModel::from(items))
    };
    ui.set_fits(list(&[
        "fit-fill",
        "fit-fit",
        "fit-stretch",
        "fit-center",
        "fit-span",
    ]));
    ui.set_screen_options(list(&["screens-all", "screens-primary"]));
    ui.set_corners(list(&[
        "pos-bottom-right",
        "pos-bottom-left",
        "pos-top-right",
        "pos-top-left",
    ]));
    let folder = chirp_core::copies_dir().display().to_string();
    tr.set_settings_save_desc(l.t("settings-save-desc", &[("folder", &folder)]).into());
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
    ui.set_fit_index(model::fit_index(cfg.fit));
    ui.set_screens_index(i32::from(cfg.screens == Screens::Primary));
    ui.set_caption(cfg.caption);
    ui.set_corner_index(model::corner_index(cfg.caption_position));
    ui.set_save_copies(cfg.save_copies);
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
    cfg.fit = model::fit_from(ui.get_fit_index());
    cfg.screens = if ui.get_screens_index() == 1 {
        Screens::Primary
    } else {
        Screens::All
    };
    cfg.caption = ui.get_caption();
    cfg.caption_position = model::corner_from(ui.get_corner_index());
    cfg.save_copies = ui.get_save_copies();
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
    ui.on_open_info(open_info);
    {
        let l = l.clone();
        ui.on_use_history(move |index| use_history(&l, index as usize));
    }
    ui.on_open_history(|index| {
        let file = SNAPSHOT.with(|s| {
            s.borrow()
                .history
                .get(index as usize)
                .map(|c| c.image.clone())
        });
        if let Some(file) = file {
            let _ = open::that(file);
        }
    });
    {
        let weak = ui.as_weak();
        ui.on_display_changed(move || {
            if let Some(ui) = weak.upgrade() {
                save_settings(&ui);
                // Show the effect right away on the current picture.
                let cfg = effective_config();
                let paths = user_paths();
                std::thread::spawn(move || {
                    let _ = chirp_core::reapply(&cfg, &paths, &NativePlatform::new(false));
                });
            }
        });
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
    {
        let l = l.clone();
        ui.on_remove_machine(move || machine::start_remove(&l));
    }
    ui.on_clear_data(|| {
        machine::clear_user_data();
        // Nothing left to show; quit so nothing is written again.
        let _ = slint::quit_event_loop();
    });
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
        trace(&format!(
            "snapshot has_picture={} status={status:?} error={is_error}",
            state.current.is_some()
        ));
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
            .map(|(c, thumb)| HistoryItem {
                thumb: thumb
                    .clone()
                    .map(slint::Image::from_rgb8)
                    .unwrap_or_default(),
                title: c.picture.title.clone().into(),
                date: c.picture.date.clone().into(),
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
                history: data.history.into_iter().map(|(c, _)| c).collect(),
            }
        });
    });
}

/// Check for a new picture in the background.
pub fn update(manual: bool) {
    if manual {
        SCHEDULE.with(|s| s.borrow_mut().checked(std::time::SystemTime::now()));
    }
    let mut already_busy = false;
    with_window(|ui| {
        already_busy = ui.get_busy();
        ui.set_busy(true);
    });
    if already_busy {
        return;
    }
    trace(&format!("busy=true manual={manual}"));
    if SNAPSHOT.with(|s| s.borrow().current.is_none()) {
        trace("spinner=loading");
    }
    let paths = user_paths();
    let cfg = effective_config();
    let market = cfg.resolved_market(sys_locale::get_locale().as_deref());
    std::thread::spawn(move || {
        let outcome = chirp_core::apply(
            &cfg,
            &paths,
            &NativePlatform::new(false),
            &market,
            if manual {
                chirp_core::Trigger::Manual
            } else {
                chirp_core::Trigger::Automatic
            },
        );
        let _ = slint::invoke_from_event_loop(move || {
            let l = Rc::new(Localizer::new(None));
            with_window(|ui| ui.set_busy(false));
            trace("busy=false");
            // No connection (e.g. right after sign-in, before the network is up):
            // wait and try again soon instead of showing an error.
            let offline = matches!(outcome, Ok(Outcome::Unchanged { .. }))
                && chirp_core::State::load(&user_paths().state_file()).offline;
            schedule_retry(offline);
            if offline && !manual {
                refresh();
                return;
            }
            // A new picture is always announced, also from the hourly check,
            // so a changing background never comes as a surprise.
            let announce = manual || matches!(outcome, Ok(Outcome::Updated { .. }));
            if announce {
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
            toast(&l.t("history-used", &[("title", &item.picture.title)]));
        }
        Err(e) => toast(&l.t("error", &[("reason", &e.to_string())])),
    }
    refresh();
}

thread_local! {
    static TOAST_TIMER: slint::Timer = slint::Timer::default();
    static RETRY_TIMER: slint::Timer = slint::Timer::default();
    static RETRY_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// After a failed check without connection, try again after 10 s, 20 s,
/// 40 s, 1 min, 2 min and then every 5 min; a successful check stops this.
fn schedule_retry(offline: bool) {
    const DELAYS: [u64; 6] = [10, 20, 40, 60, 120, 300];
    if !offline {
        RETRY_COUNT.with(|c| c.set(0));
        RETRY_TIMER.with(|t| t.stop());
        with_window(|ui| ui.set_waiting(false));
        trace("waiting=false");
        return;
    }
    let n = RETRY_COUNT.with(|c| {
        let n = c.get();
        c.set(n + 1);
        n
    });
    let delay = DELAYS[n.min(DELAYS.len() - 1)];
    with_window(|ui| ui.set_waiting(true));
    trace(&format!("waiting=true retry_in={delay}s"));
    if SNAPSHOT.with(|s| s.borrow().current.is_none()) {
        trace("spinner=waiting");
    }
    RETRY_TIMER.with(|t| {
        t.start(
            slint::TimerMode::SingleShot,
            Duration::from_secs(delay),
            || update(false),
        )
    });
}

/// With CHIRP_TRACE=<file>, write what the window shows, for automated tests.
pub fn trace(event: &str) {
    thread_local! {
        static START: std::time::Instant = std::time::Instant::now();
    }
    let Some(file) = std::env::var_os("CHIRP_TRACE") else {
        return;
    };
    let ms = START.with(|s| s.elapsed().as_millis());
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
    {
        let _ = std::io::Write::write_all(&mut f, format!("{ms} {event}\n").as_bytes());
    }
}

/// Show a short message at the bottom of the window.
pub fn toast(text: &str) {
    trace(&format!("toast={text}"));
    with_window(|ui| ui.set_toast(text.into()));
    TOAST_TIMER.with(|t| {
        t.start(slint::TimerMode::SingleShot, Duration::from_secs(5), || {
            with_window(|ui| ui.set_toast(SharedString::default()))
        })
    });
}
