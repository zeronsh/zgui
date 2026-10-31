#!/usr/bin/env python3
"""CHANGELOG.md's unreleased entries, one file each in changelog.d/.

Two pull requests adding lines to the same section of CHANGELOG.md conflict,
and each conflict costs a rebase and a CI run. So an entry is a file of its
own, `changelog.d/<pr-or-slug>.md`, which nothing else touches:

    ---
    section: fixed
    ---
    - **A press outside a popover closes it** (zgx-components): ...

`section` is one of: changed (what to check in an existing app), added,
performance, fixed. The body is the entry, as CHANGELOG.md would have it.

    scripts/changelog.py check [--base REF] [--release VERSION]
        Every entry is well formed. With --base, CHANGELOG.md's Unreleased
        section is unchanged since REF (CI's rule: write an entry instead),
        unless the change makes a release. With --release, a CHANGELOG.md
        that has VERSION's section has no entry left to assemble.
    scripts/changelog.py render
        CHANGELOG.md with the entries in its Unreleased section, on stdout:
        what the site shows.
    scripts/changelog.py assemble [--version VERSION --date YYYY-MM-DD]
        Writes the entries into CHANGELOG.md and deletes them; with a
        version, Unreleased becomes that version's section (a fragment
        landing later starts a new Unreleased). The release commit does this.
"""
import argparse
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
CHANGELOG = ROOT / "CHANGELOG.md"
ENTRIES = ROOT / "changelog.d"
# A section's key and heading, in the order a release lists them.
SECTIONS = {
    "changed": "### Changed: what to check in an existing app",
    "added": "### Added",
    "performance": "### Performance",
    "fixed": "### Fixed",
}
UNRELEASED = "## Unreleased"


class Invalid(Exception):
    pass


def entries():
    """[(path, section key, body)], by name: README.md is no entry."""
    found = []
    for path in sorted(ENTRIES.glob("*.md")) if ENTRIES.is_dir() else []:
        if path.name == "README.md":
            continue
        text = path.read_text(encoding="utf-8").replace("\r\n", "\n")
        match = re.match(r"---\n(.*?)\n---\n(.*)", text, re.S)
        if not match:
            raise Invalid(f"{path.name}: starts with front matter (---, section: …, ---)")
        fields = dict(line.split(":", 1) for line in match[1].splitlines() if ":" in line)
        section = fields.get("section", "").strip().lower()
        if section not in SECTIONS:
            raise Invalid(f"{path.name}: section is one of {', '.join(SECTIONS)}, not {section!r}")
        body = match[2].strip("\n")
        if not body.startswith("- "):
            raise Invalid(f"{path.name}: the entry is a list item, starting `- `")
        found.append((path, section, body))
    return found


def split(text):
    """(before Unreleased's body, its body, from the next release on)."""
    start = text.find(UNRELEASED + "\n")
    if start < 0:
        return None
    body = start + len(UNRELEASED) + 1
    end = text.find("\n## ", body)
    end = len(text) if end < 0 else end + 1
    return text[:body], text[body:end], text[end:]


def merged(body, found):
    """Unreleased's body with `found` in their sections: each section's new
    entries first, a missing section made where the order puts it."""
    parts = re.split(r"(?m)^(### .*)\n", body)
    intro, sections = parts[0], dict(zip(parts[1::2], parts[2::2]))
    for key in reversed(SECTIONS):
        heading = SECTIONS[key]
        new = "\n".join(entry for _, section, entry in found if section == key)
        if not new:
            continue
        old = sections.get(heading, "\n").lstrip("\n")
        sections[heading] = "\n" + new + "\n" + old
    order = [h for h in SECTIONS.values() if h in sections] + [h for h in sections if h not in SECTIONS.values()]
    out = intro if intro.strip() else "\n"
    for heading in order:
        text = sections[heading].rstrip("\n") + "\n\n"
        out += heading + "\n" + (text if text.startswith("\n") else "\n" + text)
    return out


def render(text, found):
    if not found:
        return text
    if split(text) is None:
        # Just after a release: the entries start the next one.
        first = text.find("\n## ")
        if first < 0:
            raise Invalid("CHANGELOG.md has no release section to put Unreleased before")
        text = text[:first + 1] + UNRELEASED + "\n\n" + text[first + 1:]
    before, body, after = split(text)
    return before + merged(body, found) + after


def unreleased(text):
    parts = split(text)
    return parts[1].strip() if parts else ""


def releases(text):
    return set(re.findall(r"(?m)^## (\d+\.\d+\.\d+) \(", text))


def check(args):
    found = entries()
    text = CHANGELOG.read_text(encoding="utf-8")
    render(text, found)
    if args.base:
        old = subprocess.run(["git", "show", f"{args.base}:CHANGELOG.md"], cwd=ROOT,
                             capture_output=True, text=True, check=True).stdout
        if unreleased(old) != unreleased(text) and not releases(text) - releases(old):
            raise Invalid(
                "CHANGELOG.md's Unreleased section changed: an entry goes in a file of its own, "
                "changelog.d/<pr-or-slug>.md (scripts/changelog.py says how), so pull requests "
                "don't conflict over it. To correct an entry already there, label the pull "
                "request changelog-edit.")
    if args.release and args.release in releases(text) and found:
        raise Invalid(f"{len(found)} entries in changelog.d/ aren't in {args.release}'s section: "
                      "scripts/changelog.py assemble, in the release commit")
    print(f"changelog: {len(found)} entries, well formed")


def assemble(args):
    found = entries()
    text = render(CHANGELOG.read_text(encoding="utf-8"), found)
    if args.version:
        if not args.date:
            raise Invalid("--version needs --date")
        if split(text) is None:
            raise Invalid("nothing unreleased to make a release of")
        text = text.replace(UNRELEASED + "\n", f"## {args.version} ({args.date})\n", 1)
    CHANGELOG.write_text(text, encoding="utf-8")
    for path, _, _ in found:
        path.unlink()
    print(f"changelog: {len(found)} entries assembled into CHANGELOG.md")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    checking = commands.add_parser("check")
    checking.add_argument("--base")
    checking.add_argument("--release")
    commands.add_parser("render")
    assembling = commands.add_parser("assemble")
    assembling.add_argument("--version")
    assembling.add_argument("--date")
    args = parser.parse_args()
    try:
        if args.command == "check":
            check(args)
        elif args.command == "render":
            sys.stdout.write(render(CHANGELOG.read_text(encoding="utf-8"), entries()))
        else:
            assemble(args)
    except Invalid as error:
        sys.exit(f"changelog: {error}")


if __name__ == "__main__":
    main()
