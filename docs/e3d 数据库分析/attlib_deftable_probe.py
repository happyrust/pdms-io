#!/usr/bin/env python3
"""Decode the dense triplet table at directory v47[3] (record 0x6A8).

Layout per record: [hash, mid, small] * ~170, stride 3.
Hypothesis: this is the global attribute descriptor table
  (attr_hash, definition_pointer/offset, type_or_size).

We cross-check the third column against KNOWN attribute TYPE values from
attlib_atnain_probe (POS=8, ORI=9, ISPE=5, MTOC=6, SPRE=5, TSPE=5)
and print the full triplet for a set of probe attributes.
"""
from __future__ import annotations

import struct
import sys
from pathlib import Path

BASE27_OFFSET = 0x81BF1
HASH_LOW = 531_442
HASH_HIGH = 387_951_929
PAGE_WORDS = 512
PAGE_SIZE = PAGE_WORDS * 4


def db1_hash(name: str) -> int:
    value, mul = 0, 1
    for ch in name.upper():
        digit = 0 if ch == " " else ord(ch) - 64
        value += digit * mul
        mul *= 27
    return value + BASE27_OFFSET


def db1_dehash(value: int) -> str:
    if value <= BASE27_OFFSET:
        return ""
    value -= BASE27_OFFSET
    out = []
    while value > 0:
        d = value % 27
        out.append(" " if d == 0 else chr(d + 64))
        value //= 27
    return "".join(out)


class Attlib:
    def __init__(self, path: Path):
        self.data = path.read_bytes()
        self.records = len(self.data) // PAGE_SIZE

    def words(self, record_1based: int) -> tuple[int, ...]:
        off = (record_1based - 1) * PAGE_SIZE
        return struct.unpack_from(">512I", self.data, off)

    def directory(self) -> tuple[int, ...]:
        return self.words(2)[:8]


def parse_triplets(lib: Attlib, start_record: int, max_records: int = 40):
    """Parse stride-3 triplets until a record stops looking like the table."""
    out = {}
    rec = start_record
    for _ in range(max_records):
        words = lib.words(rec)
        n_hash = sum(1 for w in words if HASH_LOW <= w <= HASH_HIGH)
        if n_hash < 80:  # table records are dense (~170); stop otherwise
            break
        i = 0
        while i + 2 < PAGE_WORDS:
            h = words[i]
            if HASH_LOW <= h <= HASH_HIGH:
                out.setdefault(h, (rec, i, words[i + 1], words[i + 2]))
                i += 3
            else:
                i += 1
        rec += 1
    return out, rec - start_record


def main() -> None:
    path = Path(sys.argv[1] if len(sys.argv) > 1 else "test-file/attlib.dat")
    lib = Attlib(path)
    directory = lib.directory()
    seg = directory[3]
    print(f"file={path} records={lib.records}")
    print(f"segment v47[3]=0x{seg:X} (record {seg})")

    table, n_rec = parse_triplets(lib, seg)
    print(f"parsed {len(table)} triplets across {n_rec} records")

    known_type = {
        "POS": 8, "ORI": 9, "ISPE": 5, "MTOC": 6, "SPRE": 5, "TSPE": 5,
        "ANGL": None, "BORE": None, "NAME": None, "DESC": None, "HEIG": None,
        "WLDN": None, "PTNO": None, "MTOT": None, "LEAV": None, "ARRI": None,
    }
    print("\nattr triplets (hash, mid, small):")
    for name, ty in known_type.items():
        h = db1_hash(name)
        rec_i = table.get(h)
        if rec_i is None:
            print(f"  {name:6s} 0x{h:08X}  NOT FOUND in table")
            continue
        rec, idx, mid, small = rec_i
        flag = ""
        if ty is not None:
            flag = "  TYPE-match" if small == ty else f"  (probe TYPE={ty}, small={small})"
        print(f"  {name:6s} 0x{h:08X}  rec={rec} idx={idx:3d} mid={mid} (0x{mid:X}) small={small}{flag}")

    # Distribution of the 'small' column to guess its meaning
    from collections import Counter
    dist = Counter(v[3] for v in table.values())
    print("\n'small' column distribution:", dict(sorted(dist.items())))
    dist2 = Counter(v[2] for v in table.values())
    print("'mid' distinct count:", len(dist2), "top:", dist2.most_common(8))


if __name__ == "__main__":
    main()
