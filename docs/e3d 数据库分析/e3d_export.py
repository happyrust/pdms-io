# -*- coding: utf-8 -*-
r"""
e3d_export.py - Full offline export of an E3D/PDMS design db to JSON,
with optional cross-db reference resolution.

Walks the latest session's B-tree index; for every valid primary element decodes
(offline, via schema *vir.dat typedefs + record framing): noun, NAME, refno, owner,
all implicit attributes, all DA/explicit attributes. Reference attributes (type
4/8/16) carry (dbno, refseq); with --cat <catalogue.db> their targets are resolved
to names (design<->catalogue links: SPRE->SPCO, PSPE->SPEC, CREF/HREF/TREF->elements).

Usage:
  python e3d_export.py <db_file> [out.json] [--exe <dir>] [--cat <db>]... [--max N]
"""
import sys, os, json
from collections import Counter

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from e3d_db_reader_v2 import E3DDb, looks_like_noun       # noqa: E402
from e3d_attr_decoder import SchemaSet, decode_full_element  # noqa: E402


def _attrs_to_dict(lst):
    out = {}
    for a in lst:
        if a['value'] is None:
            continue
        out[a['name'] or ('0x%X' % a['hash'])] = a['value']
    return out


def index_db(db, ss, collect=True, max_el=10 ** 9):
    """Walk a db: return (elements, refmap{(dbno,seq):(noun,name)})."""
    root = db.session_chain()[0]['index_root_pgno']
    leaf = (e for e in db.walk_index(root, max_entries=4_000_000) if e[3] != 0)  # generous: large dbs (ams1112) exceed 300k leaves
    elems, refmap, seen = [], {}, set()
    for r0, r1, pg, off in leaf:
        if len(elems) >= max_el and collect:
            break
        bo = pg * db.page_size + off * 2
        if bo + 44 > len(db.blob):
            continue
        w0 = db.u32(bo)
        if (w0 >> 16) != 0 or not (8 <= (w0 & 0xFFFF) <= 512):
            continue
        if not looks_like_noun(db.u32(bo + 12)) or (r0, r1) in seen:
            continue
        seen.add((r0, r1))
        r = decode_full_element(ss, db.blob, bo)
        if not r.get('noun'):
            continue
        if r.get('element_name'):
            refmap[(r0, r1)] = (r['noun_name'], r['element_name'])
        if collect:
            r['_refno'] = (r0, r1)
            elems.append(r)
    return elems, refmap


def resolve_refs(attrs, refmap):
    """For ref-type attrs (4/8/16), map (dbno,refseq) pairs to target names."""
    refs = {}
    for a in attrs:
        if a['type'] not in (4, 8, 16) or not a['value']:
            continue
        tgts = []
        for pair in a['value']:
            if isinstance(pair, (list, tuple)) and len(pair) == 2 and tuple(pair) != (0, 0):
                t = refmap.get(tuple(pair))
                tgts.append(t[1] if t else '=%d/%d' % tuple(pair))
        if tgts:
            refs[a['name'] or ('0x%X' % a['hash'])] = tgts
    return refs


def main():
    args = sys.argv[1:]
    cats, exe, max_el = [], r'D:\AVEVA\Everything3D2.10', 10 ** 9
    pos = []
    i = 0
    while i < len(args):
        if args[i] == '--cat':
            cats.append(args[i + 1]); i += 2
        elif args[i] == '--exe':
            exe = args[i + 1]; i += 2
        elif args[i] == '--max':
            max_el = int(args[i + 1]); i += 2
        else:
            pos.append(args[i]); i += 1
    if not pos:
        print(__doc__); return
    db_path = pos[0]
    out = pos[1] if len(pos) > 1 else 'e3d_full_export.json'

    ss = SchemaSet(exe)
    db = E3DDb(db_path)
    elems, refmap = index_db(db, ss, collect=True, max_el=max_el)
    for cat in cats:                       # merge catalogue refmaps for cross-db resolution
        _, cmap = index_db(E3DDb(cat), ss, collect=False)
        refmap.update(cmap)

    out_elems, nh, named = [], Counter(), 0
    for r in elems:
        rec = {'refno': '%d/%d' % r['_refno'], 'noun': r['noun_name'], 'name': r.get('element_name'),
               'owner': '%d/%d' % r['owner'], 'implicit': _attrs_to_dict(r['attrs']),
               'explicit': _attrs_to_dict(r['da'])}
        refs = resolve_refs(r['attrs'], refmap)
        if refs:
            rec['refs'] = refs
        named += bool(rec['name'])
        nh[r['noun_name']] += 1
        out_elems.append(rec)

    doc = {'file': db_path, 'element_count': len(out_elems), 'named': named,
           'noun_types': len(nh), 'refmap_size': len(refmap),
           'noun_histogram': dict(nh.most_common()), 'elements': out_elems}
    with open(out, 'w', encoding='utf-8') as f:
        json.dump(doc, f, ensure_ascii=False, indent=1)
    print('exported %d elements (%d named, %d noun types, refmap=%d) -> %s'
          % (len(out_elems), named, len(nh), len(refmap), out))
    resolved_total = sum(len(e.get('refs', {})) for e in out_elems)
    print('elements with resolved refs: %d' % sum(1 for e in out_elems if e.get('refs')))
    sample = next((e for e in out_elems if e.get('refs') and e['name']), None)
    if sample:
        print('\nsample element with resolved refs:')
        print(json.dumps({k: sample[k] for k in ('refno', 'noun', 'name', 'refs')}, ensure_ascii=False, indent=1)[:700])


if __name__ == '__main__':
    main()
