# -*- coding: utf-8 -*-
r"""
type_enum_probe.py
==================
Catalog the type-def descriptor `type` enumeration (desc[2]) empirically across
ALL E3D schema/template libraries (%AVEVA_DESIGN_EXE%\*vir.dat).

Backs up the authoritative db4_get_ce_att (0x10612A50) switch analysis:
for every distinct `type` code it reports how many attributes use it, the
distribution of `size`, how many are inline (main/alt offset != 0) vs off==0
(pseudo/explicit/DA), and a few example NOUN.ATTR names (db1_dehash).

Pure offline; read-only. Usage:
    python type_enum_probe.py [EXE_DIR]   (default D:\AVEVA\Everything3D2.10)
"""
import os, sys, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from e3d_attr_decoder import SchemaSet, db1_dehash

EXE = sys.argv[1] if len(sys.argv) > 1 else r'D:\AVEVA\Everything3D2.10'

ss = SchemaSet(EXE)
print('schemas=%d  noun_types=%d' % (len(ss.schemas), len(ss.noun2schema)))

# tally per type: count, sizes, inline(main!=0), inline(alt!=0), off==0, examples
stat = collections.defaultdict(lambda: dict(n=0, sizes=collections.Counter(),
                                            main=0, alt=0, off0=0, ex=[]))
seen_noun = set()
for noun, s in ss.noun2schema.items():
    if noun in seen_noun:
        continue
    seen_noun.add(noun)
    try:
        td = s.typedef(noun)
    except Exception:
        continue
    if not td:
        continue
    nn = db1_dehash(noun)
    for h, d in td.items():
        t = d['type']
        st = stat[t]
        st['n'] += 1
        st['sizes'][d['size']] += 1
        if d['off']:
            st['main'] += 1
        if d['alt']:
            st['alt'] += 1
        if d['off'] == 0 and d['alt'] == 0:
            st['off0'] += 1
        if len(st['ex']) < 8:
            st['ex'].append('%s.%s(sz=%d,off=%d,alt=%d)' %
                            (nn, db1_dehash(h) or ('0x%X' % h),
                             d['size'], d['off'], d['alt']))

print('\n%-5s %-8s %-22s %-7s %-7s %-7s  examples' %
      ('type', 'count', 'top sizes', 'main!=0', 'alt!=0', 'off==0'))
for t in sorted(stat):
    st = stat[t]
    tops = ','.join('%d:%d' % (sz, c) for sz, c in st['sizes'].most_common(4))
    print('%-5d %-8d %-22s %-7d %-7d %-7d' %
          (t, st['n'], tops, st['main'], st['alt'], st['off0']))

# detailed examples for the text / special / array types
for t in (9, 10, 11, 12, 14, 15, 16, 17, 18, 19):
    if t in stat:
        print('\n--- type %d examples ---' % t)
        for e in stat[t]['ex']:
            print('   ', e)
