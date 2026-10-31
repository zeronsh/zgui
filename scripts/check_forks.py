#!/usr/bin/env python3
"""A fork's version on crates.io is what vendor/ holds at that version.

zgx's crates build vendor/ here and crates.io's forks there, so a fork changed
without a version passes every build in this repository, and the next
release's crates fail to compile for everyone who installs them (#1026, #1030:
release_check.sh patches vendor/ over crates.io, which hides it too).

For each fork (a vendor/ crate named zgx-*) whose version crates.io has, this
compares what `cargo package` would put in it with what crates.io published. A
version crates.io doesn't have yet passes: it goes up first at the release.

    python3 scripts/check_forks.py              # any failure fails
    python3 scripts/check_forks.py --offline-ok # crates.io unreachable passes

CI takes the second: an outage there shouldn't fail every pull request. The
release takes the first.
"""

import hashlib
import io
import json
import pathlib
import re
import subprocess
import sys
import tarfile
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
AGENT = {"User-Agent": "zgx-release-check"}
# What cargo writes into a package: they vary with cargo, not with the fork.
# The manifest is compared as written, Cargo.toml.orig.
GENERATED = {".cargo_vcs_info.json", "Cargo.toml", "Cargo.lock"}


class Unreachable(Exception):
    pass


def fetch(url, missing_ok=False):
    last = None
    for _ in range(3):
        try:
            request = urllib.request.Request(url, headers=AGENT)
            with urllib.request.urlopen(request, timeout=30) as response:
                return response.read()
        except urllib.error.HTTPError as error:
            if missing_ok and error.code == 404:
                return None
            last = error
        except OSError as error:
            last = error
    raise Unreachable(f"{url}: {last}")


def manifest_field(text, field):
    package = text.split("[package]", 1)[1].split("\n[", 1)[0]
    return re.search(rf'^{field} = "([^"]+)"', package, re.M).group(1)


def forks():
    for manifest in sorted(ROOT.glob("vendor/*/Cargo.toml")):
        text = manifest.read_text(encoding="utf-8")
        if "[package]" in text and manifest_field(text, "name").startswith("zgx-"):
            yield manifest.parent, manifest_field(text, "name"), manifest_field(text, "version")


def published_checksum(name, version):
    """The sha256 of `version` of `name` on crates.io, None if it has none."""
    index = fetch(f"https://index.crates.io/{name[:2]}/{name[2:4]}/{name}", missing_ok=True)
    for line in (index or b"").splitlines():
        entry = json.loads(line)
        if entry["vers"] == version:
            return entry["cksum"]
    return None


def packaged(directory):
    """What `cargo package` would put in the fork in `directory`."""
    listing = subprocess.run(
        ["cargo", "package", "--list", "--allow-dirty", "--manifest-path", directory / "Cargo.toml"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if listing.returncode != 0:
        sys.exit(f"cargo package --list, {directory}:\n{listing.stderr}")
    return {line.strip().replace("\\", "/") for line in listing.stdout.splitlines() if line.strip()}


def lines(data):
    # A Windows checkout has CRLF where the package has LF.
    return data.replace(b"\r\n", b"\n")


def differences(directory, name, version, crate):
    prefix = f"{name}-{version}/"
    with tarfile.open(fileobj=io.BytesIO(crate), mode="r:gz") as archive:
        published = {
            member.name[len(prefix):]: archive.extractfile(member).read()
            for member in archive.getmembers()
            if member.isfile()
        }
    here = packaged(directory)
    found = []
    for path in sorted((set(published) | here) - GENERATED):
        on_disk = directory / ("Cargo.toml" if path == "Cargo.toml.orig" else path)
        if path not in published:
            found.append(f"{path} (new)")
        elif path not in here:
            found.append(f"{path} (gone)")
        elif lines(on_disk.read_bytes()) != lines(published[path]):
            found.append(path)
    return found


def main():
    offline_ok = "--offline-ok" in sys.argv[1:]
    failed = False
    for directory, name, version in forks():
        try:
            checksum = published_checksum(name, version)
            if checksum is None:
                print(f"{name} {version}: not on crates.io yet, the release publishes it")
                continue
            crate = fetch(f"https://static.crates.io/crates/{name}/{name}-{version}.crate")
        except Unreachable as error:
            if offline_ok:
                print(f"{name} {version}: not checked, crates.io unreachable ({error})")
                continue
            sys.exit(f"crates.io unreachable: {error}")
        if hashlib.sha256(crate).hexdigest() != checksum:
            sys.exit(f"{name} {version}: the download isn't what the index says it is")
        found = differences(directory, name, version, crate)
        if found:
            failed = True
            print(f"{name} {version}: {directory.relative_to(ROOT).as_posix()} isn't what crates.io has:")
            for path in found:
                print(f"    {path}")
        else:
            print(f"{name} {version}: as crates.io has it")
    if failed:
        sys.exit(
            "A fork's change needs a version of its own, and what calls it that version's "
            "requirement: crates.io's copy is what zgx's users build (docs/releasing.md)."
        )


if __name__ == "__main__":
    main()
