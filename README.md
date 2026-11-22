<p align="center"><img src="assets/logo.svg" width="96" alt=""></p>

# Chirp Wallpaper

The daily Bing picture as desktop background **and** lock screen, for Windows 10/11 and GNOME.
Free, open source (MIT), one small program: no installation, no account, no administrator rights.

* **4K**: downloads the largest version Bing offers (usually 3840×2160), falls back to 1920×1080.
* **The story behind the picture**: title, place, copyright, a short text and a link to learn more.
* **21 languages**, picked from the system language (English otherwise). Pictures and texts follow your Bing region.
* **Offline-safe**: if no new picture can be loaded, nothing on screen changes; the reason is remembered and shown.
* **Made for reset school computers**: settings and tasks are prepared once; a baseline picture covers days without internet.

> Status: early version. The command line program works; the windowed app (Windows 11 style and GNOME) follows.
> Chirp Wallpaper is not affiliated with Microsoft. Pictures and texts come from Bing and belong to their photographers.

## Download

| System | File |
|---|---|
| Windows 10/11 (x64) | [chirp-windows-x64.exe](https://github.com/wi-wissen/chirp-wallpaper/releases/latest/download/chirp-windows-x64.exe) |
| Linux with GNOME | [chirp-linux-x86_64.tar.gz](https://github.com/wi-wissen/chirp-wallpaper/releases/latest/download/chirp-linux-x86_64.tar.gz) |

Rename the Windows file to `chirp.exe` if you like and put it anywhere.

## Use

```
chirp apply        # load today's picture and set it
chirp status       # what is on screen, the story behind it, last result
chirp install      # change automatically at sign-in and every hour
chirp uninstall
```

Settings:

```
chirp config --market de-DE        # Bing region, or "auto" (system language)
chirp config --resolution 1080p    # or "uhd" (default)
chirp config --lockscreen off
chirp config --pause-until 2026-12-24
chirp config --resume
```

`--lang de` overrides the interface language. Exit code `2` means "no new picture, nothing changed".

### Portable mode

Put an (even empty) `chirp.toml` next to the program. Settings and pictures then stay in that folder.

## Reset ("frozen") school computers

Computers that return to a saved state at every start keep what was set up while the image was prepared,
but lose everything downloaded later. Prepare the image once, as administrator:

```
chirp config --machine --market de-DE
chirp install --machine     # start-up task as SYSTEM + sign-in entry for all users
chirp apply --machine       # download now, set the lock screen for everyone
chirp seal --machine        # keep today's picture as baseline
```

Or use your own baseline picture, e.g. the school logo:

```
chirp seal --machine --image C:\Images\school.jpg --title "Our school"
```

At every start the SYSTEM task downloads the picture into `%ProgramData%\ChirpWallpaper` and sets the lock screen
(`PersonalizationCSP`, also shown before sign-in). At sign-in each user's desktop is set from that shared cache
without a second download. Without internet the screen keeps the baseline picture, and `chirp status` says which one it is.

Network access needed: `www.bing.com` only.

## Where things are

| | Windows | Linux |
|---|---|---|
| User settings | `%APPDATA%\chirp-wallpaper\config.toml` | `~/.config/chirp-wallpaper/config.toml` |
| User pictures | `%LOCALAPPDATA%\chirp-wallpaper` | `~/.local/share/chirp-wallpaper` |
| Machine settings and pictures | `%ProgramData%\ChirpWallpaper` | `/etc/chirp-wallpaper`, `/var/lib/chirp-wallpaper` |

Settings are merged: defaults → machine → portable → user.

## GNOME

The desktop is set with `gsettings`; GNOME's lock screen shows the same picture blurred.
The login screen (GDM) has no supported setting and is left unchanged.
`chirp install` registers a systemd user timer.

## Translations

Messages live in [`crates/chirp/locales`](crates/chirp/locales), one [Fluent](https://projectfluent.org) file per language.
All files except English and German are first drafts and welcome corrections by native speakers.
A test checks that every language has every message.

## Build

```
cargo test
cargo build --release -p chirp
```

## Release

Actions → **Release** → *Run workflow*, enter a version like `0.2.0`. The workflow builds `main`, tags it and
publishes the files under fixed names, so the links above always point to the newest version.

## Licence

MIT, see [LICENSE](LICENSE). Clean-room implementation: no code from other wallpaper apps was used.
