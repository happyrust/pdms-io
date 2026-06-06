#!/usr/bin/env python3
"""Offline experiment: where does an attribute's *element offset* come from?

We already know (attlib_atnain_probe.py):
  - noun  ATGTIX (v47[6]): noun_hash  -> (record, disp)   e.g. WELD idx82 rec2225 disp1
  - attr  ATGTIX (v47[2]): attr_hash  -> (record, disp)   e.g. POS  idx27 rec1129 disp127

Measured (real elements): WELD.POS sits at record word13.
  record layout: w0=count,w1-2=refno,w3=noun,w4-5=owner,w6=page_no, then implicit payload.
  If the implicit payload starts at w7, POS@w13 => payload word offset 6.

This script dumps the candidate matrix rows and looks for a small integer (6 or 13)
that could be the per-noun offset of POS, with NO IDA dependency.
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
    value = 0
    mul = 1
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
        digit = value % 27
        out.append(" " if digit == 0 else chr(digit + 64))
        value //= 27
    return "".join(out)


class Attlib:
    def __init__(self, path: Path):
        self.data = path.read_bytes()
        self.records = len(self.data) // PAGE_SIZE

    def words(self, record_1based: int) -> tuple[int, ...]:
        page = record_1based - 1
        off = page * PAGE_SIZE
        return struct.unpack_from(">512I", self.data, off)

    def directory(self) -> tuple[int, ...]:
        return self.words(2)[:8]

    def parse_atgtix(self, start_record: int) -> list[tuple[int, int, int]]:
        """Return list of (hash, record, disp), 1-based index = position."""
        out: list[tuple[int, int, int]] = []
        record = start_record
        while True:
            words = self.words(record)
            i = 0
            advanced = False
            while i < PAGE_WORDS:
                w = words[i]
                if HASH_LOW <= w <= HASH_HIGH:
                    combined = words[i + 1]
                    out.append((w, combined // PAGE_WORDS, combined % PAGE_WORDS))
                    i += 2
                    continue
                if w == 0:
                    record += 1
                    advanced = True
                    break
                return out
            if not advanced:
                return out


def find(entries: list[tuple[int, int, int]], name: str):
    target = db1_hash(name)
    for idx, (h, rec, disp) in enumerate(entries, start=1):
        if h == target:
            return idx, rec, disp
    return None


def main() -> None:
    path = Path(sys.argv[1] if len(sys.argv) > 1 else "test-file/attlib.dat")
    lib = Attlib(path)
    directory = lib.directory()
    attr_ix = lib.parse_atgtix(directory[2])
    noun_ix = lib.parse_atgtix(directory[6])
    print(f"file={path} records={lib.records}")
    print("directory=" + " ".join(f"0x{x:X}" for x in directory))
    print(f"attr_index n={len(attr_ix)} noun_index n={len(noun_ix)}")

    weld = find(noun_ix, "WELD")
    pos = find(attr_ix, "POS")
    print(f"WELD noun -> idx/rec/disp = {weld}")
    print(f"POS  attr -> idx/rec/disp = {pos}")

    weld_idx, weld_rec, weld_disp = weld
    pos_idx, pos_rec, pos_disp = pos

    # Hypothesis A: offset lives on the ATTRIBUTE record page (POS row),
    # column selected by noun_index.
    print("\n[A] POS attribute record page row, indexed by noun_index candidates:")
    page = lib.words(pos_rec)
    for ni in (weld_idx, weld_idx - 1, weld_idx + 1):
        for k in (-2, -1, 0, 1):
            col = pos_disp + ni + k
            if 0 <= col < PAGE_WORDS:
                print(f"  page[{pos_rec}][disp({pos_disp})+ni({ni}){k:+d}={col}] = {page[col]}  (0x{page[col]:X})")

    # Hypothesis B: offset lives on the NOUN record page (WELD row),
    # column selected by attr_index.
    print("\n[B] WELD noun record page row, indexed by attr_index candidates:")
    page = lib.words(weld_rec)
    for ai in (pos_idx, pos_idx - 1, pos_idx + 1):
        for k in (-2, -1, 0, 1):
            col = weld_disp + ai + k
            if 0 <= col < PAGE_WORDS:
                print(f"  page[{weld_rec}][disp({weld_disp})+ai({ai}){k:+d}={col}] = {page[col]}  (0x{page[col]:X})")

    # Raw windows for manual inspection: look for 6 / 13 small ints.
    def window(rec: int, disp: int, span: int, label: str):
        page = lib.words(rec)
        print(f"\n[{label}] record {rec} words[{disp}..{disp+span}] (looking for small ints 1..50):")
        for i in range(disp, min(disp + span, PAGE_WORDS)):
            v = page[i]
            tag = ""
            if 1 <= v <= 50:
                tag = "  <-- small"
            if v == 6 or v == 13:
                tag = "  <== candidate offset"
            name = db1_dehash(v).strip() if HASH_LOW <= v <= HASH_HIGH else ""
            namestr = f"  ~{name}" if name else ""
            print(f"  [{i:3d}] {v:11d} 0x{v:08X}{namestr}{tag}")

    window(pos_rec, max(0, pos_disp - 2), 130, "POS attr page window")
    window(weld_rec, max(0, weld_disp - 2), 130, "WELD noun page window")


if __name__ == "__main__":
    main()
