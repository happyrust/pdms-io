#!/usr/bin/env python3
"""
Probe E3D 2.10 attlib.dat tables used by ATTOPE / ATNAIN.

This is intentionally a small, read-only reverse-engineering probe. It follows
the static core.dll wiring:

  directory record 2 word[2] -> ATGTIX for attribute hashes
  directory record 2 word[4] -> ATGTDF for DB_Noun field definitions
  directory record 2 word[6] -> ATGTIX for noun hashes

FHDBRN record numbers are 1-based, so record N is physical file page N-1.
"""
from __future__ import annotations

import argparse
import struct
from dataclasses import dataclass
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


@dataclass(frozen=True)
class AtgtixEntry:
    hash_value: int
    record: int
    displacement: int
    combined: int


@dataclass(frozen=True)
class AtgtdfEntry:
    hash_value: int
    value: int
    kind: int
    ext_index: int


class AttlibProbe:
    def __init__(self, path: Path):
        self.path = path
        self.data = path.read_bytes()
        self.records = len(self.data) // PAGE_SIZE

    def record_words(self, record_1based: int) -> tuple[int, ...]:
        """FHDBRN records are 1-based; the file pages are zero-based."""
        physical_page = record_1based - 1
        if physical_page < 0 or physical_page >= self.records:
            raise IndexError(f"record {record_1based} outside file")
        off = physical_page * PAGE_SIZE
        return struct.unpack_from(">512I", self.data, off)

    def directory(self) -> tuple[int, ...]:
        return self.record_words(2)[:8]

    def parse_atgtix(self, start_record: int, max_entries: int = 10_000) -> list[AtgtixEntry]:
        out: list[AtgtixEntry] = []
        record = start_record

        while len(out) < max_entries:
            words = self.record_words(record)
            i = 0
            while i < PAGE_WORDS:
                word = words[i]
                if HASH_LOW <= word <= HASH_HIGH:
                    combined = words[i + 1]
                    out.append(
                        AtgtixEntry(
                            hash_value=word,
                            record=combined // PAGE_WORDS,
                            displacement=combined % PAGE_WORDS,
                            combined=combined,
                        )
                    )
                    i += 2
                    continue
                if word == 0:
                    record += 1
                    break
                if word == 0xFFFFFFFF:
                    return out
                return out
            else:
                return out

        return out

    def parse_atgtdf(
        self,
        start_record: int,
        max_entries: int = 10_000,
    ) -> tuple[list[AtgtdfEntry], list[int]]:
        entries: list[AtgtdfEntry] = []
        ext: list[int] = []
        record = start_record

        while len(entries) < max_entries:
            words = self.record_words(record)
            i = 0
            while i < PAGE_WORDS:
                word = words[i]
                if HASH_LOW <= word <= HASH_HIGH:
                    value = words[i + 1]
                    kind = words[i + 2]
                    i += 3
                    ext_index = 0

                    if kind == 2:
                        ext_index = len(ext) + 1
                        if value == 4:
                            count = words[i]
                            i += 1
                            ext.append(count)
                            for _ in range(count):
                                ext.append(words[i])
                                i += 1
                        else:
                            ext.append(words[i])
                            i += 1
                    elif kind != 1:
                        return entries, ext

                    entries.append(AtgtdfEntry(word, value, kind, ext_index))
                    continue
                if word == 0:
                    record += 1
                    break
                if word == 0xFFFFFFFF:
                    return entries, ext
                return entries, ext
            else:
                return entries, ext

        return entries, ext

    @staticmethod
    def find_index(entries: list[AtgtixEntry] | list[AtgtdfEntry], name: str) -> tuple[int, object] | None:
        target = db1_hash(name)
        for index, entry in enumerate(entries, start=1):
            if entry.hash_value == target:
                return index, entry
        return None

    def matrix_pointer(
        self,
        index_entry: AtgtixEntry,
        field_index: int,
    ) -> tuple[tuple[int, ...], int, int] | None:
        words = self.record_words(index_entry.record)
        word_index = index_entry.displacement + field_index - 2
        if word_index < 0 or word_index >= PAGE_WORDS:
            return None
        pointer = words[word_index]
        if pointer == 0 or pointer == 0xFFFFFFFF:
            return None
        value_index = index_entry.displacement + pointer - 2
        if value_index < 0 or value_index >= PAGE_WORDS:
            return None
        return words, value_index, pointer

    def read_int_field(
        self,
        index_entry: AtgtixEntry,
        field_index: int,
    ) -> int | None:
        target = self.matrix_pointer(index_entry, field_index)
        if target is None:
            return None
        words, value_index, _pointer = target
        return words[value_index]

    def read_string_field(
        self,
        index_entry: AtgtixEntry,
        field_index: int,
    ) -> str | None:
        target = self.matrix_pointer(index_entry, field_index)
        if target is None:
            return None
        words, value_index, _pointer = target
        length = words[value_index]
        chars = []
        for i in range(length):
            word_index = value_index + 1 + i
            if word_index >= PAGE_WORDS:
                return None
            chars.append(chr(words[word_index] & 0xFF))
        return "".join(chars)

    def read_vector_field(
        self,
        index_entry: AtgtixEntry,
        field_index: int,
        max_values: int = 200,
    ) -> list[int] | None:
        target = self.matrix_pointer(index_entry, field_index)
        if target is None:
            return None
        words, value_index, _pointer = target
        length = words[value_index]
        values = []
        for i in range(min(length, max_values)):
            word_index = value_index + 1 + i
            if word_index >= PAGE_WORDS:
                return values
            values.append(words[word_index])
        return values


def find_entry(entries: list[AtgtixEntry] | list[AtgtdfEntry], name: str):
    target = db1_hash(name)
    for index, entry in enumerate(entries, start=1):
        if entry.hash_value == target:
            return index, entry
    return None


def attribute_summary(
    probe: AttlibProbe,
    attr_entry: AtgtixEntry,
    attr_fields: list[AtgtdfEntry],
) -> dict[str, object]:
    out: dict[str, object] = {}
    for field in ("SIZE", "TYPE", "DEFI", "DTYP", "UNIT"):
        field_hit = probe.find_index(attr_fields, field)
        if field_hit is None:
            continue
        field_idx, _field_entry = field_hit
        out[field] = probe.read_int_field(attr_entry, field_idx)
    for field in ("NAME", "CATEG"):
        field_hit = probe.find_index(attr_fields, field)
        if field_hit is None:
            continue
        field_idx, _field_entry = field_hit
        out[field] = probe.read_string_field(attr_entry, field_idx)
    return out


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "attlib",
        nargs="?",
        default="test-file/attlib.dat",
        help="Path to attlib.dat",
    )
    parser.add_argument("--noun", default="WELD")
    parser.add_argument("--attr", default="POS")
    args = parser.parse_args()

    probe = AttlibProbe(Path(args.attlib))
    directory = probe.directory()

    attr_index = probe.parse_atgtix(directory[2])
    attr_fields, _attr_field_ext = probe.parse_atgtdf(directory[0])
    noun_fields, noun_field_ext = probe.parse_atgtdf(directory[4])
    noun_index = probe.parse_atgtix(directory[6])

    print(f"file={probe.path} bytes={len(probe.data)} records={probe.records}")
    print("directory=" + " ".join(f"0x{x:X}" for x in directory))
    print(
        "counts "
        f"attr_index={len(attr_index)} "
        f"attr_fields={len(attr_fields)} "
        f"noun_fields={len(noun_fields)} "
        f"noun_field_ext={len(noun_field_ext)} "
        f"noun_index={len(noun_index)}"
    )

    noun = find_entry(noun_index, args.noun)
    attr_as_index = find_entry(attr_index, args.attr)
    attr_as_noun_field = find_entry(noun_fields, args.attr)

    print(f"noun {args.noun} hash=0x{db1_hash(args.noun):X} -> {noun}")
    print(f"attr_index {args.attr} hash=0x{db1_hash(args.attr):X} -> {attr_as_index}")
    print(f"noun_field {args.attr} hash=0x{db1_hash(args.attr):X} -> {attr_as_noun_field}")

    if attr_as_index is not None:
        _attr_idx, attr_entry = attr_as_index
        print(f"\nattribute metadata for {args.attr}:")
        summary = attribute_summary(probe, attr_entry, attr_fields)
        for field in ("SIZE", "TYPE", "DEFI", "DTYP", "UNIT"):
            value = summary.get(field)
            decoded = db1_dehash(value).strip() if value and value > BASE27_OFFSET else ""
            suffix = f" ({decoded})" if decoded else ""
            print(f"  {field:<5} = {value}{suffix}")
        for field in ("NAME", "CATEG"):
            value = summary.get(field)
            print(f"  {field:<5} = {value!r}")

    print("\nnoun fields with value/type 8:")
    for index, entry in enumerate(noun_fields, start=1):
        if entry.value == 8:
            print(
                f"  {index:03d} "
                f"0x{entry.hash_value:X} {db1_dehash(entry.hash_value).strip():<8} "
                f"kind={entry.kind}"
            )

    if noun is not None:
        _noun_idx, noun_entry = noun
        attr_hashes = {entry.hash_value for entry in attr_index}
        for field in ("DISPLY", "PRDISP"):
            field_hit = probe.find_index(noun_fields, field)
            if field_hit is None:
                continue
            field_idx, _field_entry = field_hit
            values = probe.read_vector_field(noun_entry, field_idx)
            if values is None:
                continue
            print(f"\n{args.noun}.{field} attributes ({len(values)}):")
            for value in values:
                marker = "*" if value in attr_hashes else " "
                print(f"  {marker} 0x{value:X} {db1_dehash(value).strip()}")

        print(f"\n{args.noun} resolvable attribute schema:")
        seen = set()
        for field in ("DISPLY", "PRDISP"):
            field_hit = probe.find_index(noun_fields, field)
            if field_hit is None:
                continue
            field_idx, _field_entry = field_hit
            values = probe.read_vector_field(noun_entry, field_idx) or []
            for value in values:
                if value in seen:
                    continue
                seen.add(value)
                attr_hit = next(
                    ((index, entry) for index, entry in enumerate(attr_index, start=1) if entry.hash_value == value),
                    None,
                )
                if attr_hit is None:
                    continue
                _index, entry = attr_hit
                meta = attribute_summary(probe, entry, attr_fields)
                unit = meta.get("UNIT")
                unit_name = db1_dehash(unit).strip() if isinstance(unit, int) and unit > BASE27_OFFSET else ""
                print(
                    "  "
                    f"{db1_dehash(value).strip():<8} "
                    f"hash=0x{value:X} "
                    f"type={meta.get('TYPE')} "
                    f"size={meta.get('SIZE')} "
                    f"defi={meta.get('DEFI')} "
                    f"unit={unit_name or unit} "
                    f"category={meta.get('CATEG')!r}"
                )

    if attr_as_noun_field is None:
        print(
            "\nNOTE: the requested attribute is not a DB_Noun ATGTDF field. "
            "For physical attributes such as POS, continue via DB_Attribute / "
            "attribute metadata rather than DB_Noun::internalGetField(POS)."
        )


if __name__ == "__main__":
    main()
