# Working on Chirp Wallpaper

The owner tests releases on real Windows and GNOME machines. Everything that can be checked here must be
checked here first, so that the owner never finds a bug that a test or a look at a screenshot would have shown.

## Definition of done (before every push)

1. `cargo fmt --all --check`
2. `cargo clippy --all-targets -- -D warnings` and
   `cargo clippy --target x86_64-pc-windows-gnu -p chirp-app -p chirp -p chirp-native -- -D warnings`
3. `cargo test`
4. `cargo +1.89 build -p chirp-gnome` (Rust of the Flatpak SDK) and
   `flatpak/cargo-sources.py Cargo.lock > flatpak/cargo-sources.json` if `Cargo.lock` changed
5. UI: `cargo build --release -p chirp-app -p chirp-gnome`, then `scripts/ui-test.py` and
   `scripts/ui-test.py --gnome`. All scenarios must pass.
6. **Look at every image** in `target/ui-test/` and `target/ui-test-gnome/` (`layout.png` and each scenario strip) and check:
   nothing cut off or outside the window, no large empty gaps, text readable, loading visible,
   no error where none is expected. Fix what looks wrong before pushing, not after the owner reports it.

A UI change needs a scenario or a layout case in `scripts/ui-test.py` that shows it.
A change to how and when pictures are loaded needs a test in `chirp-core` and a scenario.

## Rules

* Do not add behaviour nobody asked for (e.g. pausing after "Use"). Ask first.
* Buttons always have visible text; an icon alone is not accessible.
* Every user-visible text exists in all 21 Fluent files (`crates/chirp-native/locales`); the
  `every_locale_parses_and_has_every_message` test enforces it.
* Windows-only behaviour (registry, IDesktopWallpaper, lock screen, scheduled tasks, UAC) cannot run here.
  Compile it for `x86_64-pc-windows-gnu`, and say clearly in the summary that it is untested on Windows.
* Report results honestly: what was tested, how, and what was not.
* Work directly on `main`; commits in English, conversation with the owner in German.

## Test tools

* `scripts/fake-bing.py`: local stand-in for www.bing.com (`--offline-for N`, `--slow S`).
  Apps use it via `CHIRP_BING_BASE=http://127.0.0.1:<port>`.
* `CHIRP_TRACE=<file>`: apps write what they show (busy, waiting, picture, status, messages).
* `CHIRP_DEMO=1`: no automatic loading (screenshots). `CHIRP_PAGE`, `CHIRP_WINDOW=WxH`, `CHIRP_CONFIRM`
  open a page, a window size or a dialog directly.
