//! One update run: fetch, download, set, remember.
//!
//! When no new picture can be loaded, nothing on screen is changed: the
//! picture already set stays. On a reset machine that is the baseline picture
//! saved while the machine was prepared. The reason is stored in the state, so
//! the front ends can tell the user what happened.

use crate::config::{Config, Resolution};
use crate::paths::Paths;
use crate::picture::Picture;
use crate::state::{read_picture, write_json, State};
use crate::{Error, Result};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// What a platform must be able to do.
pub trait Platform {
    fn set_desktop(&self, image: &Path) -> Result<()>;
    fn set_lockscreen(&self, image: &Path) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Desktop,
    LockScreen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// A new picture was set.
    Updated {
        picture: Picture,
        warnings: Vec<(Target, String)>,
    },
    /// Today's picture was already in use; it was set again.
    AlreadyCurrent {
        picture: Picture,
        warnings: Vec<(Target, String)>,
    },
    /// Updates are paused.
    Paused { until: String },
    /// No new picture could be loaded; the screen was left as it is.
    Unchanged {
        reason: String,
        kept: Option<Picture>,
    },
}

/// Fetch function: `(market, resolution) -> picture metadata`.
pub type FetchFn<'a> = &'a dyn Fn(&str, Resolution) -> Result<Picture>;
/// Download function: `(url, destination file)`.
pub type DownloadFn<'a> = &'a dyn Fn(&str, &Path) -> Result<()>;

/// Run one update with Bing and real downloads.
pub fn apply(
    cfg: &Config,
    paths: &Paths,
    platform: &dyn Platform,
    market: &str,
) -> Result<Outcome> {
    let agent = crate::bing::agent();
    let download = |url: &str, dest: &Path| -> Result<()> {
        let mut file = std::fs::File::create(dest)?;
        crate::bing::download(&agent, url, &mut file).map(|_| ())
    };
    apply_with(
        cfg,
        paths,
        platform,
        market,
        &crate::today(),
        &crate::bing::fetch_latest,
        &download,
    )
}

/// Run one update with injected network functions (used by tests).
pub fn apply_with(
    cfg: &Config,
    paths: &Paths,
    platform: &dyn Platform,
    market: &str,
    today: &str,
    fetch: FetchFn,
    download: DownloadFn,
) -> Result<Outcome> {
    let mut state = State::load(&paths.state_file());

    if cfg.is_paused(today) {
        return Ok(Outcome::Paused {
            until: cfg.paused_until.clone().unwrap_or_default(),
        });
    }

    state.last_attempt = Some(crate::now_rfc3339());

    let picture = match fetch(market, cfg.resolution) {
        Ok(p) => p,
        Err(e) => return unchanged(cfg, paths, platform, &mut state, e),
    };

    let already = state.current.as_ref().is_some_and(|c| c.id == picture.id);
    let file = match ensure_image(paths, &picture, download) {
        Ok(f) => f,
        Err(e) => return unchanged(cfg, paths, platform, &mut state, e),
    };
    write_json(&paths.meta_file(&picture), &picture)?;
    fetch_thumb(paths, &picture, download);

    let warnings = set_screens(cfg, platform, &file);
    let enabled = usize::from(cfg.desktop) + usize::from(cfg.lockscreen);
    if enabled > 0 && warnings.len() == enabled {
        // Nothing could be set at all: report it like a failed update.
        let reason = warnings[0].1.clone();
        return unchanged(cfg, paths, platform, &mut state, Error::Platform(reason));
    }

    state.current = Some(picture.clone());
    state.current_file = Some(file.clone());
    state.last_success = state.last_attempt.clone();
    state.last_error = None;
    state.save(&paths.state_file())?;
    prune(paths, cfg.keep_days, &file);

    Ok(if already {
        Outcome::AlreadyCurrent { picture, warnings }
    } else {
        Outcome::Updated { picture, warnings }
    })
}

/// No new picture: leave the screen as it is. Only an account that has never
/// had a picture (e.g. a new or reset profile that starts offline) gets the
/// newest picture already on this computer, so it does not stay on the
/// system default.
fn unchanged(
    cfg: &Config,
    paths: &Paths,
    platform: &dyn Platform,
    state: &mut State,
    err: Error,
) -> Result<Outcome> {
    let reason = err.to_string();
    state.last_error = Some(reason.clone());
    if state.current.is_none() {
        if let Some((picture, file)) = fallback(paths) {
            let warnings = set_screens(cfg, platform, &file);
            let enabled = usize::from(cfg.desktop) + usize::from(cfg.lockscreen);
            if enabled == 0 || warnings.len() < enabled {
                state.current = Some(picture);
                state.current_file = Some(file);
            }
        }
    }
    state.save(&paths.state_file())?;
    let kept = state
        .current
        .clone()
        .or_else(|| baseline(paths).map(|(p, _)| p));
    Ok(Outcome::Unchanged { reason, kept })
}

/// The newest picture available without internet: the computer's shared
/// picture (downloaded by the start-up task or kept in the frozen image),
/// otherwise the baseline.
fn fallback(paths: &Paths) -> Option<(Picture, PathBuf)> {
    if paths.machine_data_dir != paths.data_dir {
        let machine = State::load(&paths.machine_data_dir.join("state.json"));
        if let (Some(p), Some(f)) = (machine.current, machine.current_file) {
            if is_usable(&f) {
                return Some((p, f));
            }
        }
    }
    baseline(paths)
}

fn set_screens(cfg: &Config, platform: &dyn Platform, file: &Path) -> Vec<(Target, String)> {
    let mut warnings = Vec::new();
    if cfg.desktop {
        if let Err(e) = platform.set_desktop(file) {
            warnings.push((Target::Desktop, e.to_string()));
        }
    }
    if cfg.lockscreen {
        if let Err(e) = platform.set_lockscreen(file) {
            warnings.push((Target::LockScreen, e.to_string()));
        }
    }
    warnings
}

/// Make sure the picture is on disk: reuse own or machine cache, else download.
fn ensure_image(paths: &Paths, picture: &Picture, download: DownloadFn) -> Result<PathBuf> {
    let file = paths.image_file(picture);
    if is_usable(&file) {
        return Ok(file);
    }
    std::fs::create_dir_all(paths.images_dir())?;
    let shared = paths.shared_image_file(picture);
    if shared != file && is_usable(&shared) {
        std::fs::copy(&shared, &file)?;
        return Ok(file);
    }
    let part = file.with_extension("jpg.part");
    let mut result = download(&picture.image_url, &part);
    if result.is_err() {
        if let Some(fallback) = &picture.fallback_url {
            result = download(fallback, &part);
        }
    }
    if let Err(e) = result {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    if !looks_like_image(&part) {
        let _ = std::fs::remove_file(&part);
        return Err(Error::Format("downloaded file is not a picture".into()));
    }
    std::fs::rename(&part, &file)?;
    Ok(file)
}

/// Download the small preview; failures only mean the gallery shows the big picture.
fn fetch_thumb(paths: &Paths, picture: &Picture, download: DownloadFn) {
    let Some(url) = &picture.thumb_url else {
        return;
    };
    let file = paths.thumb_file(picture);
    if is_usable(&file) {
        return;
    }
    let part = file.with_extension("part");
    if download(url, &part).is_ok() && looks_like_image(&part) {
        let _ = std::fs::rename(&part, &file);
    } else {
        let _ = std::fs::remove_file(&part);
    }
}

/// One picture in the local history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedPicture {
    pub picture: Picture,
    pub image: PathBuf,
    /// Small preview if available, otherwise the image itself.
    pub thumb: PathBuf,
}

/// Pictures in the cache, newest first.
pub fn history(paths: &Paths) -> Vec<CachedPicture> {
    let Ok(entries) = std::fs::read_dir(paths.images_dir()) else {
        return Vec::new();
    };
    let mut list: Vec<CachedPicture> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter_map(|meta| {
            let picture = read_picture(&meta)?;
            let image = paths.image_file(&picture);
            if !is_usable(&image) {
                return None;
            }
            let thumb = paths.thumb_file(&picture);
            let thumb = if is_usable(&thumb) {
                thumb
            } else {
                image.clone()
            };
            Some(CachedPicture {
                picture,
                image,
                thumb,
            })
        })
        .collect();
    list.sort_by(|a, b| {
        b.picture
            .date
            .cmp(&a.picture.date)
            .then(a.picture.id.cmp(&b.picture.id))
    });
    list
}

/// Set a picture from the history and remember it as current.
pub fn use_cached(
    cfg: &Config,
    paths: &Paths,
    platform: &dyn Platform,
    item: &CachedPicture,
) -> Result<Vec<(Target, String)>> {
    let warnings = set_screens(cfg, platform, &item.image);
    let enabled = usize::from(cfg.desktop) + usize::from(cfg.lockscreen);
    if enabled > 0 && warnings.len() == enabled {
        return Err(Error::Platform(warnings[0].1.clone()));
    }
    let mut state = State::load(&paths.state_file());
    state.current = Some(item.picture.clone());
    state.current_file = Some(item.image.clone());
    state.save(&paths.state_file())?;
    Ok(warnings)
}

fn is_usable(file: &Path) -> bool {
    std::fs::metadata(file).is_ok_and(|m| m.len() > 0)
}

/// JPEG, PNG or WebP magic bytes.
fn looks_like_image(file: &Path) -> bool {
    let mut head = [0u8; 12];
    let Ok(mut f) = std::fs::File::open(file) else {
        return false;
    };
    if std::io::Read::read_exact(&mut f, &mut head).is_err() {
        return false;
    }
    head.starts_with(&[0xFF, 0xD8, 0xFF])
        || head.starts_with(&[0x89, b'P', b'N', b'G'])
        || (head.starts_with(b"RIFF") && &head[8..12] == b"WEBP")
}

/// Delete cached pictures older than `keep_days`, never the current one.
fn prune(paths: &Paths, keep_days: u32, current: &Path) {
    let Ok(entries) = std::fs::read_dir(paths.images_dir()) else {
        return;
    };
    let limit = Duration::from_secs(u64::from(keep_days.max(1)) * 86_400);
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if !name.ends_with(".jpg") || name.ends_with(".thumb.jpg") || path == current {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > limit);
        if old {
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(path.with_extension("json"));
            let _ = std::fs::remove_file(path.with_extension("thumb.jpg"));
        }
    }
}

/// The baseline picture and its file, if one was saved.
pub fn baseline(paths: &Paths) -> Option<(Picture, PathBuf)> {
    paths
        .baseline_candidates()
        .into_iter()
        .find_map(|(img, meta)| {
            let pic = read_picture(&meta)?;
            is_usable(&img).then_some((pic, img))
        })
}

/// Save the current picture as baseline ("seal" while preparing a machine).
pub fn seal(paths: &Paths) -> Result<Picture> {
    let state = State::load(&paths.state_file());
    let (Some(picture), Some(file)) = (state.current, state.current_file) else {
        return Err(Error::Config("no picture has been set yet".into()));
    };
    std::fs::create_dir_all(&paths.data_dir)?;
    std::fs::copy(&file, paths.baseline_image())?;
    write_json(&paths.baseline_meta(), &picture)?;
    Ok(picture)
}

/// Use a picture of one's own as baseline (e.g. a school logo).
pub fn seal_custom(paths: &Paths, image: &Path, title: &str) -> Result<Picture> {
    if !looks_like_image(image) {
        return Err(Error::Format(format!(
            "{} is not a JPEG, PNG or WebP picture",
            image.display()
        )));
    }
    std::fs::create_dir_all(&paths.data_dir)?;
    std::fs::copy(image, paths.baseline_image())?;
    let picture = Picture {
        id: format!("custom-{}", crate::today()),
        source: "custom".into(),
        market: String::new(),
        date: crate::today(),
        title: title.into(),
        caption: String::new(),
        copyright: String::new(),
        description: None,
        info_url: None,
        image_url: String::new(),
        fallback_url: None,
        thumb_url: None,
        note: None,
    };
    write_json(&paths.baseline_meta(), &picture)?;
    Ok(picture)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const JPEG: &[u8] = &[
        0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F', 0, 1, 1,
    ];

    #[derive(Default)]
    struct FakePlatform {
        desktop: RefCell<Vec<PathBuf>>,
        lock: RefCell<Vec<PathBuf>>,
        fail_lock: bool,
    }

    impl Platform for FakePlatform {
        fn set_desktop(&self, image: &Path) -> Result<()> {
            self.desktop.borrow_mut().push(image.into());
            Ok(())
        }
        fn set_lockscreen(&self, image: &Path) -> Result<()> {
            if self.fail_lock {
                return Err(Error::Platform("lock screen blocked by policy".into()));
            }
            self.lock.borrow_mut().push(image.into());
            Ok(())
        }
    }

    fn sample(date: &str) -> Picture {
        Picture {
            id: format!("bing-de-DE-{date}"),
            source: "bing".into(),
            market: "de-DE".into(),
            date: date.into(),
            title: "Möwen über den Hügeln".into(),
            caption: "Hügel".into(),
            copyright: "© Beispiel".into(),
            description: None,
            info_url: None,
            image_url: "https://example.invalid/uhd.jpg".into(),
            fallback_url: Some("https://example.invalid/fhd.jpg".into()),
            thumb_url: None,
            note: None,
        }
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("chirp-apply-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_jpeg(_url: &str, dest: &Path) -> Result<()> {
        std::fs::write(dest, JPEG)?;
        Ok(())
    }

    #[test]
    fn sets_new_picture_and_remembers_it() {
        let dir = temp("new");
        let paths = Paths::in_dir(&dir);
        let platform = FakePlatform::default();
        let fetch = |_: &str, _: Resolution| Ok(sample("2026-11-22"));
        let out = apply_with(
            &Config::default(),
            &paths,
            &platform,
            "de-DE",
            "2026-11-22",
            &fetch,
            &write_jpeg,
        )
        .unwrap();
        assert!(matches!(out, Outcome::Updated { .. }));
        assert_eq!(platform.desktop.borrow().len(), 1);
        assert_eq!(platform.lock.borrow().len(), 1);
        let state = State::load(&paths.state_file());
        assert_eq!(state.current.unwrap().id, "bing-de-DE-2026-11-22");
        assert!(state.last_error.is_none());
        assert!(paths.meta_file(&sample("2026-11-22")).is_file());

        let again = apply_with(
            &Config::default(),
            &paths,
            &platform,
            "de-DE",
            "2026-11-22",
            &fetch,
            &write_jpeg,
        )
        .unwrap();
        assert!(matches!(again, Outcome::AlreadyCurrent { .. }));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn offline_keeps_current_picture() {
        let dir = temp("offline");
        let paths = Paths::in_dir(&dir);
        let platform = FakePlatform::default();
        let ok = |_: &str, _: Resolution| Ok(sample("2026-11-21"));
        apply_with(
            &Config::default(),
            &paths,
            &platform,
            "de-DE",
            "2026-11-21",
            &ok,
            &write_jpeg,
        )
        .unwrap();

        let offline = |_: &str, _: Resolution| Err(Error::Network("dns error".into()));
        let out = apply_with(
            &Config::default(),
            &paths,
            &platform,
            "de-DE",
            "2026-11-22",
            &offline,
            &write_jpeg,
        )
        .unwrap();
        match out {
            Outcome::Unchanged { reason, kept } => {
                assert!(reason.contains("dns error"));
                assert_eq!(kept.unwrap().date, "2026-11-21");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(platform.desktop.borrow().len(), 1, "screen must not change");
        let state = State::load(&paths.state_file());
        assert!(state.last_error.is_some());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn offline_on_fresh_machine_reports_baseline() {
        let dir = temp("baseline");
        let paths = Paths::in_dir(&dir);
        std::fs::create_dir_all(&paths.machine_data_dir).unwrap();
        std::fs::write(paths.machine_data_dir.join("baseline.jpg"), JPEG).unwrap();
        write_json(
            &paths.machine_data_dir.join("baseline.json"),
            &sample("2026-09-01"),
        )
        .unwrap();
        let offline = |_: &str, _: Resolution| Err(Error::Network("timeout".into()));
        let out = apply_with(
            &Config::default(),
            &paths,
            &FakePlatform::default(),
            "de-DE",
            "2026-11-22",
            &offline,
            &write_jpeg,
        )
        .unwrap();
        match out {
            Outcome::Unchanged { kept, .. } => assert_eq!(kept.unwrap().date, "2026-09-01"),
            other => panic!("unexpected {other:?}"),
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn new_account_offline_gets_the_computers_picture() {
        let dir = temp("freshuser");
        let paths = Paths::in_dir(&dir);
        // The start-up task (machine scope) downloaded a picture earlier.
        let machine_img = paths.machine_data_dir.join("images/m.jpg");
        std::fs::create_dir_all(machine_img.parent().unwrap()).unwrap();
        std::fs::write(&machine_img, JPEG).unwrap();
        let machine_state = State {
            current: Some(sample("2026-11-20")),
            current_file: Some(machine_img.clone()),
            ..State::default()
        };
        machine_state
            .save(&paths.machine_data_dir.join("state.json"))
            .unwrap();

        let platform = FakePlatform::default();
        let offline = |_: &str, _: Resolution| Err(Error::Network("offline".into()));
        let out = apply_with(
            &Config::default(),
            &paths,
            &platform,
            "de-DE",
            "2026-11-22",
            &offline,
            &write_jpeg,
        )
        .unwrap();
        match out {
            Outcome::Unchanged { kept, .. } => assert_eq!(kept.unwrap().date, "2026-11-20"),
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(platform.desktop.borrow().as_slice(), [machine_img]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn falls_back_to_full_hd() {
        let dir = temp("fallback");
        let paths = Paths::in_dir(&dir);
        let fetch = |_: &str, _: Resolution| Ok(sample("2026-11-22"));
        let tried = RefCell::new(Vec::new());
        let dl = |url: &str, dest: &Path| -> Result<()> {
            tried.borrow_mut().push(url.to_string());
            if url.contains("uhd") {
                return Err(Error::Network("HTTP 404".into()));
            }
            write_jpeg(url, dest)
        };
        let out = apply_with(
            &Config::default(),
            &paths,
            &FakePlatform::default(),
            "de-DE",
            "2026-11-22",
            &fetch,
            &dl,
        )
        .unwrap();
        assert!(matches!(out, Outcome::Updated { .. }));
        assert_eq!(tried.borrow().len(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_non_images() {
        let dir = temp("html");
        let paths = Paths::in_dir(&dir);
        let fetch = |_: &str, _: Resolution| Ok(sample("2026-11-22"));
        let html = |_: &str, dest: &Path| -> Result<()> {
            std::fs::write(dest, b"<html>captive portal</html>")?;
            Ok(())
        };
        let out = apply_with(
            &Config::default(),
            &paths,
            &FakePlatform::default(),
            "de-DE",
            "2026-11-22",
            &fetch,
            &html,
        )
        .unwrap();
        assert!(matches!(out, Outcome::Unchanged { .. }));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn uses_machine_cache_without_download() {
        let dir = temp("shared");
        let paths = Paths::in_dir(&dir);
        let pic = sample("2026-11-22");
        let shared = paths.shared_image_file(&pic);
        std::fs::create_dir_all(shared.parent().unwrap()).unwrap();
        std::fs::write(&shared, JPEG).unwrap();
        let fetch = |_: &str, _: Resolution| Ok(sample("2026-11-22"));
        let no_dl = |_: &str, _: &Path| -> Result<()> { panic!("must not download") };
        let out = apply_with(
            &Config::default(),
            &paths,
            &FakePlatform::default(),
            "de-DE",
            "2026-11-22",
            &fetch,
            &no_dl,
        )
        .unwrap();
        assert!(matches!(out, Outcome::Updated { .. }));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn lock_screen_failure_is_a_warning() {
        let dir = temp("lockfail");
        let paths = Paths::in_dir(&dir);
        let platform = FakePlatform {
            fail_lock: true,
            ..Default::default()
        };
        let fetch = |_: &str, _: Resolution| Ok(sample("2026-11-22"));
        let out = apply_with(
            &Config::default(),
            &paths,
            &platform,
            "de-DE",
            "2026-11-22",
            &fetch,
            &write_jpeg,
        )
        .unwrap();
        match out {
            Outcome::Updated { warnings, .. } => assert_eq!(warnings[0].0, Target::LockScreen),
            other => panic!("unexpected {other:?}"),
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn paused_does_nothing() {
        let dir = temp("paused");
        let paths = Paths::in_dir(&dir);
        let cfg = Config {
            paused_until: Some("2026-12-01".into()),
            ..Config::default()
        };
        let fetch = |_: &str, _: Resolution| -> Result<Picture> { panic!("must not fetch") };
        let out = apply_with(
            &cfg,
            &paths,
            &FakePlatform::default(),
            "de-DE",
            "2026-11-22",
            &fetch,
            &write_jpeg,
        )
        .unwrap();
        assert_eq!(
            out,
            Outcome::Paused {
                until: "2026-12-01".into()
            }
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn history_lists_cached_pictures_newest_first() {
        let dir = temp("history");
        let paths = Paths::in_dir(&dir);
        for date in ["2026-11-20", "2026-11-22", "2026-11-21"] {
            let fetch = move |_: &str, _: Resolution| {
                let mut p = sample(date);
                p.thumb_url = Some("https://example.invalid/thumb.jpg".into());
                Ok(p)
            };
            apply_with(
                &Config::default(),
                &paths,
                &FakePlatform::default(),
                "de-DE",
                date,
                &fetch,
                &write_jpeg,
            )
            .unwrap();
        }
        let list = history(&paths);
        let dates: Vec<&str> = list.iter().map(|c| c.picture.date.as_str()).collect();
        assert_eq!(dates, ["2026-11-22", "2026-11-21", "2026-11-20"]);
        assert!(list[0].thumb.to_string_lossy().ends_with(".thumb.jpg"));

        let platform = FakePlatform::default();
        use_cached(&Config::default(), &paths, &platform, &list[2]).unwrap();
        assert_eq!(
            State::load(&paths.state_file()).current.unwrap().date,
            "2026-11-20"
        );
        assert_eq!(platform.desktop.borrow().len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn seal_copies_current_picture() {
        let dir = temp("seal");
        let paths = Paths::in_dir(&dir);
        let fetch = |_: &str, _: Resolution| Ok(sample("2026-11-22"));
        apply_with(
            &Config::default(),
            &paths,
            &FakePlatform::default(),
            "de-DE",
            "2026-11-22",
            &fetch,
            &write_jpeg,
        )
        .unwrap();
        let sealed = seal(&paths).unwrap();
        assert_eq!(sealed.date, "2026-11-22");
        let (pic, img) = baseline(&paths).unwrap();
        assert_eq!(pic.id, sealed.id);
        assert!(img.is_file());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
