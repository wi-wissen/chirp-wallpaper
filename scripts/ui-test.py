#!/usr/bin/env python3
"""End-to-end checks of the Windows app (it also runs on Linux), as a user sees it.

    cargo build --release -p chirp-app
    scripts/ui-test.py                # all scenarios and the layout sheet
    scripts/ui-test.py first-start    # one scenario
    scripts/ui-test.py --gnome        # the same scenarios with the GNOME app
                                      # (cargo build --release -p chirp-gnome)

Each scenario starts a local fake Bing (scripts/fake-bing.py), a virtual
screen (Xvfb) and the app with an empty profile, takes screenshots at fixed
times and checks the app's trace (CHIRP_TRACE) against what must happen.
Results go to target/ui-test/ (GNOME: target/ui-test-gnome/): one PNG strip per
scenario and layout.png.
A failed check exits with status 1.

Needs: Xvfb, ImageMagick (import), Python 3 with Pillow for the sheets.
"""

import os
import pathlib
import shutil
import signal
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
APP = ROOT / "target/release/ChirpWallpaper"
GNOME = "--gnome" in sys.argv
if GNOME:
    APP = ROOT / "target/release/chirp-wallpaper"
OUT = ROOT / ("target/ui-test-gnome" if GNOME else "target/ui-test")
FAKE = ROOT / "scripts/fake-bing.py"


RUN_NUMBER = [0]


def wait_for_port(port, seconds=10):
    import socket
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        with socket.socket() as s:
            if s.connect_ex(("127.0.0.1", port)) == 0:
                return
        time.sleep(0.1)
    raise RuntimeError(f"fake Bing did not start on port {port}")


class Run:
    """One app start in its own profile, display and fake Bing.

    Every run gets its own display number and port, so a process that is
    still shutting down can never answer for the next run."""

    def __init__(self, name, home, offline_for=0, slow=0, lang="de_DE.UTF-8",
                 scale="1", size="1040x700", page=0):
        RUN_NUMBER[0] += 1
        display = 90 + RUN_NUMBER[0]
        port = 8700 + RUN_NUMBER[0]
        self.name, self.home = name, home
        self.trace = home / "trace.log"
        self.trace.unlink(missing_ok=True)  # each run checks only its own events
        self.display = f":{display}"
        w, h = (int(v) for v in size.split("x"))
        f = float(scale)
        screen = f"{int(w * f)}x{int(h * f)}x24"
        self.procs = [
            subprocess.Popen(["Xvfb", self.display, "-screen", "0", screen, "-nolisten", "tcp"],
                             stderr=subprocess.DEVNULL),
            subprocess.Popen([sys.executable, str(FAKE), "--port", str(port),
                              "--offline-for", str(offline_for), "--slow", str(slow)],
                             stderr=open(home / "fake-bing.log", "a")),
        ]
        wait_for_port(port)
        time.sleep(0.5)  # Xvfb
        env = dict(os.environ, DISPLAY=self.display, HOME=str(home),
                   XDG_DATA_HOME=str(home / "data"), XDG_CONFIG_HOME=str(home / "config"),
                   LANG=lang, LANGUAGE=lang.split("_")[0], LC_ALL="",
                   CHIRP_BING_BASE=f"http://127.0.0.1:{port}", CHIRP_TRACE=str(self.trace),
                   SLINT_SCALE_FACTOR=scale, CHIRP_WINDOW=size, CHIRP_PAGE=str(page))
        env.pop("WAYLAND_DISPLAY", None)
        # Talk to the local fake Bing directly, never through a proxy.
        for var in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"):
            env.pop(var, None)
        env["NO_PROXY"] = env["no_proxy"] = "127.0.0.1,localhost"
        self.start = time.monotonic()
        env["GSK_RENDERER"] = "cairo"
        cmd = ["dbus-run-session", "--", str(APP)] if GNOME else [str(APP)]
        self.app = subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL,
                                    stderr=open(home / "app.log", "a"))

    def shot(self, at, path):
        time.sleep(max(0.0, at - (time.monotonic() - self.start)))
        subprocess.run(["import", "-display", self.display, "-window", "root", str(path)],
                       check=False, stderr=subprocess.DEVNULL)
        return path

    def stop(self):
        self.app.send_signal(signal.SIGTERM)
        self.app.wait(timeout=10)
        for p in self.procs:
            p.terminate()
            p.wait(timeout=10)

    def events(self):
        if not self.trace.exists():
            return []
        lines = self.trace.read_text(encoding="utf-8").splitlines()
        return [(int(l.split(" ", 1)[0]), l.split(" ", 1)[1]) for l in lines if " " in l]


def strip(paths, out, labels):
    """Put screenshots side by side with their time."""
    from PIL import Image, ImageDraw
    ims = [Image.open(p).convert("RGB") for p in paths]
    w = 520
    ims = [im.resize((w, int(im.height * w / im.width))) for im in ims]
    h = max(im.height for im in ims) + 22
    sheet = Image.new("RGB", (len(ims) * (w + 8), h), "white")
    draw = ImageDraw.Draw(sheet)
    for i, (im, label) in enumerate(zip(ims, labels)):
        sheet.paste(im, (i * (w + 8), 22))
        draw.text((i * (w + 8) + 4, 4), label, fill="black")
    sheet.save(out)


class Check:
    def __init__(self, name):
        self.name, self.failures = name, []

    def that(self, ok, text):
        print(f"  {'ok  ' if ok else 'FAIL'} {text}")
        if not ok:
            self.failures.append(text)


def first(events, prefix):
    return next((t for t, e in events if e.startswith(prefix)), None)


def scenario_first_start(home):
    """Empty profile, network fine but pictures slow: the spinner must be visible."""
    run = Run("first-start", home, slow=4)
    times = [0.8, 2.5, 5, 9]
    shots = [run.shot(t, OUT / f"first-start-{t}s.png") for t in times]
    run.stop()
    ev = run.events()
    c = Check("first-start")
    busy, pic = first(ev, "busy=true"), first(ev, "snapshot has_picture=true")
    c.that(busy is not None and busy < 1500, f"check starts at once (busy after {busy} ms)")
    c.that(pic is not None, f"picture appears (after {pic} ms)")
    c.that(busy is not None and pic is not None and pic - busy > 2000,
           "loading state lasts long enough to be seen (spinner)")
    c.that(not any("error=true" in e for _, e in ev), "no error shown")
    c.that("spinner=loading" in [e for _, e in ev], "spinner says the picture is loading")
    c.that("spinner=waiting" not in [e for _, e in ev],
           "never claims to wait for a connection while online")
    c.that(any(e.startswith("toast=") for _, e in ev), "new picture is announced")
    strip(shots, OUT / "first-start.png", [f"{t}s" for t in times])
    return c


def scenario_offline_start(home):
    """Empty profile, no network for 25 s (e.g. right after sign-in)."""
    run = Run("offline-start", home, offline_for=25, slow=1)
    times = [1.5, 8, 20, 40]
    shots = [run.shot(t, OUT / f"offline-start-{t}s.png") for t in times]
    run.stop()
    ev = run.events()
    c = Check("offline-start")
    c.that(not any("error=true" in e for _, e in ev), "no error shown while offline")
    c.that(first(ev, "waiting=true") is not None, "waits for the connection")
    c.that("spinner=waiting" in [e for _, e in ev], "spinner says it waits for the connection")
    pic = first(ev, "snapshot has_picture=true")
    c.that(pic is not None and pic > 25000, f"picture appears once online (after {pic} ms)")
    c.that(not any(e.startswith("toast=") and t < 25000 for t, e in ev),
           "no message while offline")
    strip(shots, OUT / "offline-start.png", [f"{t}s" for t in times])
    return c


def scenario_offline_with_picture(home):
    """A picture from before, no network at all: picture stays, calm note, no error."""
    run = Run("offline-with-picture", home, offline_for=10_000)
    times = [3, 15]
    shots = [run.shot(t, OUT / f"offline-with-picture-{t}s.png") for t in times]
    run.stop()
    ev = run.events()
    c = Check("offline-with-picture")
    c.that(first(ev, "snapshot has_picture=true") is not None, "previous picture is shown")
    c.that(not any("error=true" in e for _, e in ev), "no error shown")
    c.that(not any(e.startswith("toast=") for _, e in ev), "no message pops up")
    strip(shots, OUT / "offline-with-picture.png", [f"{t}s" for t in times])
    return c


def seed_history(home, days=5):
    """Copy the loaded picture as older days, so the history has several cards."""
    import json
    images = home / "data/chirp-wallpaper/images"
    metas = sorted(p for p in images.glob("*.json"))
    if not metas:
        return
    meta = json.loads(metas[0].read_text(encoding="utf-8"))
    stem = metas[0].with_suffix("")
    titles = ["Abendlicht über dem Meer", "Ruhige See", "Hügel im Morgengrauen",
              "Eine Wanderung durch sehr lange Täler mit Titel, der nicht passt", "Herbst"]
    for n in range(1, days + 1):
        date = f"2026-01-{n:02d}"
        m = dict(meta, id=f"bing-de-DE-{date}", date=date, title=titles[(n - 1) % len(titles)])
        target = images / m["id"]
        shutil.copy(f"{stem}.jpg", f"{target}.jpg")
        if pathlib.Path(f"{stem}.thumb.jpg").exists():
            shutil.copy(f"{stem}.thumb.jpg", f"{target}.thumb.jpg")
        (images / f"{m['id']}.json").write_text(json.dumps(m), encoding="utf-8")


def layout_sheet(home):
    """Screenshots of every page at several scales, sizes and languages."""
    seed_history(home)
    combos = [("de_DE.UTF-8", "1", "1040x700"), ("de_DE.UTF-8", "1.5", "1040x700"),
              ("de_DE.UTF-8", "1.5", "760x520"), ("uk_UA.UTF-8", "1.25", "760x520")]
    shots, labels = [], []
    for lang, scale, size in combos:
        for page in (0, 1, 2):
            run = Run("layout", home, lang=lang, scale=scale, size=size, page=page,
                      offline_for=10_000)
            path = run.shot(3, OUT / f"layout-{lang[:2]}-{scale}-{size}-{page}.png")
            run.stop()
            shots.append(path)
            labels.append(f"{lang[:2]} {scale}x {size} page {page}")
    from PIL import Image
    rows = [shots[i:i + 3] for i in range(0, len(shots), 3)]
    parts = []
    for i, row in enumerate(rows):
        strip(row, OUT / f"layout-row{i}.png", labels[i * 3:i * 3 + 3])
        parts.append(Image.open(OUT / f"layout-row{i}.png"))
    sheet = Image.new("RGB", (max(p.width for p in parts), sum(p.height for p in parts)), "white")
    y = 0
    for p in parts:
        sheet.paste(p, (0, y))
        y += p.height
    sheet.save(OUT / "layout.png")


def main():
    if not APP.exists():
        sys.exit("Build first: cargo build --release -p chirp-app")
    wanted = [a for a in sys.argv[1:] if not a.startswith("--")]
    if OUT.exists():
        shutil.rmtree(OUT)
    OUT.mkdir(parents=True)
    home = OUT / "home"
    home.mkdir()
    checks = []
    # Order matters: later scenarios use the picture loaded by the first one.
    for name, fn in [("first-start", scenario_first_start),
                     ("offline-start", None),
                     ("offline-with-picture", scenario_offline_with_picture)]:
        if wanted and name not in wanted:
            continue
        print(name)
        if name == "offline-start":
            fresh = OUT / "home-offline"
            fresh.mkdir()
            checks.append(scenario_offline_start(fresh))
        else:
            checks.append(fn(home))
    if not GNOME and (not wanted or "layout" in wanted):
        print("layout (look at target/ui-test/layout.png)")
        layout_sheet(home)
    failed = [c for c in checks if c.failures]
    print(f"\n{len(checks) - len(failed)} of {len(checks)} scenarios passed")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
