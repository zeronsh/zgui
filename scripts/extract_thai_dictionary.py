#!/usr/bin/env python3
"""Extract ICU4X's Thai word dictionary for zgx's `cjk-thai-words` feature.

ICU4X's compiled data keeps the Thai dictionary in one table with the
Burmese, Khmer and Lao ones; linking that table links all four. zgx takes
Chinese and Japanese from the compiled data and Thai from the file this
writes, `crates/zgx/data/thaidict.bin`: the dictionary's alphabet (a code
point inversion list: a little-endian `u32` count, then that many `u32`s),
then its trie (a `ZeroTrieSimpleAscii`'s store), as
`DictionaryBreakData::ZeroTrie` holds them.

    python3 scripts/extract_thai_dictionary.py \
        ~/.cargo/registry/src/*/icu_segmenter_data-2.4.0

Run it again when `icu_segmenter` moves to a new minor version, from that
version's `icu_segmenter_data`. The data is Unicode's, under the Unicode
License v3 (`crates/zgx/data/LICENSE-UNICODE`).
"""
import pathlib
import re
import struct
import sys

ESCAPE = re.compile(rb"\\(x[0-9A-Fa-f]{2}|.)", re.S)
SIMPLE = {b"0": 0, b"n": 10, b"r": 13, b"t": 9, b"\\": 92, b'"': 34, b"'": 39}


def decode(literal: bytes) -> bytes:
    """The bytes a Rust byte string literal's body (between the quotes) holds."""
    out = bytearray()
    at = 0
    for match in ESCAPE.finditer(literal):
        out += literal[at : match.start()]
        code = match.group(1)
        if code[:1] == b"x":
            out.append(int(code[1:], 16))
        elif code in SIMPLE:
            out.append(SIMPLE[code])
        elif code == b"\n":
            # A line continuation: the break and the next line's indent go.
            rest = literal[match.end() :]
            at = match.end() + len(rest) - len(rest.lstrip())
            continue
        else:
            raise SystemExit(f"unknown escape \\{code!r}")
        at = match.end()
    out += literal[at:]
    return bytes(out)


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    source = pathlib.Path(sys.argv[1]) / "data" / "segmenter_dictionary_extended_v2.rs.data"
    data = source.read_bytes()
    # The lookup's trie names the dictionaries in order: Burmese, Khmer, Lao,
    # Thai (`\x80` to `\x83`); VALUES holds them in that order.
    store = re.search(rb'store\s*:\s*b"((?:[^"\\]|\\.)*)"', data, re.S)
    names = decode(store.group(1))
    if not names.endswith(b"haidict\x83"):
        raise SystemExit(f"Thai is not the fourth dictionary: {names!r}")
    values = data[data.index(b"const VALUES") :]
    entries = values.split(b"DictionaryBreakData::")[1:]
    if len(entries) != 4:
        raise SystemExit(f"expected four dictionaries, found {len(entries)}")
    thai = entries[3]
    if not thai.startswith(b"ZeroTrie"):
        raise SystemExit("Thai's dictionary is not a ZeroTrie with an alphabet")
    literal = rb'b"((?:[^"\\]|\\.)*)"'
    alphabet = re.search(rb"from_parts_unchecked\s*\(.*?" + literal + rb".*?(\d+)u32", thai, re.S)
    points = decode(alphabet.group(1))
    # Potential code points: three little-endian bytes each.
    inversion = [int.from_bytes(points[i : i + 3], "little") for i in range(0, len(points), 3)]
    size = sum(end - start for start, end in zip(inversion[::2], inversion[1::2]))
    if len(points) % 3 or len(inversion) % 2 or size != int(alphabet.group(2)):
        raise SystemExit(f"a malformed alphabet: {inversion} for {alphabet.group(2)} characters")
    trie = decode(re.search(rb"ZeroTrieSimpleAscii\s*\{\s*store\s*:.*?" + literal, thai, re.S).group(1))
    out = pathlib.Path(__file__).resolve().parent.parent / "crates" / "zgx" / "data" / "thaidict.bin"
    out.parent.mkdir(exist_ok=True)
    header = struct.pack(f"<I{len(inversion)}I", len(inversion), *inversion)
    out.write_bytes(header + trie)
    print(f"{out}: alphabet {[hex(p) for p in inversion]}, trie {len(trie)} bytes")


if __name__ == "__main__":
    main()
