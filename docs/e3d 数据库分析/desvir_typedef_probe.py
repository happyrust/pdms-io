# -*- coding: utf-8 -*-
r"""
desvir_typedef_probe.py
=======================
Offline reader for the E3D *schema / template* DABACON databases
(%AVEVA_DESIGN_EXE%\*vir.dat, e.g. desvir.dat = DESIGN schema) that store the
**element type definitions** ("typedef / skeleton K"): the per-attribute
STORAGE-OFFSET tables that db4_get_ce_att uses to read attribute values out of an
element record.

This is the missing piece for *fully offline* attribute decoding. The offsets are
PERSISTED ON DISK in these schema files (NOT accumulated at runtime as previously
hypothesised).

Reverse-engineered from core.dll 2.10 (IDA, base 0x10000000), big-endian on disk:

  DB_SchemaMngr::openAllSchemas (0x10498BE0)
    -> DB_DBSchema::openSchema  (0x10497310)
         path = %AVEVA_DESIGN_EXE%/<schema>.dat   FHFIND "OLD,READ" "DB,BL 512"
         -> db_open_template_db (0x105DC6F0 -> 0x105F44E0)
              -> db2_open_template_db (0x10621850, "2.1.1")

  db2_open_template_db: read_page(handle, page=1, hdr, 8)   [db1_read_page 0x10630C20]
      hdr.word0 == 6           magic / db type
      hdr.word2  = templ type id   (registry key, must match db block store id)
      hdr.word5  = element-type count
      hdr.word7  = type-index (tlu) start page
    tlu = count * 7 words, chain-read (511 data + 1 link word per 512-word page):
      entry[0] = noun hash (ASC sorted)
      entry[1]/[2] = skeleton-K start page / word count   <-- the typedef
      entry[3]/[4] = skeleton-I start page / word count
      entry[5]/[6] = skeleton-J start page / word count

  db2_get_element_details (0x10624400, "2.2.5"): binary-search tlu by noun -> skel K
  typedef block (skeleton K):
      word9  (byte 0x24) = descriptor count
      word14 (byte 0x38) = descriptor array start
      descriptor (stride = desc[1] words):
        [0]=attr hash  [1]=stride  [2]=type  [3]=size
        [5]=MAIN offset|bit   [8]=ALT offset|bit   (offset=&0xFFFFF, bit=>>20)

  db4_get_ce_att (0x10612A50): per element record (start = CE+0x14 data buf + 4*CE+0x40):
      record[0] (u16) = record word count (bounds)
      sel = (record[word10] >> 29) & 1   ->  use desc[5] (sel=0) or desc[8] (sel=3..)
      off = desc[5|8] & 0xFFFFF          ->  record[off] = component COUNT
      data starts at record[off + 1]
        type 5 (bool): bit = (record[off] >> (desc>>20)) & 1   (no count word)
        type 2/6 (real): each component = 2 words, stored LOW-word-first
                         double = bytes( BE(hi=words[k+1]) , BE(lo=words[k]) )
        else (int/ref): each component = 1 word

  page -> byte offset:  off = (page - 1) * 2048

Validated: WELD.POS in pdms-test-data/sam7200_0001 -> (9630.0, 8072.0, 5282.5).
"""
import struct, sys, os

SCHEMA = sys.argv[1] if len(sys.argv) > 1 else r'D:\AVEVA\Everything3D2.10\desvir.dat'
PAGE_BYTES = 2048
DATA_WORDS = 511

_sch = open(SCHEMA, 'rb').read()


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
    def __init__(self, buf):
        self.buf = buf
        hdr = _rw(buf, _po(1), 16)
        assert hdr[0] == 6, 'bad magic %r (page formula/endian?)' % hdr[0]
        self.templ_type = hdr[2]
        self.count = hdr[5]
        self.tlu_page = hdr[7]
        tlu = _chain(buf, self.tlu_page, 7 * self.count)
        self.entries = [tlu[7 * i:7 * i + 7] for i in range(self.count)]

    def find(self, noun_hash):
        e = self.entries
        lo, hi = 0, self.count
        while lo < hi:
            mid = (lo + hi) // 2
            v = e[mid][0]
            if v < noun_hash:
                lo = mid + 1
            elif v > noun_hash:
                hi = mid
            else:
                return mid
        return -1

    def typedef(self, noun_hash):
        """Return {attr_hash: dict(type,size,off,alt,bit,altbit)} for a noun."""
        idx = self.find(noun_hash)
        if idx < 0:
            return None
        e = self.entries[idx]
        skel = _chain(self.buf, e[1], e[2])
        out, i = {}, 14
        for _ in range(skel[9]):
            if i + 8 >= len(skel):
                break
            h, stride, typ, size, m, a = skel[i], skel[i+1], skel[i+2], skel[i+3], skel[i+5], skel[i+8]
            out[h] = dict(type=typ, size=size,
                          off=m & 0xFFFFF, bit=m >> 20,
                          alt=a & 0xFFFFF, altbit=a >> 20)
            if stride <= 0:
                break
            i += stride
        return out


def decode_attr(rec_words, desc, attr_hash):
    """Decode one attribute value from an element record (list of native words)."""
    sel = (rec_words[10] >> 29) & 1
    off = desc['alt'] if sel else desc['off']
    bit = desc['altbit'] if sel else desc['bit']
    t = desc['type']
    if off == 0:
        return None  # pseudo / not stored inline
    if t == 5:  # bool: packed bit at record[off]
        return (rec_words[off] >> bit) & 1
    cnt = rec_words[off]
    if t in (2, 6):  # real -> IEEE754 double, components stored low-word-first
        vals = []
        for j in range(cnt):
            lo = rec_words[off + 1 + 2 * j]
            hi = rec_words[off + 2 + 2 * j]
            vals.append(struct.unpack('>d', struct.pack('>II', hi, lo))[0])
        return vals
    return [rec_words[off + 1 + j] for j in range(max(cnt, 1))]  # int/ref words


if __name__ == '__main__':
    s = Schema(_sch)
    print('schema      :', os.path.basename(SCHEMA))
    print('templ type  : 0x%X' % s.templ_type)
    print('type count  :', s.count, '  tlu sorted:',
          all(s.entries[i][0] <= s.entries[i+1][0] for i in range(s.count-1)))

    WELD, POS, ORI = 0x97247, 0x853B1, 0x83787
    td = s.typedef(WELD)
    print('\nWELD typedef: %d descriptors' % len(td))
    print('  POS desc:', td[POS])
    print('  ORI desc:', td[ORI])

    samp = os.path.join(os.path.dirname(__file__), '..', '..', 'pdms-test-data', 'sam7200_0001')
    if os.path.exists(samp):
        rec = open(samp, 'rb').read()
        W = _rw(rec, 1653784, 46)         # known WELD element record (sam7200_0001)
        assert W[3] == WELD, 'not a WELD record'
        print('\nsam7200 WELD record  (sel=%d -> %s offsets)' %
              ((W[10] >> 29) & 1, 'ALT' if (W[10] >> 29) & 1 else 'MAIN'))
        print('  POS =', decode_attr(W, td[POS], POS), ' (expect [9630.0, 8072.0, 5282.5])')
        print('  ORI =', decode_attr(W, td[ORI], ORI))
