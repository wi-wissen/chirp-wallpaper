//! Where files live.
//!
//! * **User**: the signed-in user's own folders, no administrator rights.
//! * **Machine**: shared folders for all users (ProgramData on Windows).
//!   Used by the start-up task on reset school machines.
//! * **Portable**: everything next to the program, if a `chirp.toml` lies
//!   beside it.

use crate::picture::Picture;
use std::path::{Path, PathBuf};

pub const APP_DIR: &str = "chirp-wallpaper";
pub const PORTABLE_MARKER: &str = "chirp.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    User,
    Machine,
    Portable,
}

#[derive(Debug, Clone)]
pub struct Paths {
    pub scope: Scope,
    /// Cache, state and baseline of this scope.
    pub data_dir: PathBuf,
    /// Data folder of the machine scope, read as a fallback by users.
    pub machine_data_dir: PathBuf,
    /// Config files in the order they are merged.
    pub config_files: Vec<PathBuf>,
    /// The config file settings are saved to.
    pub config_file: PathBuf,
}

impl Paths {
    /// Pick the scope: portable if a marker file sits next to the program,
    /// otherwise machine or user as requested.
    pub fn detect(machine: bool) -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        if let Some(dir) = exe_dir.filter(|d| d.join(PORTABLE_MARKER).is_file()) {
            return Self::portable(&dir);
        }
        if machine {
            Self::machine()
        } else {
            Self::user()
        }
    }

    pub fn user() -> Self {
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join(APP_DIR);
        let config_file = dirs::config_dir()
            .unwrap_or_else(|| data_dir.clone())
            .join(APP_DIR)
            .join("config.toml");
        Self {
            scope: Scope::User,
            data_dir,
            machine_data_dir: machine_data_dir(),
            config_files: vec![machine_config_file(), config_file.clone()],
            config_file,
        }
    }

    pub fn machine() -> Self {
        Self {
            scope: Scope::Machine,
            data_dir: machine_data_dir(),
            machine_data_dir: machine_data_dir(),
            config_files: vec![machine_config_file()],
            config_file: machine_config_file(),
        }
    }

    pub fn portable(dir: &Path) -> Self {
        let config_file = dir.join(PORTABLE_MARKER);
        Self {
            scope: Scope::Portable,
            data_dir: dir.join("chirp-data"),
            machine_data_dir: machine_data_dir(),
            config_files: vec![machine_config_file(), config_file.clone()],
            config_file,
        }
    }

    /// Everything inside one folder; used by tests.
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            scope: Scope::User,
            data_dir: dir.join("user"),
            machine_data_dir: dir.join("machine"),
            config_files: vec![dir.join("config.toml")],
            config_file: dir.join("config.toml"),
        }
    }

    pub fn images_dir(&self) -> PathBuf {
        self.data_dir.join("images")
    }

    pub fn state_file(&self) -> PathBuf {
        self.data_dir.join("state.json")
    }

    pub fn image_file(&self, picture: &Picture) -> PathBuf {
        self.images_dir()
            .join(format!("{}.jpg", picture.file_stem()))
    }

    pub fn thumb_file(&self, picture: &Picture) -> PathBuf {
        self.images_dir()
            .join(format!("{}.thumb.jpg", picture.file_stem()))
    }

    pub fn meta_file(&self, picture: &Picture) -> PathBuf {
        self.images_dir()
            .join(format!("{}.json", picture.file_stem()))
    }

    /// Same picture in the machine cache (downloaded by the start-up task).
    pub fn shared_image_file(&self, picture: &Picture) -> PathBuf {
        self.machine_data_dir
            .join("images")
            .join(format!("{}.jpg", picture.file_stem()))
    }

    pub fn baseline_image(&self) -> PathBuf {
        self.data_dir.join("baseline.jpg")
    }

    pub fn baseline_meta(&self) -> PathBuf {
        self.data_dir.join("baseline.json")
    }

    /// Baseline files to look at, own scope first, then the machine's.
    pub fn baseline_candidates(&self) -> Vec<(PathBuf, PathBuf)> {
        let mut list = vec![(self.baseline_image(), self.baseline_meta())];
        if self.machine_data_dir != self.data_dir {
            list.push((
                self.machine_data_dir.join("baseline.jpg"),
                self.machine_data_dir.join("baseline.json"),
            ));
        }
        list
    }
}

fn machine_data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        let base = std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        base.join("ChirpWallpaper")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/var/lib").join(APP_DIR)
    }
}

fn machine_config_file() -> PathBuf {
    #[cfg(windows)]
    {
        machine_data_dir().join("config.toml")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/etc").join(APP_DIR).join("config.toml")
    }
}
