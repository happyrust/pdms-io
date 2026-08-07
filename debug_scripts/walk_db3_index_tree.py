"""Walk the db3 RefNo index tree from the latest session root.

Counts reachable leaf entries under two competing readers so the effect of the
db3 fix can be separated from pre-existing traversal bugs:

  free-count : entry count comes from the page header's trailing free-word
               field, the way core.dll sub_5B010F0 derives it
  zero-scan  : walk 16-byte slots and stop at the first all-zero slot, the way
               IndexPageView::from_page used to

It also reports what the crate's IndexTableIterator would yield, which only
follows the leftmost leaf and one level of siblings.
"""

import struct
import sys

INDEX_NOUN = 0x00CC47DF
HEADER_DWORDS = 7
HEADER_BYTES = HEADER_DWORDS * 4
START_MARKER = (0x80000001, 0x80000001)


def u32(blob, off):
    return struct.unpack_from(">I", blob, off)[0]


class Db:
    def __init__(self, path):
        with open(path, "rb") as fh:
            self.blob = fh.read()
        self.page_dwords = u32(self.blob, 0x34)
        self.page_bytes = self.page_dwords * 4
        self.latest_ses_pgno = u32(self.blob, 0x28)

    def page(self, pgno):
        base = pgno * self.page_bytes
        return self.blob[base:base + self.page_bytes]

    def index_root(self):
        ses = self.page(self.latest_ses_pgno)
        assert u32(ses, 0) == 3, f"page {self.latest_ses_pgno} is not a session page"
        return u32(ses, 0x1C), u32(ses, 0x0C)

    def parse(self, pgno, mode):
        page = self.page(pgno)
        if u32(page, 4) != INDEX_NOUN:
            return None
        level = u32(page, 8)
        free = u32(page, 0x18)
        stride = 16

        if mode == "free-count":
            count = max(self.page_dwords - HEADER_DWORDS - free, 0) // 4
        else:
            count = 0
            off = HEADER_BYTES
            while off + stride <= self.page_bytes:
                if page[off:off + stride] == b"\x00" * stride:
                    break
                count += 1
                off += stride

        entries = []
        for i in range(count):
            off = HEADER_BYTES + i * stride
            if off + stride > self.page_bytes:
                break
            entries.append((u32(page, off), u32(page, off + 4), u32(page, off + 8)))
        return level, entries


def walk_full(db, root, mode):
    """Full recursive descent: every child of every internal page."""
    seen_pages = set()
    leaf_entries = 0
    stack = [root]
    while stack:
        pgno = stack.pop()
        if pgno in seen_pages:
            continue
        seen_pages.add(pgno)
        parsed = db.parse(pgno, mode)
        if parsed is None:
            continue
        level, entries = parsed
        if level == 0:
            leaf_entries += sum(1 for e in entries if (e[0], e[1]) != START_MARKER)
        else:
            for e in entries:
                stack.append(e[2])
    return leaf_entries, len(seen_pages)


def walk_like_rust_iterator(db, root, mode):
    """Mimic IndexTableIterator: leftmost descent, then one level of siblings."""
    yielded = 0
    stack = []
    pgno = root
    while True:
        parsed = db.parse(pgno, mode)
        if parsed is None:
            return yielded
        level, entries = parsed
        if level == 0:
            yielded += sum(1 for e in entries if (e[0], e[1]) != START_MARKER)
            break
        if not entries:
            return yielded
        stack.append((pgno, 0))
        pgno = entries[0][2]

    while stack:
        parent, idx = stack.pop()
        parsed = db.parse(parent, mode)
        if parsed is None:
            break
        _, entries = parsed
        if idx + 1 >= len(entries):
            break
        stack.append((parent, idx + 1))
        pgno = entries[idx + 1][2]
        while True:
            parsed = db.parse(pgno, mode)
            if parsed is None:
                return yielded
            level, entries = parsed
            if level == 0:
                yielded += sum(1 for e in entries if (e[0], e[1]) != START_MARKER)
                break
            if not entries:
                return yielded
            stack.append((pgno, 0))
            pgno = entries[0][2]
    return yielded


def main(path):
    db = Db(path)
    root, sesno = db.index_root()
    print(f"file             = {path}")
    print(f"page size        = {db.page_bytes} bytes ({db.page_dwords} dwords)")
    print(f"latest_ses_pgno  = {db.latest_ses_pgno}  (sesno {sesno})")
    print(f"index_root       = page {root}")
    root_level, root_entries = db.parse(root, "free-count")
    print(f"root level       = {root_level}, root entries = {len(root_entries)}")
    print()
    for mode in ("free-count", "zero-scan"):
        total, pages = walk_full(db, root, mode)
        partial = walk_like_rust_iterator(db, root, mode)
        print(f"[{mode}]")
        print(f"  full recursive descent : {total} entries over {pages} pages")
        print(f"  IndexTableIterator     : {partial} entries")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else
                  r"D:\work\plant-code\pdms-io\pdms-test-data\sam7200_0001"))
