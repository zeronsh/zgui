"""Perf probe (not for merge): per-instruction counts inside memcmp, base
vs head, disassembled from libc (load base from memcmp's ifunc resolver)."""
import collections, re, subprocess, sys

def counts(path):
    """{function: Counter(address -> exclusive instructions)}, libc's path."""
    out = collections.defaultdict(collections.Counter)
    fn, obj, libc, skip = None, None, None, False
    for line in open(path, errors='replace'):
        line = line.rstrip('\n')
        if line.startswith('ob='):
            obj = line[3:]
        elif line.startswith('fn='):
            fn = line[3:]
            if 'libc.so' in (obj or ''):
                libc = obj
        elif line.startswith('calls='):
            skip = True  # the next cost line is the call's inclusive cost
        elif line.startswith('0x') and fn:
            if skip:
                skip = False
                continue
            parts = line.split()
            if len(parts) >= 2:
                out[fn][int(parts[0], 16)] += int(parts[1])
        elif line and line[0] in 'cj' and '=' in line[:6]:
            continue
    return out, libc

base, libc = counts(sys.argv[1])
head, _ = counts(sys.argv[2])
name = next(f for f in head if 'memcmp_avx2_movbe' in f)
b, h = base[name], head[name]
print(f'=== {name}: base {sum(b.values()):,} head {sum(h.values()):,}')
# The load base: the runtime entry of memcmp's resolver against its file address.
resolver = [f for f in head if re.search(r'(^|:)(memcmp|bcmp)$', f) and 'ifunc' in f]
nm = subprocess.run(['nm', '-D', '--defined-only', libc], capture_output=True, text=True).stdout
file_addr = {l.split()[2]: int(l.split()[0], 16) for l in nm.splitlines() if len(l.split()) == 3}
load = None
for f in resolver:
    sym = f.rsplit(':', 1)[-1]
    if sym in file_addr and head[f]:
        load = min(head[f]) - file_addr[sym]
        break
print('=== libc', libc, 'load', hex(load) if load is not None else None, 'resolvers', resolver)
grew = sorted((a for a in set(b) | set(h) if h[a] != b[a]), key=lambda a: h[a] - b[a], reverse=True)
for a in grew[:30]:
    print(f'{a:#x} (file {a - load:#x} ) base {b[a]:,} head {h[a]:,} {h[a]-b[a]:+,}' if load is not None else f'{a:#x} {b[a]} {h[a]}')
if load is not None:
    lo, hi = min(h) - load, max(h) - load + 16
    print('=== disassembly of the executed range')
    print(subprocess.run(['objdump', '-d', '--no-show-raw-insn', f'--start-address={lo:#x}', f'--stop-address={hi:#x}', libc],
                         capture_output=True, text=True).stdout[-30000:])
