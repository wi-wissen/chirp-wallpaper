//! Icon in the notification area: click opens the window, the menu offers
//! "Open", "Update now" and "Quit".

use chirp_native::Localizer;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

const ICON_PNG: &[u8] = include_bytes!("../../../assets/icon-32.png");

pub fn create(l: &Localizer) -> Option<TrayIcon> {
    let menu = Menu::new();
    let open = MenuItem::with_id("open", l.t("tray-open", &[]), true, None);
    let update = MenuItem::with_id("update", l.t("today-update", &[]), true, None);
    let more = MenuItem::with_id("more", l.t("label-more", &[]), true, None);
    let quit = MenuItem::with_id("quit", l.t("tray-quit", &[]), true, None);
    menu.append_items(&[
        &open,
        &more,
        &update,
        &PredefinedMenuItem::separator(),
        &quit,
    ])
    .ok()?;

    MenuEvent::set_event_handler(Some(|event: MenuEvent| {
        let id = event.id.0.clone();
        let _ = slint::invoke_from_event_loop(move || match id.as_str() {
            "open" => crate::show_window(),
            "update" => crate::update(true),
            "more" => crate::open_info(),
            "quit" => {
                let _ = slint::quit_event_loop();
            }
            _ => {}
        });
    }));
    TrayIconEvent::set_event_handler(Some(|event: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            let _ = slint::invoke_from_event_loop(crate::show_window);
        }
    }));

    TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip("Chirp Wallpaper")
        .with_icon(icon()?)
        .build()
        .ok()
}

fn icon() -> Option<tray_icon::Icon> {
    let img = image::load_from_memory(ICON_PNG).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    tray_icon::Icon::from_rgba(img.into_raw(), w, h).ok()
}
