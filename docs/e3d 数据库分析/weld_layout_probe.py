#!/usr/bin/env python3
"""Empirically reconstruct the WELD implicit-region layout (IDA-free).

Mechanism is now binary-confirmed (format spec 7.6):
  value(attr) = element_record[offset], offset = word index from RECORD START.
  header = w0..w6 (count, refno0/1, noun, owner0/1, page_no).

We collect several WELD records from a sample db and align their implicit words.
Words that are CONSTANT across instances are structural/bitfield/enum defaults;
words that VARY are per-instance attribute values (POS/ORI doubles, etc.).
This yields an evidence-based layout without needing the runtime type-def.
"""
from __future__ import annotations

import struct
import sys
from pathlib import Path

BASE = 0x81BF1
INDEX_NOUN = 0xCC47DF


def dehash(h: int) -> str:
    if h <= BASE:
        return ""
    k = h - BASE
    s = ""
    while k > 0:
        d = k % 27
        s += " " if d == 0 else chr(d + 64)
        k //= 27
    return s


def main() -> None:
    path = Path(sys.argv[1] if len(sys.argv) > 1 else "pdms-test-data/sam7200_0001")
    want = int(sys.argv[2]) if len(sys.argv) > 2 else 4
    data = path.read_bytes()
    u32 = lambda o: struct.unpack_from(">I", data, o)[0]
    f64 = lambda o: struct.unpack_from(">d", data, o)[0]

    page_size = u32(0x34) * 4
    if page_size not in (512, 2048, 4096):
        page_size = 2048
    n_pages = len(data) // page_size
    latest = u32(0x28)

    def po(pg):
        return pg * page_size

    def is_index(pg):
        return 0 < pg < n_pages and u32(po(pg) + 4) == INDEX_NOUN

    root = None
    pg = latest
    if 0 < pg < n_pages and u32(po(pg)) == 3:
        root = u32(po(pg) + 0x1C)

    welds = []

    def walk(pg, depth=0):
        if depth > 32 or not is_index(pg) or len(welds) >= want:
            return
        base = po(pg)
        w = 0x1C
        while w + 16 <= page_size and len(welds) < want:
            r0 = u32(base + w)
            r1 = u32(base + w + 4)
            cpg = u32(base + w + 8)
            v = u32(base + w + 12)
            w += 16
            if r0 == 0:
                break
            if r0 == 0x80000001 and r1 == 0x80000001:
                continue
            off = v >> 12
            if off == 0 and is_index(cpg):
                walk(cpg, depth + 1)
            elif off:
                bo = cpg * page_size + off * 2
                if bo + 8 <= len(data) and u32(bo + 0x0C) == 0x97247:  # WELD
                    welds.append(bo)

    walk(root)
    print(f"file={path} page_size={page_size} root={root} found {len(welds)} WELD(s)")

    records = []
    for bo in welds:
        cnt = u32(bo) & 0xFFFF
        words = [u32(bo + 4 * i) for i in range(min(cnt, 60))]
        records.append((bo, cnt, words))

    if not records:
        return

    width = min(r[1] for r in records)
    print("\nrefno / count:")
    for bo, cnt, words in records:
        print(f"  byte_off={bo} count={cnt} refno=({words[1]:#x},{words[2]:#x}) owner=({words[4]:#x},{words[5]:#x})")

    print("\naligned implicit words (C=const across all, V=varies):")
    print("  idx  " + "  ".join(f"inst{i}" .rjust(10) for i in range(len(records))) + "   tag")
    for i in range(width):
        col = [r[2][i] for r in records]
        const = all(c == col[0] for c in col)
        tag = "C" if const else "V"
        hexs = "  ".join(f"{c:#010x}" for c in col)
        ann = ""
        if 7 <= i <= width - 2 and not const:
            # try double interpretation at this word (pair i,i+1) for first instance
            try:
                d0 = struct.unpack(">d", struct.pack(">II", records[0][2][i], records[0][2][i + 1]))[0]
                if 1e-6 < abs(d0) < 1e7:
                    ann = f"  ~dbl[{i}:{i+2}]_inst0={d0:.3f}"
            except Exception:
                pass
        print(f"  [{i:2d}] {hexs}  {tag}{ann}")

    # decode POS (w13) and orientation triple for instance 0 per confirmed mechanism
    bo = records[0][0]
    print("\ninstance0 typed decode (offset = word index from record start):")
    pos = (f64(bo + 13 * 4), f64(bo + 15 * 4), f64(bo + 17 * 4))
    print(f"  POS @ off=13 (type8/size3): {tuple(round(x,3) for x in pos)}")
    tri = (f64(bo + 20 * 4), f64(bo + 22 * 4), f64(bo + 24 * 4))
    print(f"  ORI-ish @ off=20 (3 doubles): {tuple(round(x,3) for x in tri)}")


if __name__ == "__main__":
    main()
