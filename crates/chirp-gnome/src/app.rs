use adw::prelude::*;
use chirp_core::{history, CachedPicture, Config, Outcome, Paths, Picture, Resolution, State};
use chirp_native::{platform, ui, Localizer, NativePlatform};
use gtk::{gdk, gio, glib};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub const APP_ID: &str = "io.github.wi_wissen.ChirpWallpaper";
const WEBSITE: &str = "https://chirp-wallpaper.wi7.net/";
const SOURCE: &str = "https://github.com/wi-wissen/chirp-wallpaper";

const CSS: &str = "
.chirp-picture { border-radius: 12px; }
.chirp-thumb { border-radius: 8px; }
";

thread_local! {
    static HOLD: RefCell<Option<gio::ApplicationHoldGuard>> = const { RefCell::new(None) };
}

pub fn run() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    let window: Rc<RefCell<Option<Rc<Ui>>>> = Rc::default();

    {
        let window = window.clone();
        app.connect_startup(move |app| {
            let provider = gtk::CssProvider::new();
            provider.load_from_string(CSS);
            if let Some(display) = gdk::Display::default() {
                gtk::style_context_add_provider_for_display(
                    &display,
                    &provider,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }
            let ui = Ui::build(app);
            ui.refresh();
            set_hold(app, autostart_enabled());
            // First check shortly after start, then every hour.
            // CHIRP_DEMO=1 shows the stored pictures without loading new ones (screenshots).
            if std::env::var_os("CHIRP_DEMO").is_none() {
                let first = ui.clone();
                glib::timeout_add_seconds_local_once(3, move || first.update(false));
                let hourly = ui.clone();
                glib::timeout_add_seconds_local(3600, move || {
                    hourly.update(false);
                    glib::ControlFlow::Continue
                });
            }
            *window.borrow_mut() = Some(ui);
        });
    }

    app.connect_command_line(move |_app, cmdline| {
        let background = cmdline.arguments().iter().any(|a| a == "--background");
        if !background {
            if let Some(ui) = window.borrow().as_ref() {
                ui.window.present();
            }
        }
        glib::ExitCode::SUCCESS
    });

    app.run()
}

fn set_hold(app: &adw::Application, hold: bool) {
    HOLD.with(|h| {
        let mut h = h.borrow_mut();
        if hold && h.is_none() {
            *h = Some(app.hold());
        } else if !hold {
            *h = None;
        }
    });
}

fn autostart_enabled() -> bool {
    if crate::portal::in_flatpak() {
        crate::portal::autostart_enabled()
    } else {
        platform::autostart_command(false).is_some_and(|c| c.contains("--background"))
    }
}

/// gsettings outside a Flatpak, the wallpaper portal inside.
fn native_platform() -> Box<dyn chirp_core::Platform> {
    if crate::portal::in_flatpak() {
        Box::new(crate::portal::PortalPlatform)
    } else {
        Box::new(NativePlatform::new(false))
    }
}

fn set_autostart(enable: bool, reason: &str) -> Result<(), String> {
    if crate::portal::in_flatpak() {
        return crate::portal::request_autostart(enable, reason);
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let cmd = platform::command_line(&exe, "--background");
    platform::set_autostart(false, enable.then_some(cmd.as_str())).map_err(|e| e.to_string())
}

fn paths() -> Paths {
    Paths::detect(false)
}

fn user_config() -> Config {
    Config::load(&[paths().config_file]).unwrap_or_default()
}

fn effective_config() -> Config {
    Config::load(&paths().config_files).unwrap_or_default()
}

/// Raw RGB pixels decoded off the main thread.
struct Pixels {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl Pixels {
    fn texture(self) -> gdk::Texture {
        let stride = self.width as usize * 3;
        gdk::MemoryTexture::new(
            self.width as i32,
            self.height as i32,
            gdk::MemoryFormat::R8g8b8,
            &glib::Bytes::from_owned(self.data),
            stride,
        )
        .upcast()
    }
}

fn decode(file: &Path, max: u32) -> Option<Pixels> {
    let img = image::open(file).ok()?;
    let img = if img.width() > max || img.height() > max {
        img.thumbnail(max, max)
    } else {
        img
    };
    let rgb = img.to_rgb8();
    Some(Pixels {
        width: rgb.width(),
        height: rgb.height(),
        data: rgb.into_raw(),
    })
}

struct Loaded {
    state: State,
    picture: Option<Pixels>,
    baseline: Option<Picture>,
    history: Vec<(CachedPicture, Option<Pixels>)>,
}

fn load(paths: &Paths) -> Loaded {
    let state = State::load(&paths.state_file());
    let picture = state.current_file.as_deref().and_then(|f| decode(f, 1600));
    let baseline = chirp_core::apply::baseline(paths).map(|(p, _)| p);
    let history = history(paths)
        .into_iter()
        .take(30)
        .map(|c| {
            let t = decode(&c.thumb, 480);
            (c, t)
        })
        .collect();
    Loaded {
        state,
        picture,
        baseline,
        history,
    }
}

pub struct Ui {
    l: Localizer,
    window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    banner: adw::Banner,
    picture: gtk::Picture,
    meta: gtk::Label,
    title: gtk::Label,
    caption: gtk::Label,
    copyright: gtk::Label,
    description: gtk::Label,
    baseline: gtk::Label,
    update_button: gtk::Button,
    more_button: gtk::Button,
    file_button: gtk::Button,
    history: gtk::FlowBox,
    history_empty: gtk::Label,
    autostart: adw::SwitchRow,
    desktop: adw::SwitchRow,
    region: adw::ComboRow,
    resolution: adw::ComboRow,
    pause: adw::ComboRow,
    current: RefCell<Option<Picture>>,
    current_file: RefCell<Option<PathBuf>>,
    items: RefCell<Vec<CachedPicture>>,
    busy: RefCell<bool>,
    loading: RefCell<bool>,
}

fn label(classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .css_classes(classes.iter().map(|c| c.to_string()).collect::<Vec<_>>())
        .build();
    l.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    l
}

impl Ui {
    fn build(app: &adw::Application) -> Rc<Ui> {
        let l = Localizer::new(None);
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Chirp Wallpaper")
            .icon_name(APP_ID)
            .default_width(920)
            .default_height(720)
            .build();

        let stack = adw::ViewStack::new();
        let header = adw::HeaderBar::new();
        let switcher = adw::ViewSwitcher::builder()
            .stack(&stack)
            .policy(adw::ViewSwitcherPolicy::Wide)
            .build();
        header.set_title_widget(Some(&switcher));
        let menu = gio::Menu::new();
        menu.append(Some(&l.t("nav-about", &[])), Some("app.about"));
        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&menu)
            .primary(true)
            .build();
        header.pack_end(&menu_button);

        let banner = adw::Banner::new("");
        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&stack));
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.add_top_bar(&banner);
        toolbar.set_content(Some(&toasts));
        window.set_content(Some(&toolbar));

        // Today
        let picture = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Cover)
            .height_request(340)
            .css_classes(["chirp-picture", "card"])
            .build();
        picture.set_overflow(gtk::Overflow::Hidden);
        let meta = label(&["dim-label", "caption"]);
        let title = label(&["title-1"]);
        let caption = label(&[]);
        let copyright = label(&["dim-label", "caption"]);
        let description = label(&["body"]);
        for l in [&title, &caption, &copyright, &description] {
            l.set_selectable(true);
        }
        let baseline = label(&["dim-label", "caption"]);
        let update_button = gtk::Button::builder()
            .label(l.t("today-update", &[]))
            .css_classes(["pill", "suggested-action"])
            .build();
        let more_button = gtk::Button::builder()
            .label(l.t("label-more", &[]))
            .css_classes(["pill"])
            .build();
        let file_button = gtk::Button::builder()
            .label(l.t("today-show-file", &[]))
            .css_classes(["pill"])
            .build();
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        for b in [&update_button, &more_button, &file_button] {
            buttons.append(b);
        }
        let today = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(10)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();
        today.append(&picture);
        let gap = gtk::Box::new(gtk::Orientation::Vertical, 0);
        gap.set_height_request(6);
        today.append(&gap);
        for w in [&meta, &title, &caption, &copyright] {
            today.append(w);
        }
        buttons.set_margin_top(8);
        buttons.set_margin_bottom(8);
        today.append(&buttons);
        today.append(&description);
        today.append(&baseline);
        stack.add_titled_with_icon(
            &scrolled(&clamp(&today, 860)),
            Some("today"),
            &l.t("nav-today", &[]),
            "image-x-generic-symbolic",
        );

        // History
        let history = gtk::FlowBox::builder()
            .max_children_per_line(3)
            .min_children_per_line(1)
            .selection_mode(gtk::SelectionMode::None)
            .homogeneous(true)
            .row_spacing(12)
            .column_spacing(12)
            .build();
        let history_empty = gtk::Label::builder()
            .label(l.t("history-empty", &[]))
            .css_classes(["dim-label"])
            .build();
        let history_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();
        history_box.append(&history_empty);
        history_box.append(&history);
        stack.add_titled_with_icon(
            &scrolled(&clamp(&history_box, 1000)),
            Some("history"),
            &l.t("nav-history", &[]),
            "view-grid-symbolic",
        );

        // Settings
        let autostart = adw::SwitchRow::builder()
            .title(l.t("settings-autostart", &[]))
            .subtitle(l.t("settings-autostart-desc-gnome", &[]))
            .build();
        let desktop = adw::SwitchRow::builder()
            .title(l.t("settings-desktop", &[]))
            .build();
        let general = adw::PreferencesGroup::builder()
            .title(l.t("settings-general", &[]))
            .build();
        general.add(&autostart);
        general.add(&desktop);
        let mut region_names = vec![l.t("settings-region-auto", &[])];
        region_names.extend(ui::REGIONS.iter().map(|(_, n)| n.to_string()));
        let region = combo(
            &l.t("settings-region", &[]),
            &l.t("settings-region-desc", &[]),
            &region_names,
        );
        let resolution = combo(
            &l.t("settings-resolution", &[]),
            "",
            &["4K (UHD)".into(), "Full HD (1080p)".into()],
        );
        let pauses: Vec<String> = ["pause-off", "pause-day", "pause-week", "pause-month"]
            .iter()
            .map(|k| l.t(k, &[]))
            .collect();
        let pause = combo(
            &l.t("settings-pause", &[]),
            &l.t("settings-pause-desc", &[]),
            &pauses,
        );
        let picture_group = adw::PreferencesGroup::builder()
            .title(l.t("settings-picture", &[]))
            .build();
        picture_group.add(&region);
        picture_group.add(&resolution);
        picture_group.add(&pause);
        let prefs = adw::PreferencesPage::new();
        prefs.add(&general);
        prefs.add(&picture_group);
        stack.add_titled_with_icon(
            &prefs,
            Some("settings"),
            &l.t("nav-settings", &[]),
            "emblem-system-symbolic",
        );

        let ui = Rc::new(Ui {
            l,
            window,
            toasts,
            banner,
            picture,
            meta,
            title,
            caption,
            copyright,
            description,
            baseline,
            update_button,
            more_button,
            file_button,
            history,
            history_empty,
            autostart,
            desktop,
            region,
            resolution,
            pause,
            current: RefCell::default(),
            current_file: RefCell::default(),
            items: RefCell::default(),
            busy: RefCell::new(false),
            loading: RefCell::new(false),
        });
        ui.load_settings();
        ui.connect(app);
        // Start with the main button focused, not inside a selectable text.
        GtkWindowExt::set_focus(&ui.window, Some(&ui.update_button));
        ui
    }

    fn connect(self: &Rc<Self>, app: &adw::Application) {
        let about = gio::SimpleAction::new("about", None);
        {
            let ui = self.clone();
            about.connect_activate(move |_, _| ui.show_about());
        }
        app.add_action(&about);

        {
            let ui = self.clone();
            let app = app.clone();
            self.window.connect_close_request(move |w| {
                if ui.autostart.is_active() {
                    // Keep running in the background; starting the app again shows the window.
                    w.set_visible(false);
                } else {
                    app.quit();
                }
                glib::Propagation::Stop
            });
        }
        {
            let ui = self.clone();
            self.update_button.connect_clicked(move |_| ui.update(true));
        }
        {
            let ui = self.clone();
            self.more_button.connect_clicked(move |_| {
                if let Some(url) = ui
                    .current
                    .borrow()
                    .as_ref()
                    .and_then(|p| p.info_url.clone())
                {
                    gtk::UriLauncher::new(&url).launch(
                        Some(&ui.window),
                        gio::Cancellable::NONE,
                        |_| {},
                    );
                }
            });
        }
        {
            let ui = self.clone();
            self.file_button.connect_clicked(move |_| {
                if let Some(f) = ui.current_file.borrow().as_ref() {
                    gtk::FileLauncher::new(Some(&gio::File::for_path(f))).open_containing_folder(
                        Some(&ui.window),
                        gio::Cancellable::NONE,
                        |_| {},
                    );
                }
            });
        }
        {
            let ui = self.clone();
            let app = app.clone();
            self.autostart.connect_active_notify(move |row| {
                if *ui.loading.borrow() {
                    return;
                }
                let reason = ui.l.t("settings-autostart-desc-gnome", &[]);
                let active = row.is_active();
                let ui2 = ui.clone();
                let app2 = app.clone();
                glib::spawn_future_local(async move {
                    let result = gio::spawn_blocking(move || set_autostart(active, &reason)).await;
                    if let Ok(Err(e)) = result {
                        ui2.toast(&ui2.l.t("error", &[("reason", &e)]));
                    }
                    set_hold(&app2, autostart_enabled());
                });
            });
        }
        {
            let ui = self.clone();
            self.desktop
                .connect_active_notify(move |_| ui.save_settings());
        }
        for row in [&self.region, &self.resolution] {
            let ui = self.clone();
            row.connect_selected_notify(move |_| ui.save_settings());
        }
        {
            let ui = self.clone();
            self.pause.connect_selected_notify(move |row| {
                if *ui.loading.borrow() {
                    return;
                }
                let mut cfg = user_config();
                cfg.paused_until = ui::pause_until(row.selected() as i32, &chirp_core::today());
                let _ = cfg.save(&paths().config_file);
                ui.refresh();
            });
        }
    }

    fn load_settings(&self) {
        *self.loading.borrow_mut() = true;
        let cfg = effective_config();
        self.autostart.set_active(autostart_enabled());
        self.desktop.set_active(cfg.desktop);
        self.region
            .set_selected(ui::region_index(&cfg.market) as u32);
        self.resolution.set_selected(match cfg.resolution {
            Resolution::Uhd => 0,
            Resolution::FullHd => 1,
        });
        self.pause.set_selected(
            ui::pause_index(cfg.paused_until.as_deref(), &chirp_core::today()) as u32,
        );
        *self.loading.borrow_mut() = false;
    }

    fn save_settings(&self) {
        if *self.loading.borrow() {
            return;
        }
        let mut cfg = user_config();
        cfg.desktop = self.desktop.is_active();
        cfg.market = ui::region_code(self.region.selected() as i32).to_string();
        cfg.resolution = if self.resolution.selected() == 1 {
            Resolution::FullHd
        } else {
            Resolution::Uhd
        };
        let _ = cfg.save(&paths().config_file);
    }

    fn toast(&self, text: &str) {
        self.toasts.add_toast(adw::Toast::new(text));
    }

    fn show_about(&self) {
        let about = adw::AboutDialog::builder()
            .application_name("Chirp Wallpaper")
            .application_icon(APP_ID)
            .version(env!("CARGO_PKG_VERSION"))
            .comments(self.l.t("about-text", &[]))
            .website(WEBSITE)
            .issue_url(format!("{SOURCE}/issues"))
            .license_type(gtk::License::MitX11)
            .build();
        about.add_link(&self.l.t("about-source", &[]), SOURCE);
        about.present(Some(&self.window));
    }

    /// Load state and pictures in a worker thread, then show them.
    fn refresh(self: &Rc<Self>) {
        let ui = self.clone();
        glib::spawn_future_local(async move {
            let Ok(data) = gio::spawn_blocking(|| load(&paths())).await else {
                return;
            };
            ui.show(data);
        });
    }

    fn show(self: &Rc<Self>, data: Loaded) {
        let cfg = effective_config();
        let state = &data.state;
        let has = state.current.is_some();
        match &state.current {
            Some(p) => {
                self.meta.set_text(&ui::meta_line(p));
                self.title.set_text(&p.title);
                self.caption
                    .set_text(if p.caption != p.title { &p.caption } else { "" });
                self.copyright.set_text(&p.copyright);
                self.description
                    .set_text(p.description.as_deref().unwrap_or_default());
                self.more_button.set_visible(p.info_url.is_some());
            }
            None => {
                self.meta.set_text("");
                self.title.set_text(&self.l.t("today-empty", &[]));
                self.caption.set_text("");
                self.copyright.set_text("");
                self.description.set_text("");
                self.more_button.set_visible(false);
            }
        }
        for w in [
            &self.caption,
            &self.copyright,
            &self.description,
            &self.meta,
        ] {
            w.set_visible(!w.text().is_empty());
        }
        self.file_button.set_visible(has);
        self.picture.set_visible(data.picture.is_some());
        if let Some(px) = data.picture {
            self.picture.set_paintable(Some(&px.texture()));
        }
        let (status, problem) = ui::status_line(&self.l, &cfg, state, &chirp_core::today());
        self.banner.set_title(&status);
        self.banner.set_revealed(!status.is_empty());
        if problem {
            self.banner.add_css_class("warning");
        } else {
            self.banner.remove_css_class("warning");
        }
        let base = data
            .baseline
            .as_ref()
            .map(|p| {
                format!(
                    "{}: {} ({})",
                    self.l.t("label-baseline", &[]),
                    p.title,
                    p.date
                )
            })
            .unwrap_or_default();
        self.baseline.set_visible(!base.is_empty());
        self.baseline.set_text(&base);

        self.history.remove_all();
        self.history_empty.set_visible(data.history.is_empty());
        let mut items = Vec::new();
        for (i, (item, thumb)) in data.history.into_iter().enumerate() {
            self.history.append(&self.history_tile(i, &item, thumb));
            items.push(item);
        }
        *self.items.borrow_mut() = items;
        *self.current.borrow_mut() = state.current.clone();
        *self.current_file.borrow_mut() = state.current_file.clone();
        *self.loading.borrow_mut() = true;
        self.pause.set_selected(
            ui::pause_index(cfg.paused_until.as_deref(), &chirp_core::today()) as u32,
        );
        *self.loading.borrow_mut() = false;
    }

    fn history_tile(
        self: &Rc<Self>,
        index: usize,
        item: &CachedPicture,
        thumb: Option<Pixels>,
    ) -> gtk::Widget {
        let pic = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Cover)
            .height_request(150)
            .css_classes(["chirp-thumb"])
            .build();
        pic.set_overflow(gtk::Overflow::Hidden);
        if let Some(t) = thumb {
            pic.set_paintable(Some(&t.texture()));
        }
        let title = gtk::Label::builder()
            .label(&item.picture.title)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        let date = gtk::Label::builder()
            .label(&item.picture.date)
            .xalign(0.0)
            .css_classes(["dim-label", "caption"])
            .build();
        let use_button = gtk::Button::builder()
            .label(self.l.t("history-use", &[]))
            .valign(gtk::Align::Center)
            .build();
        {
            let ui = self.clone();
            use_button.connect_clicked(move |_| ui.use_history(index));
        }
        let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
        texts.set_hexpand(true);
        texts.append(&title);
        texts.append(&date);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.set_margin_start(12);
        row.set_margin_end(8);
        row.set_margin_top(8);
        row.set_margin_bottom(8);
        row.append(&texts);
        row.append(&use_button);
        let card = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["card"])
            .overflow(gtk::Overflow::Hidden)
            .build();
        card.append(&pic);
        card.append(&row);
        card.upcast()
    }

    fn use_history(self: &Rc<Self>, index: usize) {
        let Some(item) = self.items.borrow().get(index).cloned() else {
            return;
        };
        let cfg = effective_config();
        match chirp_core::use_cached(&cfg, &paths(), native_platform().as_ref(), &item) {
            Ok(_) => {
                let mut user = user_config();
                user.paused_until = ui::pause_until(2, &chirp_core::today());
                let _ = user.save(&paths().config_file);
                self.toast(&self.l.t("history-used", &[("title", &item.picture.title)]));
            }
            Err(e) => self.toast(&self.l.t("error", &[("reason", &e.to_string())])),
        }
        self.refresh();
    }

    /// Check for a new picture in a worker thread.
    fn update(self: &Rc<Self>, manual: bool) {
        if *self.busy.borrow() {
            return;
        }
        *self.busy.borrow_mut() = true;
        self.update_button.set_sensitive(false);
        let ui = self.clone();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(|| {
                let cfg = effective_config();
                let market = cfg.resolved_market(sys_locale::get_locale().as_deref());
                chirp_core::apply(&cfg, &paths(), native_platform().as_ref(), &market)
                    .map_err(|e| e.to_string())
            })
            .await;
            *ui.busy.borrow_mut() = false;
            ui.update_button.set_sensitive(true);
            if manual {
                let l = &ui.l;
                let msg = match result {
                    Ok(Ok(Outcome::Updated { picture, .. })) => {
                        l.t("updated", &[("title", &picture.title)])
                    }
                    Ok(Ok(Outcome::AlreadyCurrent { picture, .. })) => {
                        l.t("already-current", &[("title", &picture.title)])
                    }
                    Ok(Ok(Outcome::Paused { until })) => l.t("paused", &[("date", &until)]),
                    Ok(Ok(Outcome::Unchanged { reason, .. })) => {
                        l.t("unchanged", &[("reason", &reason)])
                    }
                    Ok(Err(reason)) => l.t("error", &[("reason", &reason)]),
                    Err(_) => l.t("error", &[("reason", "internal")]),
                };
                ui.toast(&msg);
            }
            ui.refresh();
        });
    }
}

fn clamp(child: &impl IsA<gtk::Widget>, width: i32) -> adw::Clamp {
    adw::Clamp::builder()
        .maximum_size(width)
        .child(child)
        .build()
}

fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(child)
        .build()
}

fn combo(title: &str, subtitle: &str, items: &[String]) -> adw::ComboRow {
    let refs: Vec<&str> = items.iter().map(String::as_str).collect();
    let row = adw::ComboRow::builder()
        .title(title)
        .model(&gtk::StringList::new(&refs))
        .build();
    if !subtitle.is_empty() {
        row.set_subtitle(subtitle);
    }
    row
}
