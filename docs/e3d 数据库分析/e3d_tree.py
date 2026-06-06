# -*- coding: utf-8 -*-
r"""
e3d_tree.py - Reconstruct the PDMS element hierarchy from an e3d_export.py JSON.

Each element record carries its owner refno (record word4-5). Linking owner->child
rebuilds the design tree (SITE/ZONE/EQUI/NOZZ/...). Pure offline, from the export.

Usage:  python e3d_tree.py [export.json] [/NAME-to-focus]
"""
import sys, os, json
from collections import defaultdict

PATH = sys.argv[1] if len(sys.argv) > 1 else 'e3d_sam7200_export.json'
FOCUS = sys.argv[2] if len(sys.argv) > 2 else None

doc = json.load(open(PATH, encoding='utf-8'))
elems = doc['elements']
by_ref = {e['refno']: e for e in elems}
children = defaultdict(list)
roots = []
for e in elems:
    owner = e['owner']
    if owner in by_ref and owner != e['refno']:
        children[owner].append(e)
    else:
        roots.append(e)


def label(e):
    return ('%s %s' % (e['noun'], e['name'])).strip() + '  [%s]' % e['refno']


def count_subtree(e, seen):
    if e['refno'] in seen:
        return 0
    seen.add(e['refno'])
    return 1 + sum(count_subtree(c, seen) for c in children.get(e['refno'], []))


def show(e, depth=0, maxdepth=5, maxkids=6, seen=None):
    seen = seen if seen is not None else set()
    if e['refno'] in seen:
        return
    seen.add(e['refno'])
    ch = children.get(e['refno'], [])
    print('  ' * depth + ('+ ' if ch else '- ') + label(e) + (' (%d ch)' % len(ch) if ch else ''))
    if depth >= maxdepth:
        return
    for c in ch[:maxkids]:
        show(c, depth + 1, maxdepth, maxkids, seen)
    if len(ch) > maxkids:
        print('  ' * (depth + 1) + '... %d more' % (len(ch) - maxkids))


print('elements=%d  roots=%d  (linked by owner refno)' % (len(elems), len(roots)))
roots.sort(key=lambda e: -count_subtree(e, set()))
print('\n== top roots by subtree size ==')
for r in roots[:5]:
    print('  %-40s subtree=%d' % (label(r), count_subtree(r, set())))

print('\n== hierarchy (top root, depth<=4) ==')
if roots:
    show(roots[0], maxdepth=4, maxkids=5)

if FOCUS:
    tgt = next((e for e in elems if e['name'] == FOCUS), None)
else:
    tgt = next((e for e in elems if e['noun'] == 'EQUI' and e['name']), None)
if tgt:
    print('\n== branch: %s ==' % (tgt['name'] or tgt['refno']))
    show(tgt, maxdepth=3, maxkids=10)
