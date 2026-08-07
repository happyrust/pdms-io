"""Verify db3 index-page header semantics against a real PDMS database.

Checks three claims taken from core.dll decompilation:
  1. index pages carry page_type == 5 (sub_5B014E0 rejects anything else with code 659)
  2. dword[6] is a trailing free-word count, not a page pointer
     entry_count == (page_words - free - 7) / (key_words + value_words)
  3. the low bit of the packed word is a live/released flag, so the
     "stop at the first all-zero slot" reader over-counts entries
"""

import struct
import sys
from collections import Counter

INDEX_NOUN = 0x00CC47DF
HEADER_WORDS = 7


def main(path: str) -> int:
    with open(path, "rb") as fh:
        blob = fh.read()

    header_page_size_field = struct.unpack_from(">I", blob, 0x34)[0]
    page_bytes = header_page_size_field * 4
    total_pages = len(blob) // page_bytes
    latest_ses_pgno = struct.unpack_from(">I", blob, 0x38)[0]

    print(f"file                = {path}")
    print(f"file size           = {len(blob)}")
    print(f"header[0x34]        = {header_page_size_field}  (words)")
    print(f"page size           = {page_bytes} bytes")
    print(f"page count          = {total_pages}")
    print(f"header[0x38]        = {latest_ses_pgno}")
    print()

    page_types = Counter()
    key_widths = Counter()
    value_widths = Counter()
    formula_ok = 0
    formula_bad = []
    total_valid = 0
    total_zero_scan = 0
    pages_with_ghosts = 0
    flag_mismatch = []

    for pgno in range(total_pages):
        base = pgno * page_bytes
        noun = struct.unpack_from(">I", blob, base + 4)[0]
        if noun != INDEX_NOUN:
            continue

        words = struct.unpack_from(f">{header_page_size_field}I", blob, base)
        page_type, _, level, key_w, val_w, _unk, free = words[:7]
        page_types[page_type] += 1
        key_widths[key_w] += 1
        value_widths[val_w] += 1

        entry_words = key_w + (2 if level else (val_w if val_w >= 0 else 0))
        if entry_words <= 0:
            continue

        usable = header_page_size_field - free - HEADER_WORDS
        by_formula = usable // entry_words
        total_valid += max(by_formula, 0)

        # what IndexPageView::from_page currently does: 16-byte slots, stop at
        # the first all-zero slot
        slot_bytes = 16
        by_zero_scan = 0
        live_by_flag = 0
        off = base + HEADER_WORDS * 4
        while off + slot_bytes <= base + page_bytes:
            slot = blob[off:off + slot_bytes]
            if slot == b"\x00" * slot_bytes:
                break
            by_zero_scan += 1
            packed = struct.unpack_from(">I", slot, 12)[0]
            if packed & 1:
                live_by_flag += 1
            off += slot_bytes
        total_zero_scan += by_zero_scan
        if by_zero_scan > by_formula:
            pages_with_ghosts += 1

        if usable % entry_words == 0 and by_formula >= 0:
            formula_ok += 1
        else:
            formula_bad.append((pgno, free, entry_words, usable))

        if level == 0 and live_by_flag != by_formula:
            flag_mismatch.append((pgno, live_by_flag, by_formula, by_zero_scan, free))

    print("--- claim 1: page_type of index pages ---")
    print(f"page_type histogram = {dict(page_types)}")
    print()
    print("--- header field widths ---")
    print(f"dword[3] key words   = {dict(key_widths)}")
    print(f"dword[4] value words = {dict(value_widths)}")
    print()
    print("--- claim 2: dword[6] as trailing free-word count ---")
    print(f"pages where (page_words - free - 7) is a whole multiple of entry width: "
          f"{formula_ok}/{sum(page_types.values())}")
    for row in formula_bad[:10]:
        print(f"  mismatch pgno={row[0]} free={row[1]} entry_words={row[2]} usable={row[3]}")
    print()
    print("--- claim 3: zero-scan reader over-counts ---")
    print(f"entries by free-count formula = {total_valid}")
    print(f"entries by zero-scan reader   = {total_zero_scan}")
    print(f"ghost entries                 = {total_zero_scan - total_valid}")
    print(f"pages affected                = {pages_with_ghosts}/{sum(page_types.values())}")
    print()
    print("--- leaf pages where popcount(flag bit 0) != formula count ---")
    print(f"count = {len(flag_mismatch)}")
    for row in flag_mismatch[:10]:
        print(f"  pgno={row[0]} live_by_flag={row[1]} by_formula={row[2]} "
              f"zero_scan={row[3]} free={row[4]}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else
                  r"D:\work\plant-code\pdms-io\pdms-test-data\sam7200_0001"))
