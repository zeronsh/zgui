#!/usr/bin/env python3
"""Instructions by function, base against head, from two cachegrind files.

    python3 scripts/perf/cgdiff.py base.cg head.cg [--top 30]

What a cell of the instruction gate (`gate.py`) moved by, function by
function: the change in each function's instructions, largest first, and
the change summed by whose code it is (zgx's own, Luau's, the C library's
allocator and string functions, other Rust). The gate writes one for each
cell that grew (`ZGX_GATE_FUNCTIONS`); this reads any two files cachegrind
wrote with `--cachegrind-out-file`.

How to read one: zgx's own functions doing more is the change's work. A
function whose source the change did not touch, moving by a constant, is
its code compiled anew (another inlining, a register spilled). The C
library's `malloc`, `free`, `memcmp` and `memcpy` moving while zgx's
allocations are the same is where the heap put them (placement), as Luau's
collector moving is when it ran, and the arm64 linker's veneers
(`linker`) where it placed code.
"""
import argparse
import re
import sys

GROUPS = ('zgx', 'luau', 'libc', 'rust', 'linker', 'other')


def load(path):
    """Instructions (`Ir`) by function in a cachegrind output file, and the
    function's file."""
    functions, files = {}, {}
    name, file = None, '???'
    events = None
    with open(path) as lines:
        for line in lines:
            if line.startswith('events:'):
                events = line.split()[1:]
            elif line.startswith('fl='):
                file = line[3:].strip()
            elif line.startswith('fn='):
                name = re.sub(r'::h[0-9a-f]{16}$', '', line[3:].strip())
                files.setdefault(name, file)
            elif line.startswith(('fi=', 'fe=')):
                continue
            elif name is not None and line[:1].isdigit():
                counts = line.split()[1:]
                index = events.index('Ir') if events and 'Ir' in events else 0
                if index < len(counts):
                    functions[name] = functions.get(name, 0) + int(counts[index])
    return functions, files


def group(name, file):
    """Whose code a function is."""
    if 'zgx' in name:
        return 'zgx'
    if (re.match(r'(lua[A-Z_]|luau|Luau|lua_|luaL_)', name) or 'luau' in file.lower() or 'mlua' in name
            or re.search(r'\b(lua_State|lua_Page|GCObject|LuaTable|TString|Proto)\b', name)):
        return 'luau'
    if file.startswith('./') or file.startswith('/build/glibc') or re.match(r'_?_?(mem|str|malloc|free|calloc|realloc|_int_|unlink_chunk|sbrk)', name):
        return 'libc'
    # Veneers the arm64 linker adds (Cortex-A53 erratum 843419): where code
    # was placed, not what it does.
    if name.startswith('e843419@'):
        return 'linker'
    if '::' in name or name.startswith('<'):
        return 'rust'
    return 'other'


def diff(base_path, head_path, top=30):
    """The report: totals by group, then the functions that moved most."""
    (base, base_files), (head, head_files) = load(base_path), load(head_path)
    files = {**base_files, **head_files}
    rows = sorted(((head.get(f, 0) - base.get(f, 0), base.get(f, 0), f) for f in set(base) | set(head)),
                  key=lambda row: -abs(row[0]))
    by_group = dict.fromkeys(GROUPS, 0)
    for delta, _, name in rows:
        by_group[group(name, files.get(name, '???'))] += delta
    total = sum(head.values()) - sum(base.values())
    out = [f'total {total:+,} ({sum(base.values()):,} -> {sum(head.values()):,})',
           '  '.join(f'{g} {by_group[g]:+,}' for g in GROUPS)]
    for delta, before, name in rows[:top]:
        if delta:
            out.append(f'{delta:+12,} {before:14,}  [{group(name, files.get(name, "???"))}] {name[:150]}')
    return '\n'.join(out), by_group, total


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('base')
    parser.add_argument('head')
    parser.add_argument('--top', type=int, default=30)
    args = parser.parse_args()
    print(diff(args.base, args.head, args.top)[0])


if __name__ == '__main__':
    sys.exit(main())
