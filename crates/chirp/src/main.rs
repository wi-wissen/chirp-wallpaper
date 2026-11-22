//! `chirp`: sets the daily Bing picture as desktop background and lock screen.
//!
//! Runs without installation and without administrator rights. Only the
//! `--machine` variants (all users, reset school machines) need them.

use chirp_core::apply::{baseline, seal, seal_custom};
use chirp_core::{Config, Outcome, Paths, Resolution, State, Target};
use chirp_native::{platform, InstallError, Localizer};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "chirp",
    version,
    about = "Chirp Wallpaper: the daily Bing picture as desktop background and lock screen."
)]
struct Cli {
    /// Interface language, e.g. `de` or `pt-BR` (default: system language).
    #[arg(long, global = true)]
    lang: Option<String>,
    /// For all users of this computer (needs administrator rights).
    #[arg(long, global = true)]
    machine: bool,
    /// Print nothing unless something goes wrong.
    #[arg(long, short, global = true)]
    quiet: bool,
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Load today's picture and set it (default).
    Apply,
    /// Show the current picture, its description and the last result.
    Status,
    /// Save the current picture (or your own) as baseline for reset machines.
    Seal {
        /// Use this picture instead of the current one, e.g. a school logo.
        #[arg(long)]
        image: Option<PathBuf>,
        /// Title shown for your own picture.
        #[arg(long, default_value = "")]
        title: String,
    },
    /// Change settings.
    Config {
        /// Bing region such as `de-DE`, or `auto`.
        #[arg(long)]
        market: Option<String>,
        #[arg(long)]
        resolution: Option<ResolutionArg>,
        #[arg(long)]
        desktop: Option<OnOff>,
        #[arg(long)]
        lockscreen: Option<OnOff>,
        /// Keep the current picture until this day (YYYY-MM-DD).
        #[arg(long)]
        pause_until: Option<String>,
        /// End a pause.
        #[arg(long)]
        resume: bool,
    },
    /// Change the picture automatically (at sign-in and every hour).
    Install,
    /// Stop changing the picture automatically.
    Uninstall,
}

#[derive(Clone, Copy, ValueEnum)]
enum ResolutionArg {
    Uhd,
    #[value(name = "1080p")]
    FullHd,
}

#[derive(Clone, Copy, ValueEnum)]
enum OnOff {
    On,
    Off,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let l = Localizer::new(cli.lang.as_deref());
    let paths = Paths::detect(cli.machine);
    let result = match cli.command.unwrap_or(Cmd::Apply) {
        Cmd::Apply => run_apply(&l, &paths, cli.machine, cli.quiet),
        Cmd::Status => run_status(&l, &paths),
        Cmd::Seal { image, title } => run_seal(&l, &paths, image, &title, cli.quiet),
        Cmd::Config {
            market,
            resolution,
            desktop,
            lockscreen,
            pause_until,
            resume,
        } => run_config(
            &l,
            &paths,
            market,
            resolution,
            desktop,
            lockscreen,
            pause_until,
            resume,
            cli.quiet,
        ),
        Cmd::Install => report_install(
            &l,
            platform::install(cli.machine),
            "install-done",
            cli.quiet,
        ),
        Cmd::Uninstall => report_install(
            &l,
            platform::uninstall(cli.machine),
            "uninstall-done",
            cli.quiet,
        ),
    };
    match result {
        Ok(code) => code,
        Err(reason) => {
            eprintln!("{}", l.t("error", &[("reason", &reason)]));
            ExitCode::FAILURE
        }
    }
}

type CmdResult = std::result::Result<ExitCode, String>;

fn system_locale() -> Option<String> {
    sys_locale::get_locale()
}

fn run_apply(l: &Localizer, paths: &Paths, machine: bool, quiet: bool) -> CmdResult {
    let mut cfg = Config::load(&paths.config_files).map_err(|e| e.to_string())?;
    if machine {
        cfg.desktop = false;
    }
    let market = cfg.resolved_market(system_locale().as_deref());
    let platform = platform::NativePlatform::new(machine);
    let outcome = chirp_core::apply(
        &cfg,
        paths,
        &platform,
        &market,
        // Scheduled runs (--quiet) keep a picture chosen by hand until a new one appears.
        if quiet {
            chirp_core::Trigger::Automatic
        } else {
            chirp_core::Trigger::Manual
        },
    )
    .map_err(|e| e.to_string())?;
    chirp_core::complete_history(paths);
    match outcome {
        Outcome::Updated { picture, warnings } | Outcome::AlreadyCurrent { picture, warnings }
            if !warnings.is_empty() =>
        {
            if !quiet {
                println!("{}", l.t("updated", &[("title", &picture.title)]));
            }
            for (target, reason) in warnings {
                let id = match target {
                    Target::Desktop => "warning-desktop",
                    Target::LockScreen => "warning-lockscreen",
                };
                eprintln!("{}", l.t(id, &[("reason", &reason)]));
            }
            Ok(ExitCode::SUCCESS)
        }
        Outcome::Updated { picture, .. } => {
            if !quiet {
                println!("{}", l.t("updated", &[("title", &picture.title)]));
            }
            Ok(ExitCode::SUCCESS)
        }
        Outcome::AlreadyCurrent { picture, .. } => {
            if !quiet {
                println!("{}", l.t("already-current", &[("title", &picture.title)]));
            }
            Ok(ExitCode::SUCCESS)
        }
        Outcome::Paused { until } => {
            if !quiet {
                println!("{}", l.t("paused", &[("date", &until)]));
            }
            Ok(ExitCode::SUCCESS)
        }
        Outcome::Unchanged { reason, kept } => {
            // Quiet runs (tasks) stay silent offline: the state file records it.
            if !quiet {
                println!("{}", l.t("unchanged", &[("reason", &reason)]));
                if let Some(p) = kept {
                    println!(
                        "{}",
                        l.t("unchanged-kept", &[("title", &p.title), ("date", &p.date)])
                    );
                }
            }
            Ok(ExitCode::from(2))
        }
    }
}

fn run_status(l: &Localizer, paths: &Paths) -> CmdResult {
    let state = State::load(&paths.state_file());
    let none = l.t("none", &[]);
    println!("{}", l.t("status-heading", &[]));
    match &state.current {
        Some(p) => {
            println!("  {}", p.title);
            if !p.caption.is_empty() && p.caption != p.title {
                println!("  {}", p.caption);
            }
            println!("  {}: {}", l.t("label-date", &[]), p.date);
            println!("  {}: {}", l.t("label-region", &[]), p.market);
            if !p.copyright.is_empty() {
                println!("  {}: {}", l.t("label-copyright", &[]), p.copyright);
            }
            if let Some(d) = &p.description {
                println!();
                for line in wrap(d, 76) {
                    println!("  {line}");
                }
            }
            if let Some(u) = &p.info_url {
                println!();
                println!("  {}: {u}", l.t("label-more", &[]));
            }
        }
        None => println!("  {none}"),
    }
    println!();
    println!(
        "{}: {}",
        l.t("label-last-success", &[]),
        state.last_success.as_deref().unwrap_or(&none)
    );
    if let Some(e) = &state.last_error {
        println!("{}: {e}", l.t("label-last-error", &[]));
    }
    let base = baseline(paths)
        .map(|(p, _)| format!("{} ({})", p.title, p.date))
        .unwrap_or(none);
    println!("{}: {base}", l.t("label-baseline", &[]));
    Ok(ExitCode::SUCCESS)
}

fn run_seal(
    l: &Localizer,
    paths: &Paths,
    image: Option<PathBuf>,
    title: &str,
    quiet: bool,
) -> CmdResult {
    let picture = match image {
        Some(img) => {
            let t = if title.is_empty() {
                img.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default()
            } else {
                title.to_string()
            };
            seal_custom(paths, &img, &t)
        }
        None => seal(paths),
    }
    .map_err(|e| e.to_string())?;
    if !quiet {
        println!(
            "{}",
            l.t(
                "seal-done",
                &[("title", &picture.title), ("date", &picture.date)]
            )
        );
    }
    Ok(ExitCode::SUCCESS)
}

#[allow(clippy::too_many_arguments)]
fn run_config(
    l: &Localizer,
    paths: &Paths,
    market: Option<String>,
    resolution: Option<ResolutionArg>,
    desktop: Option<OnOff>,
    lockscreen: Option<OnOff>,
    pause_until: Option<String>,
    resume: bool,
    quiet: bool,
) -> CmdResult {
    // Only this scope's own file is changed, so machine settings stay intact.
    let mut cfg = Config::load(&[&paths.config_file]).map_err(|e| e.to_string())?;
    if let Some(m) = market {
        cfg.market = m;
    }
    if let Some(r) = resolution {
        cfg.resolution = match r {
            ResolutionArg::Uhd => Resolution::Uhd,
            ResolutionArg::FullHd => Resolution::FullHd,
        };
    }
    if let Some(d) = desktop {
        cfg.desktop = matches!(d, OnOff::On);
    }
    if let Some(s) = lockscreen {
        cfg.lockscreen = matches!(s, OnOff::On);
    }
    if let Some(p) = pause_until {
        cfg.paused_until = Some(p);
    }
    if resume {
        cfg.paused_until = None;
    }
    cfg.save(&paths.config_file).map_err(|e| match e {
        chirp_core::Error::Io(io) if io.kind() == std::io::ErrorKind::PermissionDenied => {
            l.t("needs-admin", &[])
        }
        other => other.to_string(),
    })?;
    if !quiet {
        println!(
            "{}",
            l.t(
                "config-saved",
                &[("file", &paths.config_file.display().to_string())]
            )
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn report_install(
    l: &Localizer,
    result: Result<(), InstallError>,
    ok_id: &str,
    quiet: bool,
) -> CmdResult {
    match result {
        Ok(()) => {
            if !quiet {
                println!("{}", l.t(ok_id, &[]));
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(InstallError::NeedsAdmin) => Err(l.t("needs-admin", &[])),
        Err(InstallError::NotSupported) => Err(l.t("not-supported", &[])),
        Err(InstallError::Failed(reason)) => Err(reason),
    }
}

/// Simple word wrap for the description.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}
