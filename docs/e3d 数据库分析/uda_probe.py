# -*- coding: utf-8 -*-
r"""
uda_probe.py
============
Offline survey of USER-DEFINED ATTRIBUTE (UDA) storage in an E3D/PDMS element db.

UDA on-disk facts (core.dll 2.10, IDA-confirmed):
  - PDMS_Hash::IsUDA (0x10001bc0):  hash > 0x171FAD39  =>  UDA.
  - db4_get_ce_att (0x10612A50):    a UDA value is NOT in the implicit area; it is a
    DA/explicit-region entry keyed by the UDA hash (off==0 path -> DA-list scan),
    framed like any DA entry: [hash][ctrl: type<<26 | wordcount][value words...].
    UDA entries observed with ctrl type 7 (int-array framing).
  - The value words are a nested UDA value-blob: [len][0][...field/value tokens...]
    often ending with the 1601/1701 (OF/WRT) qualifier markers (exppdms/EXRTPD grammar).
  - UDA *name/type/unit* come from the UDA DICTIONARY library (udalib): LXANAM (name),
    LXALEN (length), LXUNIT (unit) -> a separate dictionary db (like catalogue refs),
    so offline we recover the UDA hash + raw value blob, but not the UDA name without
    the dictionary file.

This probe walks the latest session B-tree and reports UDA usage. Read-only.

Usage:  python uda_probe.py <db_file> [--exe <dir>] [--max N] [--show K]
"""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from e3d_db_reader_v2 import E3DDb, looks_like_noun
from e3d_attr_decoder import SchemaSet, decode_da_list, db1_dehash, NAME_HASH, UDA_THRESHOLD
from collections import Counter


def survey(db_path, exe, max_el=10**9, show=6):
    ss = SchemaSet(exe)
    db = E3DDb(db_path)
    root = db.session_chain()[0]['index_root_pgno']
    n_el = n_uda_el = n_uda = 0
    by_hash = Counter()      # uda hash -> element count
    type_hist = Counter()    # ctrl type of uda entries
    samples = []
    seen = set()
    for r0, r1, pg, off in db.walk_index(root, max_entries=300000):
        if r1 == 0:
            continue
        bo = pg * db.page_size + off * 2
        if bo + 44 > len(db.blob):
            continue
        w0 = db.u32(bo)
        if (w0 >> 16) != 0 or not (8 <= (w0 & 0xFFFF) <= 512):
            continue
        if not looks_like_noun(db.u32(bo + 12)) or (r0, r1) in seen:
            continue
        seen.add((r0, r1))
        n_el += 1
        if n_el > max_el:
            break
        try:
            da = decode_da_list(db.blob, bo, 1)
        except Exception:
            continue
        udas = [a for a in da if a['hash'] > UDA_THRESHOLD]
        if not udas:
            continue
        n_uda_el += 1
        n_uda += len(udas)
        for a in udas:
            by_hash['0x%X' % a['hash']] += 1
            type_hist[a['type']] += 1
        if len(samples) < show:
            nm = next((a['value'] for a in da if a['hash'] == NAME_HASH), None)
            samples.append((db1_dehash(db.u32(bo + 12)), nm, udas))
    return dict(db=os.path.basename(db_path), elements=n_el, uda_elements=n_uda_el,
                uda_entries=n_uda, distinct=by_hash, type_hist=type_hist, samples=samples)


def _blob_struct(words):
    """Best-effort top-level parse of a UDA value-blob: [len][flag][...][trailer]."""
    if not words:
        return {}
    trailer = [w for w in words[-2:] if w in (1601, 1602, 1701, 1702)]
    return dict(declared_len=words[0], flag=words[1] if len(words) > 1 else None,
                trailer=trailer, n=len(words))


if __name__ == '__main__':
    args = sys.argv[1:]
    exe, max_el, show = r'D:\AVEVA\Everything3D2.10', 10**9, 6
    pos = []
    i = 0
    while i < len(args):
        if args[i] == '--exe':
            exe = args[i + 1]; i += 2
        elif args[i] == '--max':
            max_el = int(args[i + 1]); i += 2
        elif args[i] == '--show':
            show = int(args[i + 1]); i += 2
        else:
            pos.append(args[i]); i += 1
    if not pos:
        print(__doc__); sys.exit(0)
    res = survey(pos[0], exe, max_el, show)
    print('db=%(db)s  elements=%(elements)d  uda_elements=%(uda_elements)d  uda_entries=%(uda_entries)d'
          % res)
    print('uda entry ctrl-type histogram:', dict(res['type_hist']))
    print('distinct UDA hashes (top 15):')
    for h, c in res['distinct'].most_common(15):
        print('   %-12s x%d' % (h, c))
    print('\nsample UDA-bearing elements:')
    for noun, name, udas in res['samples']:
        print('  %s %s' % (noun, name or ''))
        for a in udas:
            st = _blob_struct(a['value'] if isinstance(a['value'], list) else [])
            print('     %-14s type=%d wc=%d  struct=%s' %
                  (db1_dehash(a['hash']), a['type'],
                   len(a['value']) if isinstance(a['value'], list) else 0, st))
