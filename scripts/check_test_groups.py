#!/usr/bin/env python3
"""Every integration test file builds into a test binary.

zgui and zgui-script turn Cargo's automatic tests off (`autotests = false`)
and build one binary per area: a group's `main.rs` lists its files as
modules (`#[path = "../x.rs"] mod x;`). A file under `tests/` that no group
lists, and that is no `[[test]]` of its own, silently never runs. This
follows each `[[test]]` target's modules as rustc resolves them (`mod x;`,
`#[path]`) and fails on any `tests/**/*.rs` they never reach, naming where
to list it.

    python3 scripts/check_test_groups.py
"""
import pathlib
import re
import sys
import tomllib

CRATES = ["crates/zgui", "crates/zgui-script"]
# A module declared from a file: `mod x;`, with any attributes before it.
DECLARATION = re.compile(
    r"((?:#\[[^\]]*\]\s*)*)(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;")
PATH = re.compile(r'#\[\s*path\s*=\s*"([^"]+)"\s*\]')


def strip_comments(source):
    source = re.sub(r"/\*.*?\*/", "", source, flags=re.S)
    return re.sub(r"//[^\n]*", "", source)


def reached(root):
    """Every file the module tree rooted at `root` includes."""
    # Each file, and whether its own `mod x;` looks beside it: a crate
    # root's, a mod.rs's and a `#[path]` file's do (rustc treats the last
    # as a mod.rs); another file's look in the directory named after it.
    seen, pending = set(), [(root, True)]
    while pending:
        file, beside = pending.pop()
        if file in seen or not file.is_file():
            continue
        seen.add(file)
        source = strip_comments(file.read_text())
        base = file.parent if beside or file.name == "mod.rs" else file.parent / file.stem
        for attributes, name in DECLARATION.findall(source):
            path = PATH.search(attributes)
            if path:
                pending.append(((file.parent / path.group(1)).resolve(), True))
            else:
                pending.append(((base / f"{name}.rs").resolve(), False))
                pending.append(((base / name / "mod.rs").resolve(), True))
    return seen


def main():
    missing = []
    for crate in map(pathlib.Path, CRATES):
        manifest = tomllib.loads((crate / "Cargo.toml").read_text())
        targets = [(crate / test["path"]).resolve() for test in manifest.get("test", [])]
        included = set()
        for target in targets:
            included |= reached(target)
        groups = sorted(t.relative_to(pathlib.Path.cwd()) for t in targets if t.name == "main.rs")
        for file in sorted((crate / "tests").rglob("*.rs")):
            if file.resolve() not in included:
                missing.append((file, crate, groups))
    for file, crate, groups in missing:
        print(f"{file}: in no test binary, so it never runs. List it in the group for its area "
              f"(`#[path = \"../{file.name}\"] mod {file.stem};` in one of "
              f"{', '.join(map(str, groups))}), or give it a [[test]] in {crate}/Cargo.toml.",
              file=sys.stderr)
    if missing:
        sys.exit(1)
    print(f"every test file of {', '.join(CRATES)} builds into a test binary")


if __name__ == "__main__":
    main()
