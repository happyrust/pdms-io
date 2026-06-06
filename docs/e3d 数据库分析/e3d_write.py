# -*- coding: utf-8 -*-
r"""
e3d_write.py
============
Minimal, SAFE offline writer for E3D/PDMS element dbs: in-place edit of a
**fixed-size inline (implicit) attribute value** (real / int / reference).

Why this is safe (IDA-confirmed, core.dll 2.10):
  - DABACON pages carry NO checksum/CRC: db1_read_page (0x10630C20) is just FHDBRN
    (block read + big-endian<->host byte-swap, lock-retry); db1_write_page (0x10633FB0)
    writes the raw page buffer (no checksum computed). So overwriting a value's bytes,
    preserving big-endian + low-word-first(real) layout, yields a byte-valid page that
    reads back correctly (the same path PDMS uses).
  - We only ever change the value WORDS at the attribute's storage offset, never the
    record framing / count / length, so the layout is untouched (verified by byte-diff
    being confined to the value region).

LIMITATIONS (by design — these stay safe):
  - Fixed-size inline values only (type 2/6 real, 3/7 int scalar, 4/8/16 ref): the new
    value must have the SAME component count as stored (no record resize / re-layout).
  - Does NOT create a PDMS session/COW entry (it edits the latest data directly). The
    value reads back correctly; PDMS session history won't show it as a tracked change.
  - Text / variable-length / DA-explicit / UDA edits are NOT handled (would need
    re-layout) and are intentionally rejected.

ALWAYS operate on a COPY. Usage (demo, edits a copy then verifies + byte-diffs):
    python e3d_write.py <db_file> [--exe DIR]
"""
import os
import sys
import struct
import shutil

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from e3d_db_reader_v2 import E3DDb, looks_like_noun
from e3d_attr_decoder import SchemaSet, decode_full_element, _rw

REAL = {2, 6}
INT = {3, 7}
REF = {4, 8, 16}


def set_inline_value(buf: bytearray, record_off: int, desc: dict, sel: int, values) -> tuple:
    """Overwrite a fixed-size inline attribute value in-place. Returns (start,end) byte
    range changed. Raises ValueError if not a safe fixed-size inline edit."""
    t = desc['type']
    off = desc['alt'] if sel else desc['off']
    if off == 0:
        raise ValueError('attribute not stored inline (off==0)')
    if t == 5:
        raise ValueError('use set_inline_bool for booleans')
    prefixed = desc['size'] > 1
    # current stored component count
    if prefixed:
        cur_cnt = struct.unpack_from('>I', buf, record_off + 4 * off)[0]
        data_word = off + 1
    else:
        cur_cnt = desc['size']
        data_word = off
    vals = list(values)
    if len(vals) != cur_cnt:
        raise ValueError('component-count change not allowed (have %d, given %d)' % (cur_cnt, len(vals)))
    start = record_off + 4 * data_word
    if t in REAL:
        if sel == 0:
            raise ValueError('packed(sel=0) float write not supported by this safe writer')
        for j, v in enumerate(vals):                 # 2 words/comp, low-word-first
            b = struct.pack('>d', float(v))          # [hi32][lo32]
            struct.pack_into('>I', buf, start + 8 * j, struct.unpack('>I', b[4:8])[0])     # lo
            struct.pack_into('>I', buf, start + 8 * j + 4, struct.unpack('>I', b[0:4])[0])  # hi
        return (start, start + 8 * len(vals))
    if t in INT:
        for j, v in enumerate(vals):
            struct.pack_into('>I', buf, start + 4 * j, int(v) & 0xFFFFFFFF)
        return (start, start + 4 * len(vals))
    if t in REF:                                      # (dbno, refseq) per component
        for j, pair in enumerate(vals):
            dbno, seq = pair
            struct.pack_into('>I', buf, start + 8 * j, int(dbno) & 0xFFFFFFFF)
            struct.pack_into('>I', buf, start + 8 * j + 4, int(seq) & 0xFFFFFFFF)
        return (start, start + 8 * len(vals))
    raise ValueError('unsupported type %d for inline write' % t)


def find_element(db: 'E3DDb', ss: 'SchemaSet', name: str):
    """Return the byte offset of the named element record, or None."""
    root = db.session_chain()[0]['index_root_pgno']
    for r0, r1, pg, off in db.walk_index(root, max_entries=300000):
        if r1 == 0:
            continue
        bo = pg * db.page_size + off * 2
        if bo + 44 > len(db.blob):
            continue
        w0 = db.u32(bo)
        if (w0 >> 16) != 0 or not (8 <= (w0 & 0xFFFF) <= 512):
            continue
        if not looks_like_noun(db.u32(bo + 12)):
            continue
        r = decode_full_element(ss, db.blob, bo)
        if r.get('element_name') == name:
            return bo
    return None


def _pos_of(ss, buf, bo):
    r = decode_full_element(ss, buf, bo)
    return next((a['value'] for a in r['attrs'] if a['hash'] == 0x853B1), None)


if __name__ == '__main__':
    args = sys.argv[1:]
    exe = r'D:\AVEVA\Everything3D2.10'
    if '--exe' in args:
        i = args.index('--exe'); exe = args[i + 1]; del args[i:i + 2]
    src = args[0] if args else r'pdms-test-data\sam7200_0001'
    dst = '_e3d_write_demo.bin'
    shutil.copyfile(src, dst)                          # NEVER edit the original
    ss = SchemaSet(exe)
    db = E3DDb(dst)
    bo = find_element(db, ss, '/WB1')
    if bo is None:
        print('no /WB1 element; pass a db that has one'); sys.exit(0)
    _, td = ss.typedef(0x97247)
    W = _rw(db.blob, bo, 46)
    sel = (W[10] >> 29) & 1
    print('WB1 @%d sel=%d  POS before = %s' % (bo, sel, _pos_of(ss, db.blob, bo)))

    buf = bytearray(open(dst, 'rb').read())
    rng = set_inline_value(buf, bo, td[0x853B1], sel, (1000.25, -2000.5, 3000.75))
    open(dst, 'wb').write(buf)

    db2 = E3DDb(dst)
    after = _pos_of(ss, db2.blob, bo)
    name_after = decode_full_element(ss, db2.blob, bo)['element_name']
    orig = open(src, 'rb').read(); edit = open(dst, 'rb').read()
    diff = [i for i in range(min(len(orig), len(edit))) if orig[i] != edit[i]]
    confined = all(rng[0] <= b < rng[1] for b in diff)
    print('POS after  = %s   name=%s' % (after, name_after))
    print('changed bytes=%d confined-to-value=%s (region=%s)' % (len(diff), confined, rng))
    ok = after == [1000.25, -2000.5, 3000.75] and name_after == '/WB1' and confined
    print('PASS' if ok else 'FAIL')
    os.remove(dst)
    sys.exit(0 if ok else 1)
