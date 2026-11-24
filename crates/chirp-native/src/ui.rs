//! Helpers shared by the Windows and the GNOME app: region list, pause
//! options and the texts shown for a picture.

use crate::Localizer;
use chirp_core::{Config, Picture, State};

/// Bing regions with their own pictures, named in their own language.
pub const REGIONS: &[(&str, &str)] = &[
    ("de-DE", "Deutsch (Deutschland)"),
    ("en-AU", "English (Australia)"),
    ("en-CA", "English (Canada)"),
    ("en-GB", "English (United Kingdom)"),
    ("en-IN", "English (India)"),
    ("en-US", "English (United States)"),
    ("es-ES", "Español (España)"),
    ("fr-CA", "Français (Canada)"),
    ("fr-FR", "Français (France)"),
    ("it-IT", "Italiano (Italia)"),
    ("ja-JP", "日本語 (日本)"),
    ("pt-BR", "Português (Brasil)"),
    ("zh-CN", "中文 (中国)"),
];

/// Index in the region list (0 = automatic).
pub fn region_index(market: &str) -> i32 {
    REGIONS
        .iter()
        .position(|(code, _)| code.eq_ignore_ascii_case(market))
        .map(|i| i as i32 + 1)
        .unwrap_or(0)
}

pub fn region_code(index: i32) -> &'static str {
    if index <= 0 {
        "auto"
    } else {
        REGIONS
            .get(index as usize - 1)
            .map(|(c, _)| *c)
            .unwrap_or("auto")
    }
}

/// Pause options: 0 = off, 1 = until tomorrow, 2 = one week, 3 = one month.
pub fn pause_until(index: i32, today: &str) -> Option<String> {
    let days = match index {
        1 => 1,
        2 => 7,
        3 => 30,
        _ => return None,
    };
    let fmt = time::macros::format_description!("[year]-[month]-[day]");
    let date = time::Date::parse(today, fmt).ok()?;
    (date + time::Duration::days(days)).format(fmt).ok()
}

/// Which pause option fits a stored pause date best.
pub fn pause_index(until: Option<&str>, today: &str) -> i32 {
    let Some(until) = until.filter(|u| *u >= today) else {
        return 0;
    };
    let fmt = time::macros::format_description!("[year]-[month]-[day]");
    let (Ok(a), Ok(b)) = (time::Date::parse(today, fmt), time::Date::parse(until, fmt)) else {
        return 0;
    };
    match (b - a).whole_days() {
        ..=1 => 1,
        2..=7 => 2,
        _ => 3,
    }
}

/// "Bing · de-DE · 2026-11-22"
pub fn meta_line(p: &Picture) -> String {
    let source = match p.source.as_str() {
        "bing" => "Bing",
        "custom" => "",
        other => other,
    };
    [source, p.market.as_str(), p.date.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The state as far as this app run has seen it: the result of a check from
/// an earlier run (maybe days ago, another version, before the network was
/// up) is not reported until this run has checked itself.
pub fn session_state(state: &State, checked_this_run: bool) -> State {
    let mut state = state.clone();
    if !checked_this_run {
        state.last_error = None;
        state.offline = false;
    }
    state
}

/// Status line under the picture: (text, is it a problem?).
pub fn status_line(l: &Localizer, cfg: &Config, state: &State, today: &str) -> (String, bool) {
    if cfg.is_paused(today) {
        let until = cfg.paused_until.clone().unwrap_or_default();
        return (l.t("paused", &[("date", &until)]), false);
    }
    if state.offline {
        // Not an error: the apps try again by themselves.
        return (l.t("status-offline", &[]), false);
    }
    if let Some(reason) = &state.last_error {
        return (l.t("unchanged", &[("reason", reason)]), true);
    }
    (String::new(), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_round_trip() {
        assert_eq!(region_index("auto"), 0);
        assert_eq!(region_code(0), "auto");
        let i = region_index("de-DE");
        assert_eq!(region_code(i), "de-DE");
        assert_eq!(region_code(99), "auto");
    }

    #[test]
    fn old_errors_wait_for_a_check_of_this_run() {
        let state = State {
            last_error: Some("network error: dns".into()),
            offline: true,
            ..State::default()
        };
        assert!(session_state(&state, false).last_error.is_none());
        assert!(!session_state(&state, false).offline);
        assert_eq!(session_state(&state, true), state);
    }

    #[test]
    fn pause_dates() {
        assert_eq!(pause_until(0, "2026-11-22"), None);
        assert_eq!(pause_until(1, "2026-11-22").as_deref(), Some("2026-11-23"));
        assert_eq!(pause_until(2, "2026-11-28").as_deref(), Some("2026-12-05"));
        assert_eq!(pause_index(Some("2026-11-29"), "2026-11-22"), 2);
        assert_eq!(pause_index(Some("2026-11-21"), "2026-11-22"), 0);
        assert_eq!(pause_index(None, "2026-11-22"), 0);
    }
}
