#!/bin/sh
# Renames zgui to zgx in this checkout: paths (`git mv`) and file contents,
# zgui/Zgui/ZGUI to zgx/Zgx/ZGX. Idempotent: run it again on a tree it has
# renamed and nothing changes.
#
#     scripts/rename-to-zgx.sh          # from the checkout's root
#
# A branch begun before the rename: rebase it onto main (git carries its
# changes to the files' new paths), run this, and commit what it renamed in
# your own commits (new files under an old path, new zgui identifiers).
#
# Left as they were:
# - docs/results and docs/platform-validation: recorded runs and evidence,
#   named as they ran (and paths into them, wherever they are cited);
# - vendor/: upstream code, but for its `zgui patch` comments and each crate's
#   ZGUI_PATCH.md (now ZGX_PATCH.md);
# - zgui-performance, the site's Cloudflare Pages project (its URL);
# - untracked files (apps/ and the like): update their path dependencies;
# - lines that name the old names on purpose (the deprecated aliases, the
#   changelog's migration notes): a line with `rename-to-zgx: keep`, or every
#   line from one with `rename-to-zgx: off` to one with `rename-to-zgx: on`.
set -eu
cd "$(git rev-parse --show-toplevel)"
exec python3 - "$@" <<'EOF'
import os, re, subprocess, sys

KEPT = ('docs/results/', 'docs/platform-validation/', 'apps/', 'scripts/rename-to-zgx.sh')
# Spans written as they are: paths into the recorded runs, the Pages project.
PROTECT = re.compile(r"(?:\.\./)*(?:docs/)?(?:results|platform-validation)/[^\s)\]\"'`>,;]*|zgui-performance")
NAME = re.compile(r'zgui|Zgui|ZGUI')
NEW = {'zgui': 'zgx', 'Zgui': 'Zgx', 'ZGUI': 'ZGX'}


def rename_span(text):
    out, at = [], 0
    for kept in PROTECT.finditer(text):
        out.append(NAME.sub(lambda m: NEW[m.group()], text[at:kept.start()]))
        out.append(kept.group())
        at = kept.end()
    out.append(NAME.sub(lambda m: NEW[m.group()], text[at:]))
    return ''.join(out)


def rename(text, line=None):
    """`text`'s lines renamed, but those kept on purpose (`rename-to-zgx:`);
    `line(text)` renames one line (all of it by default)."""
    line = line or rename_span
    lines, keep = text.split('\n'), False
    for i, text in enumerate(lines):
        if 'rename-to-zgx: off' in text:
            keep = True
        elif 'rename-to-zgx: on' in text:
            keep = False
        elif not keep and 'rename-to-zgx: keep' not in text:
            lines[i] = line(text)
    return '\n'.join(lines)


def vendored(path, text):
    """In vendored code, only comments: `//` (Rust, C, Metal), `#` (TOML)."""
    marker = '#' if path.endswith('.toml') else '//'

    def comment(line):
        at = line.find(marker)
        return line if at < 0 else line[:at] + rename_span(line[at:])
    return rename(text, comment)


files = subprocess.run(['git', 'ls-files', '-z'], capture_output=True, check=True).stdout.decode().split('\0')
moved = 0
for path in filter(None, files):
    if path.startswith(KEPT) or not os.path.isfile(path):
        continue
    if path.startswith('vendor/'):
        new = path.replace('ZGUI_PATCH.md', 'ZGX_PATCH.md')
    else:
        new = NAME.sub(lambda m: NEW[m.group()], path)
    if new != path:
        if os.path.exists(new):
            sys.exit(f'{path}: {new} exists already')
        os.makedirs(os.path.dirname(new) or '.', exist_ok=True)
        subprocess.run(['git', 'mv', path, new], check=True)
        moved += 1
    with open(new, 'rb') as f:
        data = f.read()
    if b'\0' in data[:8192] or not NAME.search(data.decode('utf-8', 'replace')):
        continue
    try:
        text = data.decode('utf-8')
    except UnicodeDecodeError:
        continue
    vendor_code = new.startswith('vendor/') and not new.endswith('ZGX_PATCH.md')
    renamed = vendored(new, text) if vendor_code else rename(text)
    if renamed != text:
        with open(new, 'w', encoding='utf-8', newline='') as f:
            f.write(renamed)
print(f'rename-to-zgx: {moved} paths moved', file=sys.stderr)
EOF
