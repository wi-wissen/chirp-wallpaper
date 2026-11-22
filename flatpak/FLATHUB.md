# Publishing on Flathub

The app id `io.github.wi_wissen.ChirpWallpaper` belongs to the GitHub account `wi-wissen`;
Flathub verifies it automatically through the GitHub login of the person who submits it.

## What is already in this repository

| File | Purpose |
|---|---|
| `flatpak/io.github.wi_wissen.ChirpWallpaper.yml` | manifest (GNOME 48 runtime, Rust from the SDK extension, offline build) |
| `flatpak/cargo-sources.json` | all crates, so Flathub can build without network; regenerate with `flatpak/cargo-sources.py Cargo.lock > flatpak/cargo-sources.json` |
| `crates/chirp-gnome/data/*.metainfo.xml` | AppStream data (description, screenshot, release notes, OARS rating) |
| `crates/chirp-gnome/data/*.desktop`, `*.svg` | launcher and icon |

Permissions: network (to load the picture), Wayland/X11 and GPU for the window. No access to files or
settings: the background is set through the **wallpaper portal** and autostart through the
**background portal**, which is what Flathub expects.

The *Flatpak* workflow builds the bundle on every change, and every release attaches `ChirpWallpaper.flatpak`.

## Submitting (one time)

1. Create a release (Actions → Release), e.g. `v0.1.0`.
2. Fork <https://github.com/flathub/flathub> and create a branch from `new-pr`.
3. Add `io.github.wi_wissen.ChirpWallpaper.yml` (copy of our manifest) and `cargo-sources.json`.
   In the copy, replace the `dir` source with the release tag:

   ```yaml
       sources:
         - type: git
           url: https://github.com/wi-wissen/chirp-wallpaper.git
           tag: v0.1.0
           commit: <commit of the tag>
         - cargo-sources.json
   ```

4. Open a pull request against `new-pr` titled "Add io.github.wi_wissen.ChirpWallpaper".
   Reviewers check the manifest, metadata and permissions; their build bot comments with a test build.
5. After the merge Flathub creates `flathub/io.github.wi_wissen.ChirpWallpaper`; later updates are pull
   requests there (new tag, new `cargo-sources.json`, new `<release>` in the metainfo).

## Installing the bundle without Flathub

Ubuntu does not include Flatpak by default:

```
sudo apt install flatpak gnome-software-plugin-flatpak
flatpak remote-add --if-not-exists --user flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user ChirpWallpaper.flatpak
```

Fedora and most other GNOME distributions have Flatpak and Flathub ready: double-click the file.
