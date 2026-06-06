#!/usr/bin/env python3
"""Enumerate ALL DB_Noun ATGTDF fields and dump WELD's vector-valued fields.

Goal: find the noun field that lists attributes in *storage* order (the DAB
list), which is what we need to reconstruct per-noun attribute offsets offline.
Reuses the validated logic from attlib_atnain_probe.py.
"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from attlib_atnain_probe import AttlibProbe, db1_hash, db1_dehash, find_entry  # noqa: E402


def main() -> None:
    path = Path(sys.argv[1] if len(sys.argv) > 1 else "test-file/attlib.dat")
    noun_name = sys.argv[2] if len(sys.argv) > 2 else "WELD"

    probe = AttlibProbe(path)
    directory = probe.directory()
    attr_index = probe.parse_atgtix(directory[2])
    noun_fields, _ext = probe.parse_atgtdf(directory[4])
    noun_index = probe.parse_atgtix(directory[6])
    attr_hashes = {e.hash_value for e in attr_index}

    print(f"file={path} noun={noun_name}")
    print(f"noun_fields={len(noun_fields)} attr_index={len(attr_index)}")

    print("\nALL noun ATGTDF fields:")
    for i, e in enumerate(noun_fields, start=1):
        print(f"  {i:3d} 0x{e.hash_value:08X} {db1_dehash(e.hash_value).strip():8s} value={e.value} kind={e.kind}")

    noun = find_entry(noun_index, noun_name)
    if noun is None:
        print(f"{noun_name} not found in noun index")
        return
    _idx, noun_entry = noun

    print(f"\n{noun_name} vector-valued fields (lists of attribute hashes):")
    for i, e in enumerate(noun_fields, start=1):
        values = probe.read_vector_field(noun_entry, i)
        if not values:
            continue
        # Heuristic: a list is "attribute-like" if most entries decode to attr hashes
        attrish = sum(1 for v in values if v in attr_hashes)
        if len(values) >= 2 and attrish >= max(2, len(values) // 2):
            decoded = ", ".join(db1_dehash(v).strip() for v in values)
            print(f"  field#{i} {db1_dehash(e.hash_value).strip():8s} n={len(values)} attrish={attrish}")
            print(f"      {decoded}")


if __name__ == "__main__":
    main()
