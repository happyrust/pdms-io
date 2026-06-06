#!/usr/bin/env python3
"""Classify the 8 attlib directory segments and hunt for a per-noun attribute
*layout/offset* table (the element-type-definition data db4_get_att_dets reads).

directory (record 2, first 8 u32s), measured on test-file/attlib.dat:
  [0]=0x3   [1]=0x4    [2]=0x693 [3]=0x6A8
  [4]=0x6CD [5]=0x6CE  [6]=0x8BC [7]=0x8C2

We already mapped: [0]=attr ATGTDF, [2]=attr ATGTIX, [4]=noun ATGTDF, [6]=noun ATGTIX.
This script samples [1],[3],[5],[7] and looks for WELD's row + the value 6
(POS payload word offset measured from real elements).
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


def classify(lib: Attlib, start_record: int, label: str, span_records: int = 2) -> None:
    print(f"\n===== segment {label} start_record={start_record} (0x{start_record:X}) =====")
    for r in range(start_record, start_record + span_records):
        words = lib.words(r)
        n_hash = sum(1 for w in words if HASH_LOW <= w <= HASH_HIGH)
        n_ff = sum(1 for w in words if w == 0xFFFFFFFF)
        n_zero = sum(1 for w in words if w == 0)
        n_small = sum(1 for w in words if 1 <= w <= 64)
        print(f"  rec {r}: hashes={n_hash} ff={n_ff} zero={n_zero} small(1..64)={n_small}")
        # show first 24 words with dehash annotation
        for i in range(24):
            w = words[i]
            name = db1_dehash(w).strip() if HASH_LOW <= w <= HASH_HIGH else ""
            ann = f"  ~{name}" if name else ""
            print(f"    [{i:3d}] {w:11d} 0x{w:08X}{ann}")


def hunt_weld_six(lib: Attlib, start_record: int, label: str, span_records: int = 6) -> None:
    """Look for WELD hash followed-by / near the value 6, in a segment."""
    weld = db1_hash("WELD")
    pos = db1_hash("POS")
    print(f"\n----- hunt in {label} (WELD=0x{weld:X} POS=0x{pos:X}) -----")
    for r in range(start_record, start_record + span_records):
        words = lib.words(r)
        for i, w in enumerate(words):
            if w in (weld, pos):
                lo = max(0, i - 2)
                hi = min(PAGE_WORDS, i + 8)
                ctx = " ".join(f"{words[j]}" for j in range(lo, hi))
                print(f"  rec {r} [{i}] {db1_dehash(w).strip()} ctx[{lo}..{hi}]: {ctx}")


def main() -> None:
    path = Path(sys.argv[1] if len(sys.argv) > 1 else "test-file/attlib.dat")
    lib = Attlib(path)
    directory = lib.directory()
    print(f"file={path} records={lib.records}")
    print("directory=" + " ".join(f"0x{x:X}" for x in directory))

    for idx in (1, 3, 5, 7):
        classify(lib, directory[idx], f"v47[{idx}]=0x{directory[idx]:X}")

    for idx in (1, 3, 5, 7):
        hunt_weld_six(lib, directory[idx], f"v47[{idx}]=0x{directory[idx]:X}")


if __name__ == "__main__":
    main()
