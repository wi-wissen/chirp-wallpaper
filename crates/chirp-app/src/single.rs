//! Only one app per user session. A second start (e.g. a double click while
//! the app runs in the tray) asks the first one to show its window.

use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, OpenEventW, SetEvent, WaitForSingleObject, EVENT_MODIFY_STATE,
    INFINITE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SetForegroundWindow, ShowWindow, SW_SHOW,
};

/// Take the per-session lock; `false` if another instance holds it.
pub fn acquire() -> bool {
    unsafe {
        // The handle is intentionally kept open for the life of the process.
        let created = CreateMutexW(None, true, w!("Local\\ChirpWallpaperApp"));
        created.is_ok() && GetLastError() != ERROR_ALREADY_EXISTS
    }
}

/// Ask the running instance to show its window.
pub fn signal_existing() {
    unsafe {
        if let Ok(event) = OpenEventW(EVENT_MODIFY_STATE, false, w!("Local\\ChirpWallpaperShow")) {
            let _ = SetEvent(event);
        }
    }
}

/// Wait for show requests from later starts.
pub fn listen() {
    let Ok(event) = (unsafe { CreateEventW(None, false, false, w!("Local\\ChirpWallpaperShow")) })
    else {
        return;
    };
    let raw = event.0 as usize;
    std::thread::spawn(move || loop {
        let handle = windows::Win32::Foundation::HANDLE(raw as *mut core::ffi::c_void);
        unsafe { WaitForSingleObject(handle, INFINITE) };
        let _ = slint::invoke_from_event_loop(crate::show_window);
    });
}

/// Give the window keyboard focus after showing it.
pub fn bring_to_front() {
    unsafe {
        if let Ok(hwnd) = FindWindowW(None, w!("Chirp Wallpaper")) {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}
