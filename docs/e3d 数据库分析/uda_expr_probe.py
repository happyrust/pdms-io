# -*- coding: utf-8 -*-
r"""
uda_expr_probe.py
=================
Decode 0xFFF-family (expression / derived) UDA value blobs from an E3D/PDMS element db.

Finding (2026-06-06): the `0xFFF?xxxx` UDA family (PDMS_Hash::IsUDA, hash > 0x171FAD39,
top nibble 0xF) does NOT hold a literal strong-typed value; its value is a **serialized
PDMS expression** (a derived / computed attribute rule), in the SAME opcode language as
ordinary expression attributes (PHEI etc., see docs/expression_opcode_table.md and
crates/parse_pdms_db/src/parser/attribute/expression_payload.rs).

UDA expression blob framing (the DA-entry value words, after [hash][ctrl: type7<<26|wc]):
    [len][0][count][sublen=len-2][1]  <RPN word stream...>
The RPN stream is the same grammar:
    0x65 number, 0x66/0x76 string, 0x67 bool, 0x6A attribute-reference
    (followed by 5 words [_, hash, low, high, suffix] and optional qualifier trailers
     2100=fn-args, 2200, 1601/1602=OF, 1701/1702/1703=WRT), operators 80x/90x/100x/...
    0x6B/0x6C/0x6D/0x73 = DORTXT geometric value literals (direction/orientation/position
    /coordinate, 's'=AT). Confirmed in core.dll EXRTPD (0x10080F62) switch cases
    'k'/'l'/'m'/'s' -> DORTXT(...); count-prefixed: count=words[i+1], advance i+1+count.
    For derived UDAs these hold ONE parametric geometric value whose components are
    sub-expressions (e.g. parametric section geometry from catalogue CDPR.* params).

This is a faithful Python port of `expression_payload.rs::decode_words` for the scalar
subset (attribute-refs + arithmetic), enough to render derived-UDA formulas offline, e.g.:
    ATTRIB HEIG OF CYLI WRT
    ATTRIB CDPR FTHK OF WRT * 2 + ATTRIB CDPR GTHK OF WRT + 37.5
    MIN ( ABS ( 300 * N - INT ( ATTRIB LDPR OLEN OF TMPL WRT / 300 ) * 300 - ... ) , 12 )

Read-only.  Usage:  python uda_expr_probe.py [db_file]
"""
import sys, os, struct
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from e3d_db_reader_v2 import E3DDb, looks_like_noun
from e3d_attr_decoder import decode_da_list, db1_dehash, UDA_THRESHOLD
from collections import Counter

S32 = lambda w: w - 0x100000000 if w >= 0x80000000 else w   # interpret word as i32


def value_base(a2, a3, a4):
    """Port of expression_payload.rs::decode_value_expr_base (0x65 numeric constant)."""
    a2u, a3u, a4u = a2 & 0xFFFFFFFF, a3 & 0xFFFFFFFF, a4 & 0xFFFFFFFF
    if (a4u & 0xC0000000) != 0x40000000:
        v = S32(a2) * 0.000030517578125 + S32(a3) * 9.313225746154785e-10
        return v * (2.0 ** S32(a4))
    hi = (a2u & 0x1FFFFF) | ((a4u & 0x7FF) << 20)
    if a2u & 0x40000000:
        hi |= 0x80000000
    return struct.unpack('>d', struct.pack('>Q', (hi << 32) | a3u))[0]


def fmt6(v):
    t = '%.6f' % v
    if '.' in t:
        t = t.rstrip('0').rstrip('.')
    return '0' if t in ('-0', '') else t


OPS = {301: ('NOT', 1), 302: ('AND', 2), 303: ('OR', 2), 401: ('EQ', 2), 501: ('NEQ', 2),
       601: ('GT', 2), 602: ('GT', 2), 603: ('LT', 2), 605: ('GE', 2), 607: ('LE', 2),
       801: ('NEG', 1), 802: ('+', 2), 803: ('-', 2), 804: ('*', 2), 805: ('/', 2),
       901: ('SIN', 1), 902: ('COS', 1), 903: ('TAN', 1), 904: ('ASIN', 1), 905: ('ACOS', 1),
       906: ('ATAN', 1), 907: ('ATAN2', 2), 1001: ('SQRT', 1), 1002: ('POW', 2), 1003: ('LOG', 1),
       1005: ('INT', 1), 1006: ('NINT', 1), 1007: ('ABS', 1), 1008: ('MAX', 2), 1009: ('MIN', 2),
       1301: ('LENGTH', 1), 1309: ('SUBSTRING', 3), 1314: ('TRIM', 1), 1321: ('OCCURS', 2),
       1822: ('IFTRUE', 3), 1824: ('DISTCONVERT', 1), 1825: ('SET', 2), 1826: ('UNSET', 1)}

VALUE_OPS = (0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6F, 0x72, 0x74, 0x75, 0x76)


def _apply(op, st):
    name, argc = OPS[op]
    if len(st) < argc:
        raise ValueError('stack underflow %d' % op)
    args = [st.pop() for _ in range(argc)][::-1]
    if op == 801:
        return '- %s' % args[0]
    if op == 301:
        return 'NOT %s' % args[0]
    if argc == 2 and name in ('+', '-', '*', '/', 'AND', 'OR', 'EQ', 'NEQ', 'GT', 'LT', 'GE', 'LE'):
        return '%s %s %s' % (args[0], name, args[1])
    return '%s ( %s )' % (name, ' , '.join(args))


def decode_words(words, strict=True):
    st, i = [], 0
    while i < len(words):
        op = words[i]
        v = parse_value(op, words, i, st)
        if v is not None:
            node, i = v
            st.append(node)
            continue
        if op in OPS:
            if len(st) < OPS[op][1]:
                if strict:
                    raise ValueError('stack underflow %d' % op)
                break
            st.append(_apply(op, st))
            i += 1
            continue
        if strict:
            raise ValueError('unknown opcode %d (0x%X)' % (op, op))
        break
    if strict and len(st) != 1:
        raise ValueError('residual stack %d' % len(st))
    return st[0] if len(st) == 1 else ' | '.join(st)


def parse_value(op, words, i, st):
    n = len(words)
    if op == 0x65:
        cnt = words[i + 1]; dc = cnt - 1; ds = i + 2; de = ds + dc
        if dc < 3:
            return (str(S32(words[ds])), de)
        base = value_base(words[ds], words[ds + 1], words[ds + 2])
        exp = words[ds + 3] if ds + 3 < n else 0
        return (fmt6(base) + ((' EX %d' % S32(exp)) if exp else ''), de)
    if op in (0x66, 0x76, 0x68):
        ln = words[i + 1]; ds = i + 2; de = ds + ln
        b = bytes((w & 0xFF) for w in words[ds:de])
        return ("'%s'" % b.decode('latin1', 'replace'), de)
    if op == 0x67:
        return ('true' if words[i + 1] == 201 else 'false', i + 2)
    if op == 0x6F:
        return ('PI', i + 1)
    if op in (0x72, 0x74, 0x75):
        nm = db1_dehash(abs(S32(words[i + 1])))
        return (nm or ('0x%08X' % (words[i + 1] & 0xFFFFFFFF)), i + 2)
    if op == 0x6A:
        ds = i + 1; de = ds + 5
        hashv, low, high, suf = words[ds + 1], S32(words[ds + 2]), S32(words[ds + 3]), S32(words[ds + 4])
        hv = abs(S32(hashv))
        name = db1_dehash(hv) or 'unknown attribute'
        text = ('ATTRIB %s' % name) if hv <= 387951929 else name
        if suf != 0:
            sn = db1_dehash(abs(suf))
            text += ' ' + (sn if sn else (str(suf) if 0 < suf < 531442 else '0x%08X' % (suf & 0xFFFFFFFF)))
        elif low == -1 and st:
            text += '[%s ]' % st.pop()
        elif low != 0 and not (low == 1 and high == 1 and name != 'PARA'):
            text += ('[%d ]' % low) if (high == low or high == 0) else (' %d TO %d' % (low, high))
        cur = de
        if cur < n and words[cur] == 2100:
            cur += 1; argc = words[cur]; cur += 1
            for _ in range(argc):
                if st:
                    st.pop()
                cur += 1
        if cur < n and words[cur] == 2200:
            cur += 3

        def block_attr(c):
            ln = words[c]; end = c + ln
            tgt = db1_dehash(abs(S32(words[c + 2]))) if (ln >= 2 and c + 2 < n) else ''
            return tgt, end
        if cur < n and words[cur] in (1601, 1602):
            if words[cur] == 1601:
                cur += 1; text += ' OF'
            else:
                cur += 1; tgt, cur = block_attr(cur); text += ' OF %s' % (tgt or '?')
        if cur < n and words[cur] in (1701, 1702, 1703):
            if words[cur] == 1703:
                tgt = db1_dehash(abs(S32(words[cur + 1]))) if cur + 1 < n else ''
                cur += 3; text += ' WRT %s' % (tgt or '?')
            elif words[cur] == 1702:
                cur += 1; tgt, cur = block_attr(cur); text += ' WRT %s' % (tgt or '?')
            else:
                cur += 1; text += ' WRT'
        return (text, cur)
    if op in (0x6B, 0x6C, 0x6D, 0x73):
        # DORTXT geometric value literal (direction/orientation/position/coordinate),
        # count-prefixed: count = words[i+1]; the block is one parametric geometric value
        # whose components are sub-expressions. (core.dll EXRTPD case 'k'/'l'/'m'/'s'.)
        count = words[i + 1]
        de = min(i + 1 + count, n)
        inner = words[i + 2:de]
        kind = {0x6B: 'GEOM_K', 0x6C: 'GEOM_L', 0x6D: 'GEOM_M', 0x73: 'AT'}[op]
        comps = _inner_components(inner)
        body = (' : '.join(comps)) if comps else ('%dw' % count)
        return ('%s( %s )' % (kind, body), de)
    return None


def _inner_components(inner):
    """Best-effort: surface the component sub-expressions inside a 0x6B/0x6C/0x6D
    geometric literal (each component is itself an RPN expression)."""
    out, j, n = [], 0, len(inner)
    while j < n:
        if inner[j] in (0x6A, 0x65, 0x67, 0x66, 0x76):
            seg = inner[j:]
            try:
                txt = decode_words(seg, strict=False)
            except Exception:
                txt = ''
            if txt:
                out.append(txt)
            break  # decode_words(strict=False) already walks the remainder
        j += 1
    return out


def expr_body(v):
    """Strip the UDA expression header [len][0][count][sublen=len-2][1] -> RPN slice."""
    if len(v) >= 6 and v[1] == 0 and v[3] == v[0] - 2 and v[4] == 1 and v[5] in VALUE_OPS:
        return v[5:]
    for i in range(1, min(len(v) - 1, 8)):
        if v[i] == 1 and v[i + 1] in VALUE_OPS:
            return v[i + 1:]
    return v[1:]


def decode_uda_expr(value_words):
    """Render a 0xFFF UDA value-blob as a PDMS expression string (best-effort)."""
    body = expr_body(value_words)
    try:
        return decode_words(body, strict=True)
    except Exception:
        partial = decode_words(body, strict=False)
        return ('~ ' + partial) if partial else None


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
        os.path.dirname(__file__), '..', '..', 'pdms-test-data', 'sam7200_0001')
    db = E3DDb(path)
    root = db.session_chain()[0]['index_root_pgno']
    by_hash, raw_by_hash, decoded = Counter(), {}, Counter()
    ok = fail = 0
    seen = set()
    for r0, r1, pg, off in db.walk_index(root):
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
        try:
            da = decode_da_list(db.blob, bo, 1)
        except Exception:
            continue
        for a in da:
            h = a['hash']
            if h <= UDA_THRESHOLD or (h >> 28) != 0xF or not isinstance(a['value'], list):
                continue
            by_hash['0x%X' % h] += 1
            raw_by_hash.setdefault(h, a['value'])
            expr = decode_uda_expr(a['value'])
            if expr and not expr.startswith('~'):
                ok += 1; decoded[expr] += 1
            else:
                fail += 1
    print('=== 0xFFF-family (expression) UDA on %s ===' % os.path.basename(path))
    print('distinct hashes=%d  entries: full-decode=%d  partial/list=%d' % (len(by_hash), ok, fail))
    print('\n=== top decoded derived-UDA expressions ===')
    for e, c in decoded.most_common(15):
        print('  x%-4d %s' % (c, e))


if __name__ == '__main__':
    main()
