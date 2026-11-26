#!/usr/bin/env python3
"""A local stand-in for www.bing.com (and the Microsoft Edge background
service), for testing the apps without internet.

    scripts/fake-bing.py --port 8765 --offline-for 20 --slow 3

* --offline-for N  for the first N seconds every connection is dropped, as
                   if there were no network.
* --slow S         pictures are sent after S seconds, so loading is visible.

Start an app with CHIRP_BING_BASE=http://127.0.0.1:8765 to use it
(CHIRP_EDGE_BASE=http://127.0.0.1:8765 for the Edge source).
Only the Python standard library is needed.
"""

import argparse
import datetime
import http.server
import json
import pathlib
import sys
import time

DATA = pathlib.Path(__file__).with_name("testdata")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=8765)
    ap.add_argument("--offline-for", type=float, default=0)
    ap.add_argument("--slow", type=float, default=0)
    args = ap.parse_args()
    started = time.monotonic()
    today = datetime.date.today()
    stamp = today.strftime("%Y%m%d")
    name = "OHR.TestHills_DE-DE1234"

    archive = {"images": [{
        "startdate": stamp,
        "fullstartdate": (today - datetime.timedelta(days=1)).strftime("%Y%m%d") + "2300",
        "enddate": (today + datetime.timedelta(days=1)).strftime("%Y%m%d"),
        "urlbase": f"/th?id={name}",
        "copyright": "Möwen über den Hügeln, Beispielland (© Testbild)",
        "copyrightlink": "https://www.bing.com/search?q=Testbild",
        "title": "Möwen über den Hügeln",
    }]}
    model = {"MediaContents": [{
        "Ssd": stamp,
        "ImageContent": {
            "Image": {"Url": f"/th?id={name}_1920x1080.webp"},
            "Headline": "Möwen über den Hügeln",
            "Title": "Hügel bei Sonnenaufgang, Beispielland",
            "Description": "Ein Testbild aus der lokalen Testumgebung. Es zeigt, wie die App "
                           "Titel, Ort, Copyright und Beschreibung darstellt.",
            "Copyright": "© Testbild",
        },
    }]}

    def edge_size(size):
        return f"EdgeTest{size}"

    edge_config = {"configs": {"BackgroundImageWC/default": {"properties": {
        "simpleImages": {"data": [
            {"attribution": "Testbild", "image": {
                k: edge_size(k[1:]) for k in ("i100", "i240", "i720", "i1080", "i1440", "i2160")}},
            {"attribution": "Getty Images", "image": {"i1080": "EdgeOther1080",
                                                      "i2160": "EdgeOther2160"}},
        ]},
    }}}}

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, fmt, *a):
            sys.stderr.write(f"fake-bing {time.monotonic() - started:6.1f}s {fmt % a}\n")

        def do_GET(self):
            if time.monotonic() - started < args.offline_for:
                self.log_message("drop %s (offline)", self.path)
                self.close_connection = True
                self.connection.close()
                return
            if self.path.startswith("/HPImageArchive.aspx"):
                return self.send(200, "application/json", json.dumps(archive).encode())
            if self.path.startswith("/hp/api/model"):
                return self.send(200, "application/json", json.dumps(model).encode())
            if self.path.startswith("/resolver/api/resolve/v3/config/"):
                if "v=" not in self.path:
                    return self.send(400, "application/json", b'{"status":400}')
                return self.send(200, "application/json", json.dumps(edge_config).encode())
            if self.path.startswith("/tenant/amp/entityid/"):
                time.sleep(args.slow)
                thumb = "240" in self.path
                body = (DATA / ("sample-thumb.jpg" if thumb else "sample.jpg")).read_bytes()
                return self.send(200, "image/jpeg", body)
            if self.path.startswith("/th?id="):
                time.sleep(args.slow)
                thumb = "_400x240" in self.path
                body = (DATA / ("sample-thumb.jpg" if thumb else "sample.jpg")).read_bytes()
                return self.send(200, "image/jpeg", body)
            self.send(404, "text/plain", b"not found")

        def send(self, code, ctype, body):
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    server = http.server.ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    server.serve_forever()


if __name__ == "__main__":
    main()
