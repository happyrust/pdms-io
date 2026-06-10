# -*- coding: utf-8 -*-
r"""
e3d_attr_decoder.py
===================
Fully-offline E3D named-attribute decoder.

Given an element record (big-endian word list) it:
  1. reads the noun hash (record word[3]),
  2. auto-selects the schema among %AVEVA_DESIGN_EXE%\*vir.dat that defines that
     noun (mirrors db2_get_element_definition iterating all template dbs),
  3. loads the on-disk type-def and decodes EVERY inline-stored named attribute
     (name via db1_dehash, value per type).

No runtime / IDA needed. Builds on desvir_typedef_probe.py findings (format spec
§7.6 / §7.7). Endianness: big-endian; page size 2048 B; page->off = (page-1)*2048.

Decode rule (db4_get_ce_att):
  sel = (record[word10] >> 29) & 1   -> use desc[5] (sel=0) else desc[8]
  off = desc & 0xFFFFF               (off==0 => not stored inline: pseudo/array/text-heap)
  type 5 (bool) : (record[off] >> bit) & 1            (no count word)
  type 2/6 real : record[off]=count; data @off+1, each = 2 words LOW-word-first double
  else          : record[off]=count; data @off+1, each = 1 word (int/enum/ref)
"""
import struct, os, glob, sys

BASE27_OFFSET = 0x81BF1
UDA_THRESHOLD = 0x171FAD39
PAGE_BYTES = 2048
DATA_WORDS = 511


def db1_dehash(value):
    """base-27 reversible name hash (offset 0x81BF1). UDA (>0x171FAD39) flagged.
    For UDAs we return ':UDA_0x<id>' (stable, unambiguous). The real UDA *name*
    is NOT in the hash: core.dll renders it via DB_Attribute::findAttribute (ATATXT
    0x10467D70) from the UDA dictionary registry. DEHASH (0x1065B930) only yields a
    LOSSY base-64 short-code (see dehash_uda_code)."""
    if value > UDA_THRESHOLD:
        return ':UDA_0x%X' % ((value - UDA_THRESHOLD) % 0x1000000)
    if value <= BASE27_OFFSET:
        return ''
    v = value - BASE27_OFFSET
    out = []
    while v > 0:
        d = v % 27
        out.append(' ' if d == 0 else chr(d + 64))
        v //= 27
    return ''.join(out).strip()


def dehash_uda_code(value):
    """Faithful port of core.dll DEHASH (0x1065B930) UDA branch: a deterministic but
    LOSSY base-64 short-code ':' + up to 4 chars, char = chr((v%64)+32), v//=64,
    v = (hash-0x171FAD39) % 0x1000000. NOTE: this is NOT the human-readable UDA name
    (which lives in the UDA dictionary db); it can contain non-identifier chars."""
    if value <= UDA_THRESHOLD:
        return None
    v = (value - UDA_THRESHOLD) % 0x1000000
    out = [':']
    for _ in range(4):
        d = v % 64
        out.append(' ' if d <= 0 else chr(d + 32))
        v //= 64
    return ''.join(out).rstrip()


def _po(p):
    return (p - 1) * PAGE_BYTES


def _rw(buf, off, n):
    return list(struct.unpack('>%dI' % n, buf[off:off + 4 * n]))


def _chain(buf, start_page, total_words):
    out, page, rem = [], start_page, total_words
    while rem > DATA_WORDS:
        wp = _rw(buf, _po(page), 512)
        out += wp[:DATA_WORDS]
        page = wp[DATA_WORDS]
        rem -= DATA_WORDS
    out += _rw(buf, _po(page), rem)[:rem]
    return out


class Schema:
    def __init__(self, path):
        self.path = path
        self.name = os.path.basename(path)
        self.buf = open(path, 'rb').read()
        h = _rw(self.buf, _po(1), 16)
        self.ok = (h[0] == 6)
        self.templ, self.count, self.tlu_page = h[2], h[5], h[7]
        self.index = {}
        if self.ok and 0 < self.count < 100000:
            tlu = _chain(self.buf, self.tlu_page, 7 * self.count)
            for i in range(self.count):
                e = tlu[7 * i:7 * i + 7]
                self.index[e[0]] = (e[1], e[2])  # noun -> (skelK page, word count)

    def typedef(self, noun):
        if noun not in self.index:
            return None
        kp, kc = self.index[noun]
        skel = _chain(self.buf, kp, kc)
        if len(skel) < 10:           # degenerate/short type-def (seen in amssys): no descriptors
            return None
        out, i = {}, 14
        for _ in range(skel[9]):
            if i + 8 >= len(skel):
                break
            h, stride = skel[i], skel[i + 1]
            out[h] = dict(type=skel[i+2], size=skel[i+3],
                          off=skel[i+5] & 0xFFFFF, bit=skel[i+5] >> 20,
                          alt=skel[i+8] & 0xFFFFF, altbit=skel[i+8] >> 20)
            if stride <= 0:
                break
            i += stride
        return out


class SchemaSet:
    def __init__(self, folder):
        self.schemas, self.noun2schema, self.by_templ = [], {}, {}
        for p in sorted(glob.glob(os.path.join(folder, '*vir.dat'))):
            try:
                s = Schema(p)
            except Exception:
                continue
            if s.ok and s.index:
                self.schemas.append(s)
                self.by_templ.setdefault(s.templ, s)   # template_id -> schema (db header 0x20)
                for noun in s.index:
                    self.noun2schema.setdefault(noun, s)

    def typedef(self, noun):
        s = self.noun2schema.get(noun)
        return (s, s.typedef(noun)) if s else (None, None)

    def schema_for_db(self, db_buf):
        """Pick a db's primary schema library from its header word8 (0x20 = schema/
        template type id), per format spec §2 / db2_open_db. Returns the Schema or None.
        Avoids guessing/loading-all: e.g. design db (0xB0692)->desvir, catalogue
        (0x8A1E6)->catvir, system (0xE567E)->sysvir."""
        if len(db_buf) < 0x24:
            return None
        templ = struct.unpack_from('>I', db_buf, 0x20)[0]
        return self.by_templ.get(templ)


def _f32(w):
    return struct.unpack('>f', struct.pack('>I', w))[0]


def _f64_lowfirst(lo, hi):
    return struct.unpack('>d', struct.pack('>II', hi, lo))[0]


# typedef desc[2] "type" enum (distinct from attlib §7.1 TYPE; empirically derived):
#   2/6 = real   3/7 = integer   4/8/16 = reference/word(2w)   5 = boolean(bit)
#   10/11/12 = array(usually off=0)   14/15/18/19 = text/special(count-prefixed)
_REAL = {2, 6}
_INT = {3, 7}
_REF = {4, 8, 16}
_PREFIX_TYPES = {14, 15, 18, 19}


def _decode_one(W, desc):
    """Decode one attribute. db4 v68 rule: scalar(size==1, non-text) is stored
    directly at off (no count word); size>1 / text is count-prefixed (count@off,
    data@off+1). sel picks main(desc5)/alt(desc8); alt reals = doubles(2w,low-first),
    main reals = floats(1w)."""
    sel = (W[10] >> 29) & 1
    off = desc['alt'] if sel else desc['off']
    bit = desc['altbit'] if sel else desc['bit']
    t, size = desc['type'], desc['size']
    if off == 0 or off >= len(W):
        return None
    if t == 5:                                          # boolean: packed bit
        return bool((W[off] >> bit) & 1)
    prefixed = (size > 1) or (t in _PREFIX_TYPES)
    if prefixed:
        cnt, data = W[off], off + 1
        if not (0 <= cnt < 4096):
            return ('rawcount', W[off])
    else:
        cnt, data = size, off                            # scalar: value directly at off
    if t in _REAL:
        if sel:                                          # alt: 64-bit doubles, low-word-first
            return [_f64_lowfirst(W[data + 2*j], W[data + 2*j + 1])
                    for j in range(cnt) if data + 2*j + 1 < len(W)]
        return [_f32(W[data + j]) for j in range(cnt) if data + j < len(W)]  # main: floats
    if t in _REF:                                        # reference: 2-word (dbno, refno)-ish
        return [(W[data + 2*j], W[data + 2*j + 1])
                for j in range(cnt) if data + 2*j + 1 < len(W)]
    return [W[data + j] for j in range(cnt) if data + j < len(W)]  # int / word / other


EXPLICIT_TEXT_TYPES = {10, 14, 15}
NAME_HASH = 0x9C18E   # db1_hash("NAME"); also the type-7 data-page magic


def _u32(buf, o):
    return struct.unpack_from('>I', buf, o)[0]


def _parse_attr_words(words, max_entries=256):
    """Parse a flat word list of [hash][ctrl:(type<<26)|wordcount][value...] entries.
    Text (types 10/14/15): value = [length][packed 4 chars/word, MSB-first]."""
    out, i = [], 0
    while len(out) < max_entries and i + 1 < len(words):
        h, ctrl = words[i], words[i + 1]
        typ, n = ctrl >> 26, ctrl & 0x3FFFFFF
        if h == 0 or not (0 < n <= 256) or i + 2 + n > len(words):
            break
        if typ in EXPLICIT_TEXT_TYPES:
            ln = words[i + 2]
            chars = b''.join(struct.pack('>I', w & 0xFFFFFFFF) for w in words[i + 3:i + 2 + n])
            # E3D stores text as UTF-8 (e.g. Chinese names 穹顶/天花板); latin1 would mojibake.
            val = chars[:ln].decode('utf-8', 'replace') if 0 <= ln <= len(chars) else None
        else:
            val = [words[i + 2 + k] for k in range(n)]
        out.append(dict(name=db1_dehash(h), hash=h, type=typ, value=val))
        i += 2 + n
    return out


def decode_explicit_attrs(buf, byte_off, max_entries=64):
    """Parse an element's EXPLICIT-attribute region at byte_off (single contiguous run)."""
    nwords = min((len(buf) - byte_off) // 4, 4096)
    words = list(struct.unpack_from('>%dI' % nwords, buf, byte_off)) if nwords > 0 else []
    return _parse_attr_words(words, max_entries)


def extract_names(buf):
    """Scan a db file for element NAME entries -> list of (byte_off, name).
    A NAME entry = [0x9C18E][ctrl with type==15][length][packed chars]."""
    out, n = [], len(buf) // 4
    for i in range(n - 2):
        o = i * 4
        if _u32(buf, o) != NAME_HASH or (_u32(buf, o + 4) >> 26) != 15:
            continue
        ln = _u32(buf, o + 8)
        if 0 < ln <= 256 and o + 12 + ln <= len(buf):
            s = buf[o + 12:o + 12 + ln].decode('utf-8', 'replace')  # UTF-8 (supports CJK names)
            if s and all(ord(c) >= 32 and c != '\uFFFD' for c in s):  # printable, no control / bad bytes
                out.append((o, s))
    return out


def decode_element(ss, W):
    """Decode all inline named attributes of an element record (word list W)."""
    noun = W[3]
    s, td = ss.typedef(noun)
    sel = (W[10] >> 29) & 1
    res = dict(noun=noun, noun_name=db1_dehash(noun),
               schema=s.name if s else None, sel=sel, count_word=W[0] & 0xFFFF, attrs=[])
    if not td:
        return res
    for h, desc in td.items():
        off = desc['alt'] if sel else desc['off']
        if off == 0:
            continue
        res['attrs'].append(dict(name=db1_dehash(h), hash=h, type=desc['type'],
                                 size=desc['size'], off=off, value=_decode_one(W, desc)))
    res['attrs'].sort(key=lambda a: a['off'])
    return res


def decode_da_list(buf, record_byte_off, which=1, page_size=2048):
    """Decode an element's DA/explicit list (which=1) or members (which=2) from its
    record header (db4_get_list framing):
      DA:      page=rec[6], off=(rec[7]>>13)&0xFFF, words=(rec[10]>>14)&0x3FFF
      members: page=rec[8], off=(rec[9]>>13)&0xFFF, words= rec[10]&0x3FFF
    The referenced location holds a node: 5-word header [(u16)=payload+5 | type<<16],
    then [hash][ctrl][value] entries (text 10/14/15 = [len][packed chars])."""
    rec = lambda i: _u32(buf, record_byte_off + 4 * i)
    w10 = rec(10)
    if which == 1:
        page, loc, words = rec(6), rec(7), (w10 >> 14) & 0x3FFF
    else:
        page, loc, words = rec(8), rec(9), w10 & 0x3FFF
    if words == 0 or page == 0:
        return []
    off = (loc >> 13) & 0xFFF
    payload, remaining, guard = [], words, 0
    while page and remaining > 0 and guard < 128:   # follow node chain across pages
        guard += 1
        node = page * page_size + off * 4
        if node + 20 > len(buf):
            break
        hdr = _u32(buf, node)
        if ((hdr >> 16) & 0xF) != which:
            break
        plen = (hdr & 0xFFFF) - 5                    # payload words in this node
        if plen <= 0:
            break
        take = min(plen, remaining, max(0, (len(buf) - (node + 20)) // 4))
        payload += [_u32(buf, node + 20 + 4 * i) for i in range(take)]
        remaining -= plen
        page, off = _u32(buf, node + 12), (_u32(buf, node + 16) >> 13) & 0xFFF   # node[3]=next page, node[4]=next off
    return _parse_attr_words(payload, 512)


def decode_full_element(ss, buf, record_byte_off):
    """Full offline element decode: header + implicit (typedef) + DA/explicit + name."""
    impl = _u32(buf, record_byte_off) & 0xFFFF
    W = _rw(buf, record_byte_off, min(impl, 256)) if 0 < impl <= 4096 else []
    res = decode_element(ss, W) if len(W) >= 11 else dict(noun=None, attrs=[])
    res['refno'] = (_u32(buf, record_byte_off + 4), _u32(buf, record_byte_off + 8))
    res['owner'] = (_u32(buf, record_byte_off + 16), _u32(buf, record_byte_off + 20))
    res['da'] = decode_da_list(buf, record_byte_off, 1)   # DA/explicit attrs (chained-aware)
    res['member_count'] = _u32(buf, record_byte_off + 40) & 0x3FFF  # children (via owner links)
    nm = [a for a in res['da'] if a['hash'] == NAME_HASH]
    res['element_name'] = nm[0]['value'] if nm else None
    return res


if __name__ == '__main__':
    exe = sys.argv[1] if len(sys.argv) > 1 else r'D:\AVEVA\Everything3D2.10'
    ss = SchemaSet(exe)
    print('loaded %d schemas, %d total noun types' % (len(ss.schemas), len(ss.noun2schema)))

    def find_record(buf, noun):
        """Scan a raw page/file for an element record: w[i]==noun and w[i-3] is a
        small impl count -> record base = (i-3)*4."""
        n = len(buf) // 4
        for i in range(3, n):
            if struct.unpack('>I', buf[4*i:4*i+4])[0] == noun:
                cnt = struct.unpack('>I', buf[4*(i-3):4*(i-3)+4])[0] & 0xFFFF
                if 8 <= cnt <= 4096:
                    return (i - 3) * 4
        return None

    here = os.path.dirname(os.path.abspath(__file__))
    td = os.path.join(here, '..', '..', 'pdms-test-data')
    samples = [
        ('sam7200_0001', os.path.join(td, 'sam7200_0001'), 1653784),
        ('ele_data_0', os.path.join(td, 'ele_data_0'), None),
    ]
    for label, path, base in samples:
        if not os.path.exists(path):
            continue
        raw = open(path, 'rb').read()
        if base is None:
            base = find_record(raw, 0x97247)
            if base is None:
                print('\n=== %s: no WELD record found ===' % label)
                continue
        W = _rw(raw, base, 46)
        label = '%s @%d' % (label, base)
        r = decode_element(ss, W)
        print('\n=== %s  noun=0x%X(%s) schema=%s sel=%d words=%d ===' %
              (label, r['noun'], r['noun_name'], r['schema'], r['sel'], r['count_word']))
        for a in r['attrs']:
            print('  %-6s t=%-2d s=%-4d off=%-3d = %s' %
                  (a['name'] or '0x%X' % a['hash'], a['type'], a['size'], a['off'], a['value']))
