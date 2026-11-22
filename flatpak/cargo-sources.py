#!/usr/bin/env python3
"""Turn Cargo.lock into Flatpak sources, so flatpak-builder can build offline.

Every crate from crates.io becomes an archive source unpacked into
cargo/vendor/<name>-<version>, plus the checksum file Cargo expects there.
A cargo config replaces crates.io with that vendor directory.

Usage: flatpak/cargo-sources.py Cargo.lock > flatpak/cargo-sources.json
"""

import json
import sys
import tomllib

CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"


def main(lock_path: str) -> None:
    with open(lock_path, "rb") as f:
        lock = tomllib.load(f)

    sources = []
    for package in lock.get("package", []):
        source = package.get("source")
        if source is None:
            continue  # a crate of this workspace
        if source != CRATES_IO:
            sys.exit(f"unsupported source for {package['name']}: {source}")
        name, version, checksum = package["name"], package["version"], package["checksum"]
        dest = f"cargo/vendor/{name}-{version}"
        sources.append(
            {
                "type": "archive",
                "archive-type": "tar-gzip",
                "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
                "sha256": checksum,
                "dest": dest,
            }
        )
        sources.append(
            {
                "type": "inline",
                "contents": json.dumps({"package": checksum, "files": {}}),
                "dest": dest,
                "dest-filename": ".cargo-checksum.json",
            }
        )

    sources.append(
        {
            "type": "inline",
            "contents": (
                '[source.vendored-sources]\ndirectory = "cargo/vendor"\n\n'
                '[source.crates-io]\nreplace-with = "vendored-sources"\n'
            ),
            "dest": "cargo",
            "dest-filename": "config.toml",
        }
    )
    json.dump(sources, sys.stdout, indent=2)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "Cargo.lock")
