#!/usr/bin/env python3
"""Locate every occurrence of given attribute/noun hashes across all attlib
records, and show the surrounding context. Decisive test for *where* the
per-noun attribute layout (if any) references POS/ANGL/etc.
"""
from __future__ import annotations

import struct
import sys
from pathlib import Path

BASE27_OFFSET = 0x81BF1
PAGE_WORDS = 512
PAGE_SIZE = PAGE_WORDS * 4


def db1_hash(name: str) -> int:
    value, mul = 0, 1
    for ch in name.upper():
        digit = 0 if ch == " " else ord(ch) - 64
        value += digit * mul
        mul *= 27
    return value + BASE27_OFFSET


def main() -> None:
    path = Path(sys.argv[1] if len(sys.argv) > 1 else "test-file/attlib.dat")
    names = sys.argv[2:] or ["POS", "ANGL", "WELD"]
    data = path.read_bytes()
    n_records = len(data) // PAGE_SIZE

    targets = {db1_hash(n): n for n in names}
    print(f"file={path} records={n_records}")
    for h, n in targets.items():
        print(f"  target {n} = 0x{h:X}")

    # directory for segment boundaries
    directory = struct.unpack_from(">8I", data, 1 * PAGE_SIZE)
    seg_names = {}
    for i, rec in enumerate(directory):
        seg_names[rec] = f"v47[{i}]"
    print("directory=" + " ".join(f"0x{x:X}" for x in directory))

    def seg_for(rec_1based: int) -> str:
        best = None
        for i, start in enumerate(directory):
            if rec_1based >= start and (best is None or start > directory[best]):
                best = i
        return f"v47[{best}]" if best is not None else "?"

    hits = {n: 0 for n in names}
    for rec in range(1, n_records + 1):
        off = (rec - 1) * PAGE_SIZE
        words = struct.unpack_from(">512I", data, off)
        for i, w in enumerate(words):
            if w in targets:
                name = targets[w]
                hits[name] += 1
                if hits[name] <= 12:
                    lo = max(0, i - 3)
                    hi = min(PAGE_WORDS, i + 5)
                    ctx = " ".join(f"{words[j]}" for j in range(lo, hi))
                    print(f"  {name} @ rec{rec}({seg_for(rec)}) idx{i}  ctx[{lo}:{hi}]= {ctx}")

    print("\ntotal hits:", hits)


if __name__ == "__main__":
    main()
