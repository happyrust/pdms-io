# -*- coding: utf-8 -*-
r"""
e3d_write_full.py
=================
Offline **FULL write** (copy-on-write + new-session commit) for E3D / PDMS element
databases. This is the offline reconstruction of the ``db5_save_work`` commit
mechanism (IDA-confirmed, core.dll 2.10, 0x105E9C80 "5.4.4"; see findings §12 and
格式规范 §12.4).

What a real PDMS "SAVEWORK" does, and what this does offline:
  - **Never mutate an existing page.** Every changed page is written as a *new* page
    appended at the end of the file (copy-on-write). Old pages stay byte-identical,
    so previous sessions remain fully readable (multi-version / append-only db).
  - **COW the path, not the tree.** To change one element's value we COW exactly:
      data page (with the edited inline value)
        -> B-tree leaf page (its entry now points at the new data page)
          -> every ancestor index page up to the root (each child pointer rewired)
    Sibling subtrees are *shared* with the previous version (only O(depth) new pages).
  - **Append a new session page** (``sesno = old+1``) whose ``index_root_pgno`` is the
    new root, linked to the previous session via ``last_ses_pgno``.
  - **Repoint page 0** to the new session.

Result is genuinely multi-version, verifiable purely offline:
  - the LATEST session resolves the element to the NEW value;
  - the PREVIOUS session still resolves it to the OLD value (old pages untouched);
  - byte-diff vs the original: in the original page range ONLY page 0's session
    pointer (``0x28``) changes; everything else is appended.

NOTE on duplicate index keys: an element refno can appear in MORE than one leaf entry
(the main record + secondary/member structures). The main record is the one whose
word0 is a clean u16 implicit-word count and whose noun dehashes cleanly (``_is_main``).
We commit against the exact main-record leaf entry and read back the main record only.

SAFETY / SCOPE
  - ALWAYS operates on a COPY (the demos copy the source first).
  - Slice 1 (``cow_commit``): fixed-size INLINE value edit (real/int/ref, same component
    count) — see §12.2.
  - Slice 2 (``cow_commit_da_text``): variable-length DA/explicit TEXT edit (e.g. NAME),
    same-page single-node DA; growth allowed up to the page / member-region bound (the
    COW'd page is referenced only by the edited element, so its tail is free space).
  - The shared commit core (``_commit_edited_data_page``) also makes the relocated record
    self-contained: in-page page pointers (DA word6 / member word8) are repointed to the
    new page.
  - Slice 3/4 (``cow_insert_element`` / ``cow_delete_element``): create / delete an element
    (B-tree max-key append / leaf-entry removal) — see §12.6.
  - Slice 5 (``cow_insert_element_split`` / ``cow_insert_leaf`` / ``_btree_insert``): arbitrary-
    key insert with recursive node split + new root (db3_split_node/db3_split_root). Validated
    by ``nav_ok`` (PDMS binary-search reachability), not strict separator==child-min.
  - Slice 6 (``cow_commit_da_text_xpage``): cross-page / chained DA TEXT rewrite via multi-page
    COW — reads the DA chain-aware, edits, re-emits a fresh node chain (single or chunked),
    repoints rec[6]/rec[7]/rec[10]. Generalises Slice 2 (covers another-page / already-chained
    DA + unbounded growth).
  - Slice 7 (``cow_members_set``): MEMBER (child refno) list rewrite via the same multi-page COW
    relocation as Slice 6 but on the type-2 node chain (rec[8]/rec[9]/rec[10] member bits) whose
    payload is a flat (r0,r1) child-refno array — covers relocate / add-child / remove-child.
  - Slice 8 (``cow_da_set_entry`` / ``cow_da_remove_entry`` / ``read_uda``): generic DA-region
    entry value set/add/remove by hash (motivating use = UDA values, hash > UDA_THRESHOLD), via
    the shared ``_relocate_da_payload`` core — strongly-typed (ref/word/real/text) values.
  - Not yet: Rust port; UDA 0xFFF-family expression-AST editing (raw-word rewrite works, AST
    semantics out of scope). Real running-E3D open is mechanism-confirmed (db2_open_db loads the
    db-block — sesno/index_root/end/claim — from the session page we rewrite; 格式规范 §12.6)
    but NOT physically round-tripped here.

Usage (demos: copy sam7200, COW-commit POS [slice1] + NAME rename [slice2], verify):
    python e3d_write_full.py [<db_file>] [--exe DIR] [--name /WB1]
"""
import os
import sys
import struct
import shutil

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from e3d_db_reader_v2 import E3DDb, INDEX_NOUN, looks_like_noun
from e3d_attr_decoder import SchemaSet, decode_full_element, UDA_THRESHOLD
from e3d_write import set_inline_value, find_element

SENTINEL = 0x80000001
POS_HASH = 0x853B1
NAME_HASH = 0x9C18E        # db1_hash("NAME"); element name lives in the DA/explicit region
TEXT_TYPES = {10, 14, 15}  # DA/explicit text attribute types (see _parse_attr_words)

# session-page field byte offsets (within the page) — see e3d_db_reader_v2.parse_session
SES_LAST = 0x04   # last_ses_pgno (link to previous session)
SES_SESNO = 0x0C  # session number
SES_END = 0x14    # end / last allocated page
SES_ROOT = 0x1C   # index_root_pgno
HDR_LATEST = 0x28  # page-0 header: latest session pgno (word 10)


def _u32(buf, off):
    return struct.unpack_from(">I", buf, off)[0]


def _put32(buf, off, val):
    struct.pack_into(">I", buf, off, val & 0xFFFFFFFF)


def is_main_record(db: "E3DDb", cpg: int, off: int) -> bool:
    """True iff (cpg, off) points at a clean MAIN element record (not a secondary /
    member structure). Mirrors the reader's record-validity filter."""
    bo = cpg * db.page_size + off * 2
    if off == 0 or bo + 44 > len(db.blob):
        return False
    w0 = db.u32(bo)
    return (w0 >> 16) == 0 and 8 <= (w0 & 0xFFFF) <= 512 and looks_like_noun(db.u32(bo + 12))


def find_leaf_path(db: "E3DDb", root: int, match):
    """DFS the B-tree exactly like the reader, recording the path to the first leaf
    entry for which ``match(r0, r1, cpg, off)`` is true.

    Returns ``(path, cpg, off)`` or ``None``:
      - ``path`` = list of ``(index_pgno, entry_byte_off)`` from root down to that
        leaf entry (byte offsets are absolute, into ``db.blob`` / a same-layout copy);
      - ``cpg`` / ``off`` = the matched leaf entry's data page and word offset.
    """
    pw = db.page_size // 4

    def is_index_page(pg):
        return 0 < pg < db.n_pages and db.u32(db.page_off(pg) + 4) == INDEX_NOUN

    def rec(pg, depth, chain):
        if not is_index_page(pg) or depth > 40:
            return None
        base = db.page_off(pg)
        # word6-bounded entries (NOT null-terminated); descend ALL children incl.
        # the 0x80000001 sentinel leftmost child (smallest-key subtree). See findings §16.
        nent = (pw - 7 - db.u32(base + 24)) // 4
        for k in range(nent):
            eoff = base + (7 + 4 * k) * 4
            r0 = db.u32(eoff)
            r1 = db.u32(eoff + 4)
            cpg = db.u32(eoff + 8)
            off = db.u32(eoff + 12) >> 12
            if off == 0 and is_index_page(cpg):
                res = rec(cpg, depth + 1, chain + [(pg, eoff)])
                if res:
                    return res
            elif (r0, r1) != (SENTINEL, SENTINEL) and match(r0, r1, cpg, off):
                return (chain + [(pg, eoff)], cpg, off)
        return None

    return rec(root, 0, [])


def _append_page(buf: bytearray, page: bytes, page_size: int) -> int:
    """Append a full page to ``buf``; return its new page number."""
    assert len(buf) % page_size == 0, "file not page-aligned"
    assert len(page) == page_size, "page wrong size"
    pgno = len(buf) // page_size
    buf.extend(page)
    return pgno


def _append_session(buf: bytearray, db: "E3DDb", new_root: int) -> dict:
    """Append a new session page (clone of the current one) with sesno+1, the given
    index root, linked to the previous session; repoint page0. Returns session report."""
    PS = db.page_size
    old_ses_pg = db.latest_ses_pgno
    new_ses = bytearray(buf[old_ses_pg * PS:(old_ses_pg + 1) * PS])
    old_sesno = _u32(new_ses, SES_SESNO)
    old_end = _u32(new_ses, SES_END)
    _put32(new_ses, SES_SESNO, old_sesno + 1)
    _put32(new_ses, SES_ROOT, new_root)
    _put32(new_ses, SES_LAST, old_ses_pg)
    new_ses_pg = _append_page(buf, bytes(new_ses), PS)
    _put32(buf, new_ses_pg * PS + SES_END, new_ses_pg)
    _put32(buf, HDR_LATEST, new_ses_pg)             # repoint page0 to the new session
    return {"old_ses_pg": old_ses_pg, "new_ses_pg": new_ses_pg,
            "old_sesno": old_sesno, "new_sesno": old_sesno + 1, "old_end_pgno": old_end}


def _commit_edited_data_page(buf: bytearray, db: "E3DDb", record_bo: int,
                             edited_page: bytes) -> dict:
    """Core COW commit: given ``edited_page`` (a full page = the MAIN record's data page
    with the edit already applied), append it, COW the B-tree path root->leaf to point at
    it, append a new session (sesno+1) and repoint page 0. Mutates ``buf``; returns report.

    This is the shared `db5_save_work` machinery used by every write slice — the slice
    only decides HOW to produce ``edited_page`` (inline value, DA text, ...)."""
    PS = db.page_size
    sess = db.session_chain()
    if not sess:
        raise ValueError("no session chain")
    if len(edited_page) != PS:
        raise ValueError("edited_page wrong size")
    old_root = sess[0]["index_root_pgno"]
    data_pgno = record_bo // PS
    data_off = (record_bo % PS) // 2
    refno = (_u32(buf, record_bo + 4), _u32(buf, record_bo + 8))

    # path to the EXACT main-record leaf entry (match by data page+offset; unambiguous)
    found = find_leaf_path(db, old_root, lambda r0, r1, cpg, off: cpg == data_pgno and off == data_off)
    if not found:
        raise ValueError("leaf entry for record @%d not found under root %d" % (record_bo, old_root))
    path, _, _ = found

    new_data_pg = _append_page(buf, edited_page, PS)

    # Make the relocated record self-contained: in-page page pointers (DA = word6,
    # members = word8) that referenced the OLD page now point to the new page (their bytes
    # were copied into it). Cross-page pointers (to other shared pages) are left intact.
    rip = record_bo % PS
    for fld in (6, 8):
        if _u32(buf, new_data_pg * PS + rip + 4 * fld) == data_pgno:
            _put32(buf, new_data_pg * PS + rip + 4 * fld, new_data_pg)

    # COW the B-tree path bottom-up (rewire each child pointer; siblings shared)
    child_old, child_new = data_pgno, new_data_pg
    cow_pages = [new_data_pg]
    for idx_pg, entry_byte_off in reversed(path):
        page = bytearray(buf[idx_pg * PS:(idx_pg + 1) * PS])
        e_in_page = entry_byte_off - idx_pg * PS
        cur = _u32(page, e_in_page + 8)
        if cur != child_old:
            raise ValueError("path inconsistency: entry child %d != expected %d" % (cur, child_old))
        _put32(page, e_in_page + 8, child_new)
        new_pg = _append_page(buf, bytes(page), PS)
        cow_pages.append(new_pg)
        child_old, child_new = idx_pg, new_pg
    new_root = child_new

    ses = _append_session(buf, db, new_root)
    return dict(refno=refno, old_root=old_root, new_root=new_root,
                data_pgno_old=data_pgno, data_pgno_new=new_data_pg,
                path_depth=len(path), cow_pages=cow_pages, **ses)


def cow_commit(buf: bytearray, db: "E3DDb", ss: "SchemaSet", record_bo: int,
               attr_hash: int, new_values) -> dict:
    """Slice 1: COW-commit one fixed-size INLINE (implicit) value edit (real/int/ref,
    same component count) at the main record ``record_bo``."""
    PS = db.page_size
    data_pgno = record_bo // PS
    data_off = (record_bo % PS) // 2
    noun_hash = _u32(buf, record_bo + 0x0C)
    _, td = ss.typedef(noun_hash)
    if td is None or attr_hash not in td:
        raise ValueError("attr 0x%X not in typedef for noun 0x%X" % (attr_hash, noun_hash))
    desc = td[attr_hash]
    sel = (_u32(buf, record_bo + 4 * 10) >> 29) & 1

    page = bytearray(buf[data_pgno * PS:(data_pgno + 1) * PS])
    rng = set_inline_value(page, data_off * 2, desc, sel, new_values)
    if rng[1] > PS:
        raise ValueError("value crosses a page boundary (needs multi-page COW)")
    rep = _commit_edited_data_page(buf, db, record_bo, bytes(page))
    rep.update(noun_hash=noun_hash, sel=sel, value_range_in_page=rng)
    return rep


def _pack_text(typ: int, text: str) -> list:
    """Encode a DA/explicit text value: [byte_length][UTF-8 packed 4/word, MSB-first].
    Returns (ctrl, value_words) mirroring _parse_attr_words (e3d_attr_decoder)."""
    sb = text.encode("utf-8")
    pad = sb + b"\x00" * ((-len(sb)) % 4)
    packed = [struct.unpack_from(">I", pad, 4 * k)[0] for k in range(len(pad) // 4)]
    value = [len(sb)] + packed
    return ((typ << 26) | len(value)), value


def _rewrite_da_text_in_page(page: bytearray, rec_in_page: int, attr_hash: int,
                             new_text: str, PW: int) -> tuple:
    """In-page rebuild of a record's single same-page DA node: replace ``attr_hash``'s
    text value with ``new_text``. Mutates ``page`` (the record's data page) in place;
    updates the DA node header (word0) and the record's rec[10] DA-word-count. Growth
    allowed up to the page / member-region bound. Returns (da_words_old, da_words_new).
    Caller must ensure the record's DA is on THIS page. Raises on unsupported layout."""
    rec = lambda i: struct.unpack_from(">I", page, rec_in_page + 4 * i)[0]
    da_off = (rec(7) >> 13) & 0xFFF
    da_words = (rec(10) >> 14) & 0x3FFF
    mem_words = rec(10) & 0x3FFF
    node_w0 = _u32(page, da_off * 4)
    if ((node_w0 >> 16) & 0xF) != 1:
        raise ValueError("DA node type != 1")
    if _u32(page, (da_off + 3) * 4) != 0:
        raise ValueError("chained multi-node DA not supported")
    plen = (node_w0 & 0xFFFF) - 5
    payload = [_u32(page, (da_off + 5 + k) * 4) for k in range(plen)]

    i, span = 0, None
    while i + 1 < len(payload):
        h, ctrl = payload[i], payload[i + 1]
        n = ctrl & 0x3FFFFFF
        if h == 0 or not (0 < n <= 256) or i + 2 + n > len(payload):
            break
        if h == attr_hash:
            span = (i, i + 2 + n, ctrl >> 26)
            break
        i += 2 + n
    if span is None:
        raise ValueError("attr 0x%X not found in DA region" % attr_hash)
    s, e, typ = span
    if typ not in TEXT_TYPES:
        raise ValueError("DA attr 0x%X is type %d, not text" % (attr_hash, typ))

    new_ctrl, val = _pack_text(typ, new_text)
    new_payload = payload[:s] + [attr_hash, new_ctrl] + val + payload[e:]
    new_da_words = len(new_payload)

    end_word = da_off + 5 + new_da_words
    limit = PW
    if mem_words > 0 and rec(8) == rec(6):              # member region on the same page
        mem_off = (rec(9) >> 13) & 0xFFF
        if mem_off > da_off:
            limit = min(limit, mem_off)
    if end_word > limit:
        raise ValueError("DA needs %d words, only %d before page/member bound (relocation = slice 2b)"
                         % (end_word - da_off, limit - da_off))

    _put32(page, da_off * 4, (new_da_words + 5) | (1 << 16))     # node header word0 (payload+5)
    for k, wv in enumerate(new_payload):
        _put32(page, (da_off + 5 + k) * 4, wv)
    for wd in range(end_word, da_off + 5 + da_words):            # zero freed tail (shrink case)
        if wd < PW:
            _put32(page, wd * 4, 0)
    _put32(page, rec_in_page + 40, (rec(10) & ~(0x3FFF << 14)) | ((new_da_words & 0x3FFF) << 14))
    return da_words, new_da_words


def cow_commit_da_text(buf: bytearray, db: "E3DDb", record_bo: int,
                       attr_hash: int, new_text: str) -> dict:
    """Slice 2: COW-commit a variable-length DA/explicit TEXT edit (e.g. element NAME).
    Same-page single-node DA only; cross-page / chained are rejected (later slice)."""
    PS, PW = db.page_size, db.page_size // 4
    data_pgno = record_bo // PS
    if _u32(buf, record_bo + 24) != data_pgno:          # rec[6] = DA page
        raise ValueError("cross-page DA not supported in slice 2 (da_page=%d rec_page=%d)"
                         % (_u32(buf, record_bo + 24), data_pgno))
    page = bytearray(buf[data_pgno * PS:(data_pgno + 1) * PS])
    da_old, da_new = _rewrite_da_text_in_page(page, record_bo - data_pgno * PS, attr_hash, new_text, PW)
    rep = _commit_edited_data_page(buf, db, record_bo, bytes(page))
    rep.update(attr_hash=attr_hash, da_words_old=da_old, da_words_new=da_new, new_text=new_text)
    return rep


# ─── Slice 6: cross-page / chained DA rewrite via multi-page COW (relocation) ──────────
#   Generalises Slice 2 (same-page, single node, growth-bounded). The DA list is a node
#   CHAIN (db4_get_list): rec[6]=page, off=(rec[7]>>13)&0xFFF, total payload = (rec[10]>>14)
#   &0x3FFF; each node = [w0=(payload+5)|type<<16][w1][w2][w3=next page][w4=(next off)<<13]
#   [payload...]; the reader concatenates payloads across the chain then parses entries.
#   We read the chain-aware payload, edit the text, RE-EMIT it as a fresh node chain on
#   appended page(s), repoint rec[6]/rec[7]/rec[10], and reuse the shared commit core. This
#   uniformly covers: DA already on another page, an already-chained DA, and unbounded growth.

def _list_payload_words(buf: bytearray, record_bo: int, page_size: int, which: int) -> list:
    """Resolve a record's FULL list payload across the node chain (``which``=1 DA/explicit via
    rec[6]/rec[7]/(rec[10]>>14)&0x3FFF ; ``which``=2 members via rec[8]/rec[9]/rec[10]&0x3FFF).
    Mirrors the validated reader ``decode_da_list`` framing; returns the raw / unparsed payload
    (DA = [hash][ctrl][value] entries; members = flat (r0,r1) child-refno pairs)."""
    rec = lambda i: _u32(buf, record_bo + 4 * i)
    if which == 1:
        page, off, words = rec(6), (rec(7) >> 13) & 0xFFF, (rec(10) >> 14) & 0x3FFF
    else:
        page, off, words = rec(8), (rec(9) >> 13) & 0xFFF, rec(10) & 0x3FFF
    if words == 0 or page == 0:
        return []
    payload, remaining, guard = [], words, 0
    while page and remaining > 0 and guard < 128:        # follow node chain across pages
        guard += 1
        node = page * page_size + off * 4
        if node + 20 > len(buf):
            break
        hdr = _u32(buf, node)
        if ((hdr >> 16) & 0xF) != which:
            break
        plen = (hdr & 0xFFFF) - 5
        if plen <= 0:
            break
        take = min(plen, remaining, max(0, (len(buf) - (node + 20)) // 4))
        payload += [_u32(buf, node + 20 + 4 * i) for i in range(take)]
        remaining -= plen
        page, off = _u32(buf, node + 12), (_u32(buf, node + 16) >> 13) & 0xFFF
    return payload


def _da_payload_words(buf: bytearray, record_bo: int, page_size: int) -> list:
    """DA/explicit payload (type-1 node chain). See ``_list_payload_words``."""
    return _list_payload_words(buf, record_bo, page_size, 1)


def read_members(buf: bytearray, record_bo: int, page_size: int) -> list:
    """Return the element's MEMBER (child) refno list ``[(r0,r1), ...]`` (chain-aware, type-2
    node; payload = flat (r0,r1) pairs, member word count = 2 x children). Validated against
    real sam7200: every child's owner refno == this element's refno."""
    p = _list_payload_words(buf, record_bo, page_size, 2)
    return [(p[i], p[i + 1]) for i in range(0, len(p) - 1, 2)]


def _replace_text_in_payload(payload: list, attr_hash: int, new_text: str) -> tuple:
    """Return ``(new_payload, da_words_old, da_words_new)`` with ``attr_hash``'s text value
    replaced. Scans ``[hash][ctrl=(type<<26)|n][value...]`` entries (same framing the reader
    uses), so it works on the flat chain-concatenated payload."""
    i, span = 0, None
    while i + 1 < len(payload):
        h, ctrl = payload[i], payload[i + 1]
        n = ctrl & 0x3FFFFFF
        if h == 0 or not (0 < n <= 256) or i + 2 + n > len(payload):
            break
        if h == attr_hash:
            span = (i, i + 2 + n, ctrl >> 26)
            break
        i += 2 + n
    if span is None:
        raise ValueError("attr 0x%X not found in DA chain" % attr_hash)
    s, e, typ = span
    if typ not in TEXT_TYPES:
        raise ValueError("DA attr 0x%X is type %d, not text" % (attr_hash, typ))
    new_ctrl, val = _pack_text(typ, new_text)
    return payload[:s] + [attr_hash, new_ctrl] + val + payload[e:], len(payload), None


def _emit_node_chain(buf: bytearray, db: "E3DDb", header7: bytes, payload: list,
                     list_type: int, refno: tuple, force_chunk: int = None) -> tuple:
    """Append fresh page(s) holding ``payload`` as a ``list_type`` node chain (1=DA, 2=members).
    Returns ``(first_page, first_off_words, total_payload_words, n_nodes)``. Each fresh page
    reuses a 7-word data-page ``header7`` and the node sits at off=7; node word0=(payload+5)|
    (list_type<<16), word1..2=``refno`` (the element's own refno, as real nodes carry), word3..4
    = chain link (next page / (next off)<<13; last node = 0). One node holds up to ``PW-12``
    payload words; ``force_chunk`` forces smaller nodes (to exercise the cross-page chain).
    NB: a dedicated list page is mechanically valid because the reader reaches DA/members only
    via rec[6]/rec[8]+node-header, never via the page header (full PDMS page-type fidelity for
    fresh pages awaits the real running-E3D round-trip milestone)."""
    PS, PW = db.page_size, db.page_size // 4
    OFFW = 7                                              # node after a 7-word page header
    cap = PW - OFFW - 5                                   # max payload words in one node
    chunk = cap if force_chunk is None else max(1, min(force_chunk, cap))
    groups = [payload[i:i + chunk] for i in range(0, len(payload), chunk)] or [[]]
    pages = []
    for g in groups:
        page = bytearray(PS)
        page[0:28] = header7[0:28]                        # data-page header (cosmetic for list)
        _put32(page, OFFW * 4, (len(g) + 5) | (list_type << 16))   # node w0 = (payload+5)|type<<16
        _put32(page, (OFFW + 1) * 4, refno[0] & 0xFFFFFFFF)        # node w1..2 = element refno
        _put32(page, (OFFW + 2) * 4, refno[1] & 0xFFFFFFFF)
        for k, wv in enumerate(g):
            _put32(page, (OFFW + 5 + k) * 4, wv)
        pages.append(_append_page(buf, bytes(page), PS))
    for i in range(len(pages) - 1):                      # link the chain
        _put32(buf, pages[i] * PS + (OFFW + 3) * 4, pages[i + 1])             # node[3]=next page
        _put32(buf, pages[i] * PS + (OFFW + 4) * 4, (OFFW & 0xFFF) << 13)     # node[4]=next off
    return pages[0], OFFW, len(payload), len(pages)


def _emit_da_chain(buf: bytearray, db: "E3DDb", header7: bytes, payload: list,
                   force_chunk: int = None, refno: tuple = (0, 0)) -> tuple:
    """DA (type-1) node chain — thin wrapper over ``_emit_node_chain``. See it for details."""
    return _emit_node_chain(buf, db, header7, payload, 1, refno, force_chunk)


def cow_commit_da_text_xpage(buf: bytearray, db: "E3DDb", record_bo: int,
                             attr_hash: int, new_text: str, force_chunk: int = None) -> dict:
    """Slice 6: COW-commit a DA/explicit TEXT edit by RELOCATING the whole DA list onto fresh
    page(s) (multi-page COW). Generalises Slice 2 (same-page / single node / growth-bounded):
    handles a DA already on another page, an already-chained DA, and unbounded growth — the DA
    is read chain-aware, the text edited, and the result re-emitted as a fresh node chain; the
    record's rec[6]/rec[7] (DA page/off) + rec[10] DA-count are repointed, then the record page
    + B-tree path are COW'd + a new session appended (shared ``_commit_edited_data_page``)."""
    payload = _da_payload_words(buf, record_bo, db.page_size)
    if not payload:
        raise ValueError("record @%d has no DA list to edit" % record_bo)
    new_payload, da_old, _ = _replace_text_in_payload(payload, attr_hash, new_text)
    rep = _relocate_da_payload(buf, db, record_bo, new_payload, force_chunk)
    rep.update(attr_hash=attr_hash, da_words_old=da_old, new_text=new_text)
    return rep


def _relocate_da_payload(buf: bytearray, db: "E3DDb", record_bo: int, new_payload: list,
                         force_chunk: int = None) -> dict:
    """Shared Slice 6/8 core: re-emit ``new_payload`` as a fresh DA (type-1) node chain on
    appended page(s), repoint the record's rec[6]/rec[7] (DA page/off) + rec[10] DA-word count,
    then COW the record page + B-tree path + a new session. Returns the commit report."""
    PS = db.page_size
    data_pgno = record_bo // PS
    refno = (_u32(buf, record_bo + 4), _u32(buf, record_bo + 8))
    header7 = bytes(buf[data_pgno * PS:data_pgno * PS + 28])      # reuse record page's header
    da_pg, da_off, total, n_nodes = _emit_da_chain(buf, db, header7, new_payload, force_chunk, refno)

    page = bytearray(buf[data_pgno * PS:(data_pgno + 1) * PS])    # edited record page
    rip = record_bo - data_pgno * PS
    rec7 = _u32(page, rip + 7 * 4)
    _put32(page, rip + 6 * 4, da_pg)                                                  # rec[6]=DA page
    _put32(page, rip + 7 * 4, (rec7 & ~(0xFFF << 13)) | ((da_off & 0xFFF) << 13))     # rec[7] off
    rec10 = _u32(page, rip + 10 * 4)
    _put32(page, rip + 10 * 4, (rec10 & ~(0x3FFF << 14)) | ((total & 0x3FFF) << 14))  # rec[10] DA count

    rep = _commit_edited_data_page(buf, db, record_bo, bytes(page))
    rep.update(da_words_new=total, da_page_new=da_pg, da_off_new=da_off, da_nodes=n_nodes)
    return rep


# ─── Slice 8: generic DA-region entry value set/add/remove (motivating use = UDA values) ─────
#   UDA values are ordinary DA-region entries keyed by a UDA hash (> 0x171FAD39 = UDA_THRESHOLD,
#   PDMS_Hash::IsUDA): [hash][ctrl=type<<26|n][value words]. Families: 0x2C00xxxx strongly-typed
#   (type 2=real 2w IEEE double, 4=ref (db,seq), 6=word/struct, 10=text) and 0xFFFxxxx type-7
#   serialized expressions (derived attrs). We edit/add/remove any DA entry by hash, re-using the
#   Slice 6 DA-region relocation core (so it transparently handles cross-page / growth).

def read_uda(buf: bytearray, record_bo: int, page_size: int) -> list:
    """Return the element's UDA entries ``[{hash,type,value(words)}]`` from the DA region
    (hash > UDA_THRESHOLD). Value = raw words (text not decoded; same framing the reader uses)."""
    payload = _da_payload_words(buf, record_bo, page_size)
    out, i = [], 0
    while i + 1 < len(payload):
        h, ctrl = payload[i], payload[i + 1]
        n = ctrl & 0x3FFFFFF
        if h == 0 or not (0 < n <= 256) or i + 2 + n > len(payload):
            break
        if h > UDA_THRESHOLD:
            out.append(dict(hash=h, type=ctrl >> 26, value=payload[i + 2:i + 2 + n]))
        i += 2 + n
    return out


def _set_entry_in_payload(payload: list, attr_hash: int, new_ctrl: int, new_value: list) -> list:
    """Replace (or, if absent, append at the end of the valid entry run) the
    ``[hash][ctrl][value]`` entry for ``attr_hash``. Other entries are preserved verbatim."""
    i, span = 0, None
    while i + 1 < len(payload):
        h, ctrl = payload[i], payload[i + 1]
        n = ctrl & 0x3FFFFFF
        if h == 0 or not (0 < n <= 256) or i + 2 + n > len(payload):
            break
        if h == attr_hash:
            span = (i, i + 2 + n)
            break
        i += 2 + n
    entry = [attr_hash & 0xFFFFFFFF, new_ctrl & 0xFFFFFFFF] + [w & 0xFFFFFFFF for w in new_value]
    if span is None:
        return payload[:i] + entry + payload[i:]
    return payload[:span[0]] + entry + payload[span[1]:]


def _remove_entry_in_payload(payload: list, attr_hash: int) -> list:
    """Remove the ``[hash][ctrl][value]`` entry for ``attr_hash`` (raise if absent)."""
    i = 0
    while i + 1 < len(payload):
        h, ctrl = payload[i], payload[i + 1]
        n = ctrl & 0x3FFFFFF
        if h == 0 or not (0 < n <= 256) or i + 2 + n > len(payload):
            break
        if h == attr_hash:
            return payload[:i] + payload[i + 2 + n:]
        i += 2 + n
    raise ValueError("entry 0x%X not found in DA region" % attr_hash)


def cow_da_set_entry(buf: bytearray, db: "E3DDb", record_bo: int, attr_hash: int,
                     type_code: int, value_words: list, force_chunk: int = None) -> dict:
    """Slice 8: set (or add) a DA-region entry's value via DA relocation (reuses Slice 6 core).
    Generic over any DA attr; the motivating use is UDA strongly-typed values. ``value_words`` =
    raw value words (ref=[db,seq]; word/enum=[w]; real=2-word IEEE double low-word-first; text
    = ``_pack_text`` words). ``ctrl`` = ``(type_code<<26)|len``. Handles cross-page / growth."""
    payload = _da_payload_words(buf, record_bo, db.page_size)
    if not payload:
        raise ValueError("record @%d has no DA region" % record_bo)
    new_ctrl = (type_code << 26) | (len(value_words) & 0x3FFFFFF)
    new_payload = _set_entry_in_payload(payload, attr_hash, new_ctrl, value_words)
    rep = _relocate_da_payload(buf, db, record_bo, new_payload, force_chunk)
    rep.update(attr_hash=attr_hash, type_code=type_code, n_words=len(value_words),
               is_uda=attr_hash > UDA_THRESHOLD)
    return rep


def cow_da_remove_entry(buf: bytearray, db: "E3DDb", record_bo: int, attr_hash: int,
                        force_chunk: int = None) -> dict:
    """Slice 8: remove a DA-region entry (e.g. a UDA) by hash, via DA relocation."""
    payload = _da_payload_words(buf, record_bo, db.page_size)
    new_payload = _remove_entry_in_payload(payload, attr_hash)
    rep = _relocate_da_payload(buf, db, record_bo, new_payload, force_chunk)
    rep.update(attr_hash=attr_hash, removed=True, is_uda=attr_hash > UDA_THRESHOLD)
    return rep


# ─── Slice 7: member-list (child refno) rewrite via multi-page COW (type-2, reuses S6 core) ──
#   Members are a type-2 node chain (parallel to DA's type-1): rec[8]=page, off=(rec[9]>>13)&
#   0xFFF, member words = rec[10]&0x3FFF; node w0=(payload+5)|(2<<16), w1..2=element refno,
#   w3..4=chain link; payload = flat (r0,r1) CHILD-REFNO pairs (member words = 2 x #children).
#   Probed on real sam7200: every listed child's owner refno == this element's refno.

def cow_members_set(buf: bytearray, db: "E3DDb", record_bo: int, children: list,
                    force_chunk: int = None) -> dict:
    """Slice 7: set the element's MEMBER (child refno) list to ``children`` (a list of
    ``(r0, r1)`` tuples) by RE-EMITTING the type-2 node chain onto fresh page(s) (multi-page
    COW). Covers relocate-verbatim, add-child and remove-child uniformly. Repoints rec[8]/rec[9]
    (member page/off) + rec[10] member-word bits, then COW's the record page + B-tree path + a
    new session (shared ``_commit_edited_data_page``). An empty list => page 0 / 0 words."""
    PS = db.page_size
    data_pgno = record_bo // PS
    refno = (_u32(buf, record_bo + 4), _u32(buf, record_bo + 8))
    payload = []
    for (a, b) in children:
        payload += [a & 0xFFFFFFFF, b & 0xFFFFFFFF]

    if payload:
        header7 = bytes(buf[data_pgno * PS:data_pgno * PS + 28])
        mem_pg, mem_off, total, n_nodes = _emit_node_chain(buf, db, header7, payload, 2, refno, force_chunk)
    else:
        mem_pg, mem_off, total, n_nodes = 0, 0, 0, 0     # no members -> page 0 / 0 words

    page = bytearray(buf[data_pgno * PS:(data_pgno + 1) * PS])    # edited record page
    rip = record_bo - data_pgno * PS
    mem_old = _u32(page, rip + 10 * 4) & 0x3FFF
    rec9 = _u32(page, rip + 9 * 4)
    _put32(page, rip + 8 * 4, mem_pg)                                                 # rec[8]=member page
    _put32(page, rip + 9 * 4, (rec9 & ~(0xFFF << 13)) | ((mem_off & 0xFFF) << 13))    # rec[9] off
    rec10 = _u32(page, rip + 10 * 4)
    _put32(page, rip + 10 * 4, (rec10 & ~0x3FFF) | (total & 0x3FFF))                  # rec[10] member words (low 14)

    rep = _commit_edited_data_page(buf, db, record_bo, bytes(page))
    rep.update(member_words_old=mem_old, member_words_new=total, n_children=len(children),
               member_page_new=mem_pg, member_off_new=mem_off, member_nodes=n_nodes)
    return rep


def _rightmost_path(db: "E3DDb", root: int) -> tuple:
    """Navigate to the rightmost (max-key) leaf. Returns (leaf_pgno, ancestors), where
    ancestors = [(index_pgno, rightmost_internal_entry_word_index), ...] from root down to
    the leaf's parent. Entries are word6-bounded (free-words header, not null-terminated)."""
    PW = db.page_size // 4
    u = db.u32

    def is_index(pg):
        return 0 < pg < db.n_pages and u(db.page_off(pg) + 4) == INDEX_NOUN

    pg, anc = root, []
    for _ in range(40):
        base = db.page_off(pg)
        nent = (PW - 7 - u(base + 24)) // 4              # word6 = free words
        rightmost = None
        for k in range(nent):
            wo = 7 + 4 * k
            cpg, v = u(base + wo * 4 + 8), u(base + wo * 4 + 12)
            if (v >> 12) == 0 and is_index(cpg):
                rightmost = (wo, cpg)
        if rightmost is None:
            return pg, anc                               # this is the leaf
        anc.append((pg, rightmost[0]))
        pg = rightmost[1]
    raise ValueError("B-tree too deep / cycle")


def _clone_element_page(buf: bytearray, db: "E3DDb", src_record_bo: int,
                        new_refno: tuple, new_name: str) -> tuple:
    """Append a COW clone of the source element's data page, re-stamped with ``new_refno``
    and ``new_name`` (DA NAME rewrite), and made self-contained (in-page DA/member page
    pointers repointed to the new page). Returns ``(new_data_pg, src_off_words, da_old,
    da_new)``. Source DA must live on the source's own page (same-page only)."""
    PS, PW = db.page_size, db.page_size // 4
    src_pg = src_record_bo // PS
    rec_in_page = src_record_bo % PS
    if _u32(buf, src_record_bo + 24) != src_pg:
        raise ValueError("source element DA not on its own page (clone unsupported)")
    page = bytearray(buf[src_pg * PS:(src_pg + 1) * PS])
    _put32(page, rec_in_page + 4, new_refno[0])
    _put32(page, rec_in_page + 8, new_refno[1])
    da_old, da_new = _rewrite_da_text_in_page(page, rec_in_page, NAME_HASH, new_name, PW)
    new_data_pg = _append_page(buf, bytes(page), PS)
    for fld in (6, 8):                                   # self-contained in-page page pointers
        if _u32(buf, new_data_pg * PS + rec_in_page + 4 * fld) == src_pg:
            _put32(buf, new_data_pg * PS + rec_in_page + 4 * fld, new_data_pg)
    return new_data_pg, rec_in_page // 2, da_old, da_new


def cow_insert_element(buf: bytearray, db: "E3DDb", src_record_bo: int,
                       new_refno: tuple, new_name: str) -> dict:
    """Slice 3: create a NEW element by CLONING the source element (new refno + NAME),
    appended at the MAX key into the rightmost B-tree leaf via COW. No node split (requires
    room in the rightmost leaf); same-page-DA source only. Mutates ``buf``; returns report.
    (Slice 5 ``cow_insert_element_split`` generalises this to any key + node split.)"""
    PS, PW = db.page_size, db.page_size // 4
    new_data_pg, src_off_words, da_old, da_new = _clone_element_page(
        buf, db, src_record_bo, new_refno, new_name)

    # 2. append the leaf entry at the rightmost leaf (max key -> no separator key changes)
    old_root = db.session_chain()[0]["index_root_pgno"]
    leaf_pg, anc = _rightmost_path(db, old_root)
    leaf = bytearray(buf[leaf_pg * PS:(leaf_pg + 1) * PS])
    w6 = _u32(leaf, 24)
    if w6 < 4:
        raise ValueError("rightmost leaf full -> node split needed (slice 3b)")
    nent = (PW - 7 - w6) // 4
    last = ((_u32(leaf, (7 + 4 * (nent - 1)) * 4), _u32(leaf, (7 + 4 * (nent - 1)) * 4 + 4))
            if nent else (0, 0))
    if new_refno <= last:
        raise ValueError("new refno %r not > current max %r" % (new_refno, last))
    eo = (7 + 4 * nent) * 4
    _put32(leaf, eo, new_refno[0])
    _put32(leaf, eo + 4, new_refno[1])
    _put32(leaf, eo + 8, new_data_pg)
    _put32(leaf, eo + 12, (src_off_words << 12) | 1)     # off|flag (leaf data flag = 1)
    _put32(leaf, 24, w6 - 4)                              # word6 -= one entry (4 words)
    new_leaf_pg = _append_page(buf, bytes(leaf), PS)

    # 3. COW the rightmost ancestor chain (rewire child pointer; keys unchanged)
    child_old, child_new = leaf_pg, new_leaf_pg
    cow_pages = [new_data_pg, new_leaf_pg]
    for idx_pg, wo in reversed(anc):
        pageA = bytearray(buf[idx_pg * PS:(idx_pg + 1) * PS])
        e = wo * 4
        if _u32(pageA, e + 8) != child_old:
            raise ValueError("path inconsistency at pg %d" % idx_pg)
        _put32(pageA, e + 8, child_new)
        np = _append_page(buf, bytes(pageA), PS)
        cow_pages.append(np)
        child_old, child_new = idx_pg, np
    new_root = child_new

    ses = _append_session(buf, db, new_root)
    return dict(new_refno=new_refno, new_name=new_name, data_pgno_new=new_data_pg,
                leaf_pg=leaf_pg, new_leaf_pg=new_leaf_pg, leaf_free_before=w6, leaf_entries=nent,
                old_root=old_root, new_root=new_root, cow_pages=cow_pages,
                src_off_words=src_off_words, da_words_old=da_old, da_words_new=da_new, **ses)


def cow_delete_element(buf: bytearray, db: "E3DDb", ss: "SchemaSet", refno: tuple) -> dict:
    """Slice 4: delete an element by removing its MAIN-record leaf entry from the B-tree
    (compact the leaf + word6 += 4), COW the path to root, new session. The element's data
    page is intentionally left in place (old sessions still resolve it = multi-version).
    No node merge on underflow (PDMS tolerates underfull nodes). Mutates ``buf``."""
    PS, PW = db.page_size, db.page_size // 4
    old_root = db.session_chain()[0]["index_root_pgno"]
    found = find_leaf_path(db, old_root,
                           lambda r0, r1, cpg, off: (r0, r1) == refno and is_main_record(db, cpg, off))
    if not found:
        raise ValueError("main record for refno %r not found under root %d" % (refno, old_root))
    path, _, _ = found
    leaf_pg, entry_byte_off = path[-1]
    anc = path[:-1]

    leaf = bytearray(buf[leaf_pg * PS:(leaf_pg + 1) * PS])
    w6 = _u32(leaf, 24)
    nent = (PW - 7 - w6) // 4
    entry_idx = ((entry_byte_off - leaf_pg * PS) // 4 - 7) // 4
    if not (0 <= entry_idx < nent):
        raise ValueError("entry index %d outside word6 bound (%d entries)" % (entry_idx, nent))
    for k in range(entry_idx, nent - 1):                 # compact: shift later entries left one slot
        d, s = (7 + 4 * k) * 4, (7 + 4 * (k + 1)) * 4
        leaf[d:d + 16] = leaf[s:s + 16]
    last = (7 + 4 * (nent - 1)) * 4
    leaf[last:last + 16] = b"\x00" * 16                  # clear the vacated last slot
    _put32(leaf, 24, w6 + 4)                             # word6 += one entry (4 words)
    new_leaf_pg = _append_page(buf, bytes(leaf), PS)

    child_old, child_new = leaf_pg, new_leaf_pg
    cow_pages = [new_leaf_pg]
    for idx_pg, eoff in reversed(anc):
        pageA = bytearray(buf[idx_pg * PS:(idx_pg + 1) * PS])
        e = eoff - idx_pg * PS
        if _u32(pageA, e + 8) != child_old:
            raise ValueError("path inconsistency at pg %d" % idx_pg)
        _put32(pageA, e + 8, child_new)
        np = _append_page(buf, bytes(pageA), PS)
        cow_pages.append(np)
        child_old, child_new = idx_pg, np
    new_root = child_new

    ses = _append_session(buf, db, new_root)
    return dict(refno=refno, removed_entry_idx=entry_idx, leaf_pg=leaf_pg, new_leaf_pg=new_leaf_pg,
                leaf_free_before=w6, leaf_free_after=w6 + 4,
                old_root=old_root, new_root=new_root, cow_pages=cow_pages, **ses)


# ─── Slice 5: arbitrary-key B-tree insert with node split ────────────────────────────
#   db3_change_table_entry (3.3.1, sub_1061DEA0): recursive insert; on overflow split the
#   node and promote the new sibling's first (min) key as the separator into the parent.
#   db3_split_node (3.2.6, sub_1061BA50): split at the entry boundary nearest the word
#   midpoint (PW-word6-7)/2+7; both halves keep level=word2.
#   db3_split_root (3.2.7, sub_1061C340): on ROOT overflow allocate a new root, ++word2
#   (level), entry0 = sentinel 0x80000001 -> old root (lower), entry1 = sep -> new sibling.
#   This is a B+-tree: data lives only in leaves (off!=0); internal nodes hold separator
#   keys (= min key of each child subtree) + child pointers (off==0). The sentinel (−∞)
#   is the leftmost separator of every node on the LEFTMOST spine only.

def _idx_entries(db: "E3DDb", buf: bytearray, pg: int) -> tuple:
    """word6-bounded entries of index page ``pg`` read from ``buf`` (which may include
    freshly-appended pages): ([[r0,r1,cpg,v], ...], level=word2). See findings §16."""
    PS, PW = db.page_size, db.page_size // 4
    base = pg * PS
    level = _u32(buf, base + 8)                       # word2 = B-tree level (0 = leaf)
    nent = (PW - 7 - _u32(buf, base + 24)) // 4       # word6 = free words
    out = []
    for k in range(nent):
        wo = base + (7 + 4 * k) * 4
        out.append([_u32(buf, wo), _u32(buf, wo + 4), _u32(buf, wo + 8), _u32(buf, wo + 12)])
    return out, level


def _emit_index_page(buf: bytearray, db: "E3DDb", template_pg: int, level: int, entries: list) -> int:
    """Append a new index page (header cloned from ``template_pg``: type5/INDEX_NOUN/word3/
    word4/word5; word2:=level; word6:=PW-7-4*len; entries from word7, tail zeroed). Returns
    the new page no."""
    PS, PW = db.page_size, db.page_size // 4
    if 4 * len(entries) > PW - 7:
        raise ValueError("index page overflow: %d entries > capacity %d" % (len(entries), (PW - 7) // 4))
    page = bytearray(buf[template_pg * PS:(template_pg + 1) * PS])
    _put32(page, 8, level)
    _put32(page, 24, PW - 7 - 4 * len(entries))
    for w in range(7, PW):
        _put32(page, w * 4, 0)
    for i, e in enumerate(entries):
        eo = (7 + 4 * i) * 4
        _put32(page, eo, e[0]); _put32(page, eo + 4, e[1])
        _put32(page, eo + 8, e[2]); _put32(page, eo + 12, e[3])
    return _append_page(buf, bytes(page), PS)


def _key_le(a: tuple, b: tuple) -> bool:
    """``a <= b`` with the 0x80000001 sentinel treated as −∞ (leftmost separator)."""
    if a == (SENTINEL, SENTINEL):
        return True
    if b == (SENTINEL, SENTINEL):
        return False
    return a <= b


def _btree_insert(buf: bytearray, db: "E3DDb", pg: int, key: tuple, leaf_data: list, cap: int) -> tuple:
    """Insert ``key -> leaf_data=[data_pg, v]`` into the B+-subtree at index page ``pg`` (COW,
    level-driven via word2). ``cap`` = max entries per node before splitting. Returns
    ``(new_pg, split)`` where ``split`` is ``None`` or ``(sep_key, new_sibling_pg)`` for the
    parent to absorb. Mirrors db3_change_table_entry + db3_split_node."""
    entries, level = _idx_entries(db, buf, pg)
    if level == 0:                                       # leaf: sorted insert (reject duplicate)
        pos = len(entries)
        for i, e in enumerate(entries):
            if (e[0], e[1]) == key:
                raise ValueError("duplicate key %r already present" % (key,))
            if not _key_le((e[0], e[1]), key):
                pos = i
                break
        ne = entries[:pos] + [[key[0], key[1], leaf_data[0], leaf_data[1]]] + entries[pos:]
    else:                                                # internal: descend, rewire child, absorb split
        ci = 0
        for i, e in enumerate(entries):
            if _key_le((e[0], e[1]), key):
                ci = i
            else:
                break
        new_child, csplit = _btree_insert(buf, db, entries[ci][2], key, leaf_data, cap)
        ne = [list(e) for e in entries]
        ne[ci][2] = new_child                            # rewire to COW'd child (siblings shared)
        if csplit:
            (s0, s1), sib = csplit
            ne.insert(ci + 1, [s0, s1, sib, 1])          # separator -> new sibling, after ci
    if len(ne) <= cap:
        return _emit_index_page(buf, db, pg, level, ne), None
    # overflow -> split ~ in half at an entry boundary; separator = min key of upper subtree
    L = (len(ne) + 1) // 2
    lower, upper = ne[:L], ne[L:]
    sep = (upper[0][0], upper[0][1])
    return (_emit_index_page(buf, db, pg, level, lower),
            (sep, _emit_index_page(buf, db, pg, level, upper)))


def cow_insert_leaf(buf: bytearray, db: "E3DDb", root: int, key: tuple,
                    data_pg: int, off: int, cap: int = None) -> tuple:
    """Insert one leaf entry (refno ``key`` -> element at ``data_pg``/``off`` words) into the
    B+-tree rooted at ``root`` (COW), splitting nodes recursively and growing a new root on
    root overflow. Returns ``(new_root, grew)`` (``grew`` = the tree gained a level). Does NOT
    append a session, so callers may chain several inserts onto the returned root before
    committing once."""
    PW = db.page_size // 4
    cap = cap if cap is not None else (PW - 7) // 4
    _, root_level = _idx_entries(db, buf, root)
    new_root, split = _btree_insert(buf, db, root, key, [data_pg, (off << 12) | 1], cap)
    if not split:
        return new_root, False
    (s0, s1), sib = split                                # root overflowed -> new root
    nr = _emit_index_page(buf, db, root, root_level + 1,
                          [[SENTINEL, SENTINEL, new_root, 1], [s0, s1, sib, 1]])
    return nr, True


def cow_insert_element_split(buf: bytearray, db: "E3DDb", src_record_bo: int,
                            new_refno: tuple, new_name: str, cap: int = None) -> dict:
    """Slice 5: create a NEW element (clone of ``src_record_bo`` with ``new_refno`` + ``new_name``)
    and insert it at its SORTED key position via the general B+-tree insert (leaf split +
    recursive parent split + new root as needed), then commit a new session. Generalises
    Slice 3 (which only appends at the max key into a non-full rightmost leaf)."""
    new_data_pg, src_off, da_old, da_new = _clone_element_page(buf, db, src_record_bo, new_refno, new_name)
    old_root = db.session_chain()[0]["index_root_pgno"]
    new_root, grew = cow_insert_leaf(buf, db, old_root, new_refno, new_data_pg, src_off, cap)
    ses = _append_session(buf, db, new_root)
    return dict(new_refno=new_refno, new_name=new_name, data_pgno_new=new_data_pg,
                src_off_words=src_off, old_root=old_root, new_root=new_root,
                tree_grew=grew, da_words_old=da_old, da_words_new=da_new, **ses)


def _btree_descend(db: "E3DDb", buf: bytearray, root: int, key: tuple) -> bool:
    """PDMS binary-search descent: at each internal node pick the rightmost entry whose
    separator <= ``key`` (sentinel = −∞), exactly as db3 navigates. Returns True iff the
    reached leaf contains ``key``."""
    pg = root
    for _ in range(40):
        ents, level = _idx_entries(db, buf, pg)
        if level == 0:
            return any((e[0], e[1]) == key for e in ents)
        ci = 0
        for i, e in enumerate(ents):
            if _key_le((e[0], e[1]), key):
                ci = i
            else:
                break
        pg = ents[ci][2]
    return False


def _btree_check(db: "E3DDb", buf: bytearray, root: int) -> dict:
    """Validate B+-tree invariants by walking ``buf`` from ``root``. Returns a report:
    count(leaf entries), dups(adjacent equal keys), sorted_ok(non-decreasing), leaf_pages,
    index_pages, height, balanced(all leaves at one depth), keyset, and **nav_ok** -- every
    key reachable by PDMS binary-search descent. nav_ok (not "separator == child min") is the
    real correctness property: separators may be *loose* (below their child's min) after
    historical deletes, e.g. sam7200 pg3264 entry39 sep=0x3D08 < child-min 0x3D75 (PDMS, like
    cow_delete, does not tighten separators on delete) -- still a valid divider."""
    rep = {"count": 0, "dups": 0, "leaf_pages": 0, "index_pages": 0}
    heights, prev, sorted_ok, keyset = set(), [None], [True], set()

    def rec(pg, depth):
        ents, level = _idx_entries(db, buf, pg)
        rep["index_pages"] += 1
        if level == 0:
            rep["leaf_pages"] += 1
            heights.add(depth)
            for e in ents:
                k = (e[0], e[1])
                rep["count"] += 1
                keyset.add(k)
                if prev[0] is not None:
                    if k < prev[0]:
                        sorted_ok[0] = False
                    elif k == prev[0]:
                        rep["dups"] += 1
                prev[0] = k
            return
        for e in ents:
            rec(e[2], depth + 1)

    rec(root, 0)
    rep["nav_ok"] = all(_btree_descend(db, buf, root, k) for k in keyset)
    rep["sorted_ok"] = sorted_ok[0]
    rep["balanced"] = len(heights) == 1
    rep["height"] = max(heights) if heights else 0
    rep["keyset"] = keyset
    return rep


def read_attr_via_root(db: "E3DDb", ss: "SchemaSet", root: int, refno: tuple, attr_hash: int):
    """Resolve ``refno`` to its MAIN record through a SPECIFIC session's index root and
    return ``(value, byte_off, data_pgno)`` for the named attribute (or (None,None,None))."""
    found = find_leaf_path(db, root, lambda r0, r1, cpg, off: (r0, r1) == refno and is_main_record(db, cpg, off))
    if not found:
        return None, None, None
    _, data_pgno, data_off = found
    bo = data_pgno * db.page_size + data_off * 2
    r = decode_full_element(ss, db.blob, bo)
    val = next((a["value"] for a in r["attrs"] if a["hash"] == attr_hash), None)
    return val, bo, data_pgno


def commit_file(dst: str, ss: "SchemaSet", name: str, attr_hash: int, new_values) -> tuple:
    """Open ``dst``, COW-commit one inline edit to the named element's latest version,
    write it back. Returns ``(refno, report)``."""
    db = E3DDb(dst)
    bo = find_element(db, ss, name)
    if bo is None:
        raise ValueError("element %r not found" % name)
    refno = (db.u32(bo + 4), db.u32(bo + 8))
    buf = bytearray(db.blob)
    rep = cow_commit(buf, db, ss, bo, attr_hash, new_values)
    with open(dst, "wb") as f:
        f.write(buf)
    return refno, rep


def commit_file_da(dst: str, ss: "SchemaSet", name: str, attr_hash: int, new_text: str) -> tuple:
    """Open ``dst``, COW-commit a DA/explicit text edit (e.g. NAME) to the named element's
    latest version, write it back. Returns ``(refno, report)``."""
    db = E3DDb(dst)
    bo = find_element(db, ss, name)
    if bo is None:
        raise ValueError("element %r not found" % name)
    refno = (db.u32(bo + 4), db.u32(bo + 8))
    buf = bytearray(db.blob)
    rep = cow_commit_da_text(buf, db, bo, attr_hash, new_text)
    with open(dst, "wb") as f:
        f.write(buf)
    return refno, rep


def read_name_via_root(db: "E3DDb", ss: "SchemaSet", root: int, refno: tuple):
    """Resolve refno's main record through a session root; return (element_name, POS)."""
    pos, bo, _ = read_attr_via_root(db, ss, root, refno, POS_HASH)
    nm = decode_full_element(ss, db.blob, bo)["element_name"] if bo is not None else None
    return nm, pos


def commit_file_insert(dst: str, ss: "SchemaSet", src_name: str, new_name: str) -> tuple:
    """Open ``dst``, COW-insert a NEW element cloned from ``src_name`` (fresh refno = current
    max + 1, given NAME), write it back. Returns ``(new_refno, report)``."""
    db = E3DDb(dst)
    bo = find_element(db, ss, src_name)
    if bo is None:
        raise ValueError("source element %r not found" % src_name)
    mx = max(db.walk_index(db.session_chain()[0]["index_root_pgno"]), key=lambda e: (e[0], e[1]))
    new_refno = (mx[0], mx[1] + 1)
    buf = bytearray(db.blob)
    rep = cow_insert_element(buf, db, bo, new_refno, new_name)
    with open(dst, "wb") as f:
        f.write(buf)
    return new_refno, rep


def commit_file_delete(dst: str, ss: "SchemaSet", refno: tuple) -> dict:
    """Open ``dst``, COW-delete the element with ``refno`` (remove its main leaf entry),
    write it back. Returns the report."""
    db = E3DDb(dst)
    buf = bytearray(db.blob)
    rep = cow_delete_element(buf, db, ss, refno)
    with open(dst, "wb") as f:
        f.write(buf)
    return rep


def _walk_leaf_locs(db: "E3DDb", buf: bytearray, root: int) -> list:
    """Walk the B-tree under ``root`` (reading ``buf``); return [(refno, data_pgno, data_off), ...]
    for every leaf entry (sentinel / off==0 skipped). data_off is the decoded (>>12) word offset."""
    out = []

    def rec(pg):
        ents, level = _idx_entries(db, buf, pg)
        if level == 0:
            for e in ents:
                if (e[0], e[1]) == (SENTINEL, SENTINEL):
                    continue
                off = e[3] >> 12
                if off == 0:
                    continue
                out.append(((e[0], e[1]), e[2], off))
        else:
            for e in ents:
                rec(e[2])

    rec(root)
    return out


def verify_commit(orig_bytes: bytes, dst: str, ss: "SchemaSet", expects: list) -> list:
    """Post-commit self-check (Phase 7 / FR-019, T038 — mirrors Rust ``verify_commit``):
      (1) B-tree invariants (nav_ok / balanced / sorted / no-dup) under the latest session;
      (2) COW immutability — original bytes unchanged bar page0's session pointer (0x28) — plus
          append-only growth;
      (3) each ``expect`` = ``(refno, attr_hash, want)`` reads back (``want=None`` => expect absent);
      (4) owner-reference integrity — no in-db owner dangles.
    Returns a list of issue tuples (empty list = a sound commit)."""
    db = E3DDb(dst)
    buf = bytearray(db.blob)
    PS = db.page_size
    issues = []
    # (2) immutability + append-only
    if len(buf) < len(orig_bytes):
        issues.append(("NotAppended",))
    n = min(len(orig_bytes), len(buf))
    mutated = [i for i in range(n) if orig_bytes[i] != buf[i] and not (HDR_LATEST <= i < HDR_LATEST + 4)]
    if mutated:
        issues.append(("OriginalMutated", mutated[:64]))
    # (1) B-tree invariants
    root = db.session_chain()[0]["index_root_pgno"]
    rep = _btree_check(db, buf, root)
    for k in ("nav_ok", "balanced", "sorted_ok"):
        if not rep[k]:
            issues.append(("BtreeBroken", k))
    if rep["dups"] != 0:
        issues.append(("BtreeBroken", "dups"))
    # (3) read-back expectations
    for (refno, attr_hash, want) in expects:
        val, bo, _ = read_attr_via_root(db, ss, root, refno, attr_hash)
        if want is None:
            if bo is not None:
                issues.append(("ReadbackMismatch", refno, "expected absent"))
        elif bo is None:
            issues.append(("ReadbackMismatch", refno, "expected present"))
        elif list(val or []) != list(want):
            issues.append(("ReadbackMismatch", refno, "%s != %s" % (val, want)))
    # (4) owner-reference integrity
    locs = _walk_leaf_locs(db, buf, root)
    keyset = {r for (r, _, _) in locs}
    dbnos = {r[0] for r in keyset}
    seen = set()
    for (refno, cpg, off) in locs:
        if refno in seen or not is_main_record(db, cpg, off):
            continue
        seen.add(refno)
        bo = cpg * PS + off * 2
        if bo + 24 > len(buf):
            continue
        owner = (_u32(buf, bo + 16), _u32(buf, bo + 20))
        if owner == (0, 0) or owner[0] == SENTINEL:
            continue
        if owner[0] in dbnos and owner not in keyset:
            issues.append(("DanglingRef", refno, owner))
    return issues


def batch_commit(dst: str, ss: "SchemaSet", edits: list) -> int:
    """Apply several edits as ONE new session (``sesno`` only +1) — mirrors Rust
    ``EdbWriter::batch``. Each callable in ``edits`` (``f(dst)``) commits one edit normally
    (appending an intermediate session); afterwards they are collapsed into a single canonical
    session (cloned from the pre-batch session, ``sesno = base+1``, root = final root, linked to
    the pre-batch session). Intermediate sessions become unreferenced orphans (off the page0
    chain), so previous sessions and the original byte range stay intact. Returns the new sesno."""
    db0 = E3DDb(dst)
    PS = db0.page_size
    base_ses = db0.latest_ses_pgno
    base_sesno = _u32(bytearray(db0.blob), base_ses * PS + SES_SESNO)
    for f in edits:
        f(dst)  # each commits + writes dst (intermediate session)
    db = E3DDb(dst)
    final_root = db.session_chain()[0]["index_root_pgno"]
    buf = bytearray(db.blob)
    ses = bytearray(buf[base_ses * PS:(base_ses + 1) * PS])
    _put32(ses, SES_SESNO, base_sesno + 1)
    _put32(ses, SES_ROOT, final_root)
    _put32(ses, SES_LAST, base_ses)
    new_pg = _append_page(buf, bytes(ses), PS)
    _put32(buf, new_pg * PS + SES_END, new_pg)
    _put32(buf, HDR_LATEST, new_pg)
    with open(dst, "wb") as fh:
        fh.write(buf)
    return base_sesno + 1


def _demo(src, exe, name):
    dst = "_e3d_write_full_demo.bin"
    shutil.copyfile(src, dst)                       # NEVER touch the original
    try:
        ss = SchemaSet(exe)
        db = E3DDb(dst)
        bo = find_element(db, ss, name)
        if bo is None:
            print("element %r not found in %s" % (name, src))
            return 0
        refno = (db.u32(bo + 4), db.u32(bo + 8))
        orig_pos, _, orig_dpg = read_attr_via_root(db, ss, db.session_chain()[0]["index_root_pgno"], refno, POS_HASH)
        print("target %s  refno=%r  record_pg=%d  POS = %s" % (name, refno, bo // db.page_size, orig_pos))
        if not orig_pos or len(orig_pos) != 3:
            print("element has no 3-component POS; pick another --name")
            return 0
        orig_bytes = db.blob
        orig_len = len(orig_bytes)
        n_sess0 = len(db.session_chain())

        v1 = (1000.25, -2000.5, 3000.75)
        v2 = (11.0, 22.0, 33.0)

        # ---- commit #1 (detailed report) -----------------------------------
        _, rep1 = commit_file(dst, ss, name, POS_HASH, v1)
        print("\n-- commit #1 (POS -> %s) --" % (v1,))
        print("  sesno %d -> %d ;  index_root %d -> %d ;  session pg %d -> %d"
              % (rep1["old_sesno"], rep1["new_sesno"], rep1["old_root"], rep1["new_root"],
                 rep1["old_ses_pg"], rep1["new_ses_pg"]))
        print("  COW path depth=%d  pages appended=%s  (data %d -> %d)"
              % (rep1["path_depth"], rep1["cow_pages"], rep1["data_pgno_old"], rep1["data_pgno_new"]))

        # ---- commit #2 stacked on top (proves multi-version history) --------
        _, rep2 = commit_file(dst, ss, name, POS_HASH, v2)
        print("-- commit #2 (POS -> %s)  stacked --" % (v2,))
        print("  sesno %d -> %d ;  index_root %d -> %d ;  data %d -> %d"
              % (rep2["old_sesno"], rep2["new_sesno"], rep2["old_root"], rep2["new_root"],
                 rep2["data_pgno_old"], rep2["data_pgno_new"]))

        # ---- verify the full version history --------------------------------
        db2 = E3DDb(dst)
        chain = db2.session_chain()
        p2, bo2, _ = read_attr_via_root(db2, ss, chain[0]["index_root_pgno"], refno, POS_HASH)  # newest
        p1, _, _ = read_attr_via_root(db2, ss, chain[1]["index_root_pgno"], refno, POS_HASH)
        p0, _, _ = read_attr_via_root(db2, ss, chain[2]["index_root_pgno"], refno, POS_HASH)    # original
        name2 = decode_full_element(ss, db2.blob, bo2)["element_name"] if bo2 else None

        edit = open(dst, "rb").read()
        diff = [i for i in range(min(orig_len, len(edit))) if orig_bytes[i] != edit[i]]
        diff_confined = len(diff) >= 1 and all(HDR_LATEST <= d < HDR_LATEST + 4 for d in diff)
        appended = len(edit) - orig_len

        print("\n-- read-back across the session history --")
        print("  sesno=%-3d (newest) %s POS = %s   name=%s" % (chain[0]["sesno"], name, p2, name2))
        print("  sesno=%-3d           %s POS = %s" % (chain[1]["sesno"], name, p1))
        print("  sesno=%-3d (oldest) %s POS = %s" % (chain[2]["sesno"], name, p0))
        print("\n-- integrity --")
        print("  changed bytes in original range = %d at %s  (confined to page0 session ptr = %s)"
              % (len(diff), [hex(d) for d in diff], diff_confined))
        print("  pages appended = %d ;  session chain %d -> %d" % (appended // db.page_size, n_sess0, len(chain)))

        ok = (list(p2) == list(v2) and list(p1) == list(v1) and list(p0) == list(orig_pos)
              and name2 == name and diff_confined and len(chain) == n_sess0 + 2)
        print("\n%s" % ("PASS" if ok else "FAIL"))
        return 0 if ok else 1
    finally:
        if os.path.exists(dst):
            os.remove(dst)


def _demo_da(src, exe, name):
    """Slice 2 demo: COW-commit a variable-length NAME (rename) and verify multi-version
    (new name on the latest session, old name on the previous), POS unchanged, byte-diff
    confined to page0's session pointer."""
    dst = "_e3d_write_full_da_demo.bin"
    shutil.copyfile(src, dst)
    try:
        ss = SchemaSet(exe)
        db = E3DDb(dst)
        bo = find_element(db, ss, name)
        if bo is None:
            print("element %r not found in %s" % (name, src))
            return 0
        refno = (db.u32(bo + 4), db.u32(bo + 8))
        name0, pos0 = read_name_via_root(db, ss, db.session_chain()[0]["index_root_pgno"], refno)
        orig_bytes, orig_len, n_sess0 = db.blob, len(db.blob), len(db.session_chain())
        new_name = name + "-COW-RENAMED"        # longer than original -> exercises DA growth
        print("target %s  refno=%r  POS=%s  -> rename to %r" % (name0, refno, pos0, new_name))

        _, rep = commit_file_da(dst, ss, name, NAME_HASH, new_name)

        db2 = E3DDb(dst)
        chain = db2.session_chain()
        new_name_rd, new_pos = read_name_via_root(db2, ss, chain[0]["index_root_pgno"], refno)
        old_name_rd, old_pos = read_name_via_root(db2, ss, chain[1]["index_root_pgno"], refno)

        edit = open(dst, "rb").read()
        diff = [i for i in range(min(orig_len, len(edit))) if orig_bytes[i] != edit[i]]
        diff_confined = len(diff) >= 1 and all(HDR_LATEST <= d < HDR_LATEST + 4 for d in diff)
        appended = len(edit) - orig_len

        print("\n-- DA-text (NAME) commit --")
        print("  DA words %d -> %d ;  sesno %d -> %d ;  data %d -> %d ;  appended %d pages"
              % (rep["da_words_old"], rep["da_words_new"], rep["old_sesno"], rep["new_sesno"],
                 rep["data_pgno_old"], rep["data_pgno_new"], appended // db.page_size))
        print("  NEW session: name=%r  POS=%s" % (new_name_rd, new_pos))
        print("  OLD session: name=%r  POS=%s" % (old_name_rd, old_pos))
        print("  diff bytes=%d at %s  (confined to page0 session ptr = %s)"
              % (len(diff), [hex(d) for d in diff], diff_confined))

        ok = (new_name_rd == new_name and old_name_rd == name
              and list(new_pos) == list(pos0) and list(old_pos) == list(pos0)
              and diff_confined and len(chain) == n_sess0 + 1)
        print("\n%s" % ("PASS" if ok else "FAIL"))
        return 0 if ok else 1
    finally:
        if os.path.exists(dst):
            os.remove(dst)


def _demo_insert(src, exe, name):
    """Slice 3 demo: COW-create a NEW element (clone of ``name`` with a fresh refno + NAME)
    via B-tree max-key insert, and verify it exists in the latest session (right noun/owner),
    is ABSENT from the previous session, element count +1, original bytes immutable."""
    dst = "_e3d_write_full_ins_demo.bin"
    shutil.copyfile(src, dst)
    try:
        ss = SchemaSet(exe)
        db = E3DDb(dst)
        bo = find_element(db, ss, name)
        if bo is None:
            print("element %r not found in %s" % (name, src))
            return 0
        src_el = decode_full_element(ss, db.blob, bo)
        orig_bytes, orig_len, n_sess0 = db.blob, len(db.blob), len(db.session_chain())
        new_name = "/NEW-ELEM-COW"
        print("clone source %s (noun=%s owner=%s)  -> new element %r" %
              (name, src_el["noun_name"], src_el["owner"], new_name))

        new_refno, rep = commit_file_insert(dst, ss, name, new_name)

        db2 = E3DDb(dst)
        chain = db2.session_chain()
        new_root, old_root = chain[0]["index_root_pgno"], chain[1]["index_root_pgno"]
        found = find_leaf_path(db2, new_root,
                               lambda r0, r1, cpg, off: (r0, r1) == new_refno and is_main_record(db2, cpg, off))
        new_el = decode_full_element(ss, db2.blob, found[1] * db2.page_size + found[2] * 2) if found else None
        old_has = find_leaf_path(db2, old_root, lambda r0, r1, cpg, off: (r0, r1) == new_refno) is not None
        # authoritative leaf entry count is word6-bounded (the reader's null-terminated
        # walk over-reads stale slots); the appended entry must drop free-words by 4.
        new_leaf_w6 = db2.u32(rep["new_leaf_pg"] * db2.page_size + 24)
        leaf_grew = new_leaf_w6 == rep["leaf_free_before"] - 4

        edit = open(dst, "rb").read()
        diff = [i for i in range(min(orig_len, len(edit))) if orig_bytes[i] != edit[i]]
        diff_confined = len(diff) >= 1 and all(HDR_LATEST <= d < HDR_LATEST + 4 for d in diff)

        print("\n-- B-tree insert (max-key append) --")
        print("  new refno=(0x%X,0x%X)  data page=%d off=%d ;  leaf %d (free %d w) -> %d (free %d w) ;  root %d -> %d"
              % (new_refno[0], new_refno[1], rep["data_pgno_new"], rep["src_off_words"],
                 rep["leaf_pg"], rep["leaf_free_before"], rep["new_leaf_pg"], new_leaf_w6,
                 rep["old_root"], rep["new_root"]))
        print("  sesno %d -> %d ;  pages appended=%d" % (rep["old_sesno"], rep["new_sesno"], len(rep["cow_pages"]) + 1))
        print("\n-- verify --")
        print("  NEW session: name=%r noun=%s owner=%s" %
              (new_el["element_name"] if new_el else None,
               new_el["noun_name"] if new_el else None, new_el["owner"] if new_el else None))
        print("  OLD session has new refno? %s   leaf entries +1 (word6 %d->%d) = %s"
              % (old_has, rep["leaf_free_before"], new_leaf_w6, leaf_grew))
        print("  diff bytes=%d at %s confined=%s" % (len(diff), [hex(d) for d in diff], diff_confined))

        ok = (new_el is not None and new_el["element_name"] == new_name
              and new_el["noun_name"] == src_el["noun_name"] and new_el["owner"] == src_el["owner"]
              and not old_has and leaf_grew
              and diff_confined and len(chain) == n_sess0 + 1)
        print("\n%s" % ("PASS" if ok else "FAIL"))
        return 0 if ok else 1
    finally:
        if os.path.exists(dst):
            os.remove(dst)


def _demo_delete(src, exe, name):
    """Slice 4 demo: full CRUD lifecycle. Create a temp element (clone of ``name``), then
    delete it; verify across the 3 resulting sessions that the new refno is
    absent(original) -> present(after insert) -> absent(after delete), the rightmost leaf's
    free-space round-trips, and the original bytes stay immutable."""
    dst = "_e3d_write_full_del_demo.bin"
    shutil.copyfile(src, dst)
    try:
        ss = SchemaSet(exe)
        db = E3DDb(dst)
        if find_element(db, ss, name) is None:
            print("element %r not found in %s" % (name, src))
            return 0
        orig_bytes, orig_len, n_sess0 = db.blob, len(db.blob), len(db.session_chain())

        new_refno, rep_i = commit_file_insert(dst, ss, name, "/TMP-CRUD-ELEM")
        rep_d = commit_file_delete(dst, ss, new_refno)

        db2 = E3DDb(dst)
        chain = db2.session_chain()

        def has_main(root):
            return find_leaf_path(db2, root, lambda r0, r1, cpg, off:
                                  (r0, r1) == new_refno and is_main_record(db2, cpg, off)) is not None

        after_del = has_main(chain[0]["index_root_pgno"])    # newest: deleted
        after_ins = has_main(chain[1]["index_root_pgno"])    # middle: created
        original = has_main(chain[2]["index_root_pgno"])     # oldest: never existed

        edit = open(dst, "rb").read()
        diff = [i for i in range(min(orig_len, len(edit))) if orig_bytes[i] != edit[i]]
        diff_confined = len(diff) >= 1 and all(HDR_LATEST <= d < HDR_LATEST + 4 for d in diff)
        leaf_roundtrip = rep_d["leaf_free_after"] == rep_i["leaf_free_before"]

        print("created refno=(0x%X,0x%X) then deleted it" % new_refno)
        print("\n-- CRUD lifecycle across sessions --")
        print("  sesno=%-3d (original)     has element? %s" % (chain[2]["sesno"], original))
        print("  sesno=%-3d (after insert) has element? %s" % (chain[1]["sesno"], after_ins))
        print("  sesno=%-3d (after delete) has element? %s" % (chain[0]["sesno"], after_del))
        print("  rightmost leaf free words: %d --insert--> %d --delete--> %d  (round-trip=%s)"
              % (rep_i["leaf_free_before"], rep_i["leaf_free_before"] - 4, rep_d["leaf_free_after"], leaf_roundtrip))
        print("  diff bytes=%d at %s confined=%s ;  sessions %d -> %d"
              % (len(diff), [hex(d) for d in diff], diff_confined, n_sess0, len(chain)))

        ok = (original is False and after_ins is True and after_del is False
              and leaf_roundtrip and diff_confined and len(chain) == n_sess0 + 2)
        print("\n%s" % ("PASS" if ok else "FAIL"))
        return 0 if ok else 1
    finally:
        if os.path.exists(dst):
            os.remove(dst)


def _demo_btree_synthetic():
    """Slice 5 (synthetic): build a B+-tree from scratch with a tiny capacity so a handful of
    inserts force recursive node splits + new-root creation, then assert every structural
    invariant (balanced, strictly sorted, separators == subtree minima, sentinel only on the
    leftmost spine, all keys present). This exercises the recursive/root-split paths that real
    sam7200 inserts cannot cheaply reach (its nodes hold up to 126 entries)."""
    import random

    class _FakeDB:
        page_size = 2048
        n_pages = 0

    PS, PW = 2048, 512
    db = _FakeDB()
    buf = bytearray(PS * 2)                       # page0 unused, page1 = empty root leaf
    root, base = 1, PS
    for w, val in ((0, 5), (1, INDEX_NOUN), (2, 0), (3, 2), (4, 2), (5, 0), (6, PW - 7)):
        _put32(buf, base + 4 * w, val)           # type5 / noun / level0 / key2 / data2 / -- / free
    cap, n = 3, 60
    keys = [(0x5C20, s) for s in range(1, n + 1)]
    random.Random(1234).shuffle(keys)            # random order -> stress sorted placement
    grew = 0
    for k in keys:
        root, g = cow_insert_leaf(buf, db, root, k, 2, 10, cap=cap)
        grew += int(g)

    rep = _btree_check(db, buf, root)
    expect = set((0x5C20, s) for s in range(1, n + 1))
    print("  inserted %d keys (cap=%d, random order); root grew %d time(s)" % (n, cap, grew))
    print("  height=%d index_pages=%d leaf_pages=%d entries=%d max_fanout<=cap=%s" %
          (rep["height"], rep["index_pages"], rep["leaf_pages"], rep["count"], rep["count"] == n))
    print("  balanced=%s sorted=%s nav_ok=%s dups=%d keyset_complete=%s" %
          (rep["balanced"], rep["sorted_ok"], rep["nav_ok"], rep["dups"], rep["keyset"] == expect))
    ok = (rep["count"] == n and rep["dups"] == 0 and rep["sorted_ok"] and rep["balanced"]
          and rep["nav_ok"] and rep["keyset"] == expect and rep["height"] >= 2 and grew >= 1)
    print("\n%s" % ("PASS" if ok else "FAIL"))
    return 0 if ok else 1


def _demo_insert_split(src, exe, name):
    """Slice 5 (real, leaf split): insert enough new max-key leaf entries (sharing the source
    element's data page) to overflow the rightmost leaf and force a B-tree node SPLIT. Verify
    with the validated reader that the new session enumerates orig+N entries (all originals
    preserved + all N new keys present), the previous session is unchanged, a leaf split
    actually occurred, the new tree stays balanced with correct separators, and the original
    bytes are immutable (diff confined to page0)."""
    dst = "_e3d_write_full_split_demo.bin"
    shutil.copyfile(src, dst)
    try:
        ss = SchemaSet(exe)
        db = E3DDb(dst)
        bo = find_element(db, ss, name)
        if bo is None:
            print("element %r not found in %s" % (name, src)); return 0
        PS = db.page_size
        data_pg, off = bo // PS, (bo % PS) // 2
        orig_bytes, orig_len, n_sess0 = db.blob, len(db.blob), len(db.session_chain())
        root0 = db.session_chain()[0]["index_root_pgno"]
        chk0 = _btree_check(db, bytearray(db.blob), root0)
        orig_keys = set(chk0["keyset"])
        leaf_pg, _ = _rightmost_path(db, root0)
        free = db.u32(leaf_pg * PS + 24)
        N = free // 4 + 6                          # overflow the rightmost leaf + a few extra
        mx = max(db.walk_index(root0), key=lambda e: (e[0], e[1]))
        new_keys = [(mx[0], mx[1] + i) for i in range(1, N + 1)]

        buf = bytearray(db.blob)
        root, grew = root0, False
        for k in new_keys:
            root, g = cow_insert_leaf(buf, db, root, k, data_pg, off)
            grew = grew or g
        _append_session(buf, db, root)
        with open(dst, "wb") as f:
            f.write(buf)

        db2 = E3DDb(dst)
        chain = db2.session_chain()
        chk_new = _btree_check(db2, bytearray(db2.blob), chain[0]["index_root_pgno"])
        chk_old = _btree_check(db2, bytearray(db2.blob), chain[1]["index_root_pgno"])
        all_new = set(new_keys) <= chk_new["keyset"]
        kept = orig_keys <= chk_new["keyset"]
        old_same = chk_old["keyset"] == orig_keys
        split = chk_new["leaf_pages"] > chk0["leaf_pages"]

        edit = open(dst, "rb").read()
        diff = [i for i in range(min(orig_len, len(edit))) if orig_bytes[i] != edit[i]]
        confined = len(diff) >= 1 and all(HDR_LATEST <= d < HDR_LATEST + 4 for d in diff)

        print("  rightmost leaf free=%d w -> inserting N=%d new max-keys (root grew=%s)" % (free, N, grew))
        print("  leaf_pages %d -> %d (split=%s)  height %d -> %d  index_pages %d -> %d" %
              (chk0["leaf_pages"], chk_new["leaf_pages"], split, chk0["height"], chk_new["height"],
               chk0["index_pages"], chk_new["index_pages"]))
        print("  new session entries=%d (orig %d + %d)  all-new-present=%s originals-kept=%s" %
              (chk_new["count"], chk0["count"], N, all_new, kept))
        print("  new tree balanced=%s nav_ok=%s sorted=%s ;  prev session unchanged=%s" %
              (chk_new["balanced"], chk_new["nav_ok"], chk_new["sorted_ok"], old_same))
        print("  diff bytes=%d at %s confined=%s ;  sessions %d->%d" %
              (len(diff), [hex(d) for d in diff], confined, n_sess0, len(chain)))

        ok = (split and all_new and kept and old_same and chk_new["count"] == chk0["count"] + N
              and chk_new["balanced"] and chk_new["nav_ok"] and chk_new["sorted_ok"]
              and confined and len(chain) == n_sess0 + 1)
        print("\n%s" % ("PASS" if ok else "FAIL"))
        return 0 if ok else 1
    finally:
        if os.path.exists(dst):
            os.remove(dst)


def _demo_insert_mid(src, exe, name):
    """Slice 5 (real, arbitrary key): clone the source element to a FREE refno in the MIDDLE of
    the key range (not a max-key append) and insert it via the general B+-tree insert. Verify
    the new element decodes correctly (noun/owner/name) in the new session, is absent from the
    previous session, the tree stays balanced/sorted with correct separators, and the original
    bytes are immutable."""
    dst = "_e3d_write_full_mid_demo.bin"
    shutil.copyfile(src, dst)
    try:
        ss = SchemaSet(exe)
        db = E3DDb(dst)
        bo = find_element(db, ss, name)
        if bo is None:
            print("element %r not found in %s" % (name, src)); return 0
        src_el = decode_full_element(ss, db.blob, bo)
        root0 = db.session_chain()[0]["index_root_pgno"]
        orig_bytes, orig_len, n_sess0 = db.blob, len(db.blob), len(db.session_chain())
        keys = sorted(set((r0, r1) for (r0, r1, _, _) in db.walk_index(root0)))
        dbno, maxk = db.u32(bo + 4), max(keys)
        seqs = sorted(s for (d, s) in keys if d == dbno)
        present = set((dbno, s) for s in seqs)
        new_refno = None                            # a free seq in the middle of this dbno range
        for k in range(len(seqs) // 4, min(3 * len(seqs) // 4, len(seqs) - 1)):
            cand = (dbno, seqs[k] + 1)
            if seqs[k + 1] > seqs[k] + 1 and cand not in present:
                new_refno = cand; break
        if new_refno is None:
            print("  no middle gap found; skipping"); return 0
        new_name = "/MID-INSERT-COW"
        print("clone %s (noun=%s) -> MIDDLE element refno=(0x%X,0x%X) name=%r  (max refno=(0x%X,0x%X))" %
              (name, src_el["noun_name"], new_refno[0], new_refno[1], new_name, maxk[0], maxk[1]))

        chk0 = _btree_check(db, bytearray(db.blob), root0)
        buf = bytearray(db.blob)
        rep = cow_insert_element_split(buf, db, bo, new_refno, new_name)
        with open(dst, "wb") as f:
            f.write(buf)

        db2 = E3DDb(dst)
        chain = db2.session_chain()
        new_root, old_root = chain[0]["index_root_pgno"], chain[1]["index_root_pgno"]
        found = find_leaf_path(db2, new_root,
                               lambda r0, r1, cpg, off: (r0, r1) == new_refno and is_main_record(db2, cpg, off))
        new_el = decode_full_element(ss, db2.blob, found[1] * db2.page_size + found[2] * 2) if found else None
        old_has = find_leaf_path(db2, old_root, lambda r0, r1, cpg, off: (r0, r1) == new_refno) is not None
        chk_new = _btree_check(db2, bytearray(db2.blob), new_root)

        edit = open(dst, "rb").read()
        diff = [i for i in range(min(orig_len, len(edit))) if orig_bytes[i] != edit[i]]
        confined = len(diff) >= 1 and all(HDR_LATEST <= d < HDR_LATEST + 4 for d in diff)

        print("\n-- arbitrary-key insert --")
        print("  is-middle (new<max)=%s  tree_grew=%s  entries %d -> %d (+1)" %
              (new_refno < maxk, rep["tree_grew"], chk0["count"], chk_new["count"]))
        print("  NEW session: name=%r noun=%s owner=%s" %
              (new_el["element_name"] if new_el else None,
               new_el["noun_name"] if new_el else None, new_el["owner"] if new_el else None))
        print("  OLD session has it? %s ;  new tree balanced=%s nav_ok=%s sorted=%s" %
              (old_has, chk_new["balanced"], chk_new["nav_ok"], chk_new["sorted_ok"]))
        print("  diff bytes=%d at %s confined=%s ;  sessions %d->%d" %
              (len(diff), [hex(d) for d in diff], confined, n_sess0, len(chain)))

        ok = (new_refno < maxk and new_el is not None and new_el["element_name"] == new_name
              and new_el["noun_name"] == src_el["noun_name"] and new_el["owner"] == src_el["owner"]
              and not old_has and chk_new["count"] == chk0["count"] + 1
              and chk_new["balanced"] and chk_new["nav_ok"] and chk_new["sorted_ok"]
              and confined and len(chain) == n_sess0 + 1)
        print("\n%s" % ("PASS" if ok else "FAIL"))
        return 0 if ok else 1
    finally:
        if os.path.exists(dst):
            os.remove(dst)


def _da_node_chain_len(db: "E3DDb", buf: bytearray, record_bo: int, which: int = 1) -> int:
    """Count the nodes in a record's list chain (``which``=1 DA via rec[6]/rec[7], 2 members via
    rec[8]/rec[9]); follows node[3]=next page / node[4]=next off, verifying node type==which."""
    PS = db.page_size
    rec = lambda i: _u32(buf, record_bo + 4 * i)
    page, off = (rec(6), (rec(7) >> 13) & 0xFFF) if which == 1 else (rec(8), (rec(9) >> 13) & 0xFFF)
    n, guard = 0, 0
    while page and guard < 128:
        guard += 1
        node = page * PS + off * 4
        if node + 20 > len(buf) or ((_u32(buf, node) >> 16) & 0xF) != which:
            break
        n += 1
        page, off = _u32(buf, node + 12), (_u32(buf, node + 16) >> 13) & 0xFFF
    return n


def read_members_via_root(db: "E3DDb", ss: "SchemaSet", root: int, refno: tuple):
    """Resolve ``refno``'s main record via a session ``root``; return its child refno list."""
    found = find_leaf_path(db, root, lambda r0, r1, cpg, off: (r0, r1) == refno and is_main_record(db, cpg, off))
    if not found:
        return None
    return read_members(db.blob, found[1] * db.page_size + found[2] * 2, db.page_size)


def _find_member_element(db: "E3DDb", lo: int = 1, hi: int = 8):
    """Find the first main record with ``lo<=#children<=hi``. Returns ``(refno, bo, [children])``."""
    PS = db.page_size
    seen = set()
    for (r0, r1, cpg, off) in db.walk_index(db.session_chain()[0]["index_root_pgno"]):
        if not is_main_record(db, cpg, off):
            continue
        bo = cpg * PS + off * 2
        refno = (db.u32(bo + 4), db.u32(bo + 8))
        if refno in seen:
            continue
        seen.add(refno)
        if lo <= (db.u32(bo + 40) & 0x3FFF) // 2 <= hi:
            return refno, bo, read_members(db.blob, bo, PS)
    return None


def _demo_da_xpage(src, exe, name):
    """Slice 6a demo: COW-rename via DA RELOCATION onto a fresh page (cross-page DA, multi-page
    COW). Verify the chained-aware reader resolves the new name in the new session with the DA
    now on a DIFFERENT page than the record (cross-page), POS unchanged, the previous session
    still shows the old name, and the original bytes stay immutable (diff confined to page0)."""
    dst = "_e3d_write_full_xpage_demo.bin"
    shutil.copyfile(src, dst)
    try:
        ss = SchemaSet(exe)
        db = E3DDb(dst)
        bo = find_element(db, ss, name)
        if bo is None:
            print("element %r not found in %s" % (name, src)); return 0
        refno = (db.u32(bo + 4), db.u32(bo + 8))
        name0, pos0 = read_name_via_root(db, ss, db.session_chain()[0]["index_root_pgno"], refno)
        orig_bytes, orig_len, n_sess0 = db.blob, len(db.blob), len(db.session_chain())
        new_name = name + "-XPAGE-COW"
        print("target %s  refno=%r  POS=%s  -> relocate DA + rename to %r" % (name0, refno, pos0, new_name))

        buf = bytearray(db.blob)
        rep = cow_commit_da_text_xpage(buf, db, bo, NAME_HASH, new_name)
        with open(dst, "wb") as f:
            f.write(buf)

        db2 = E3DDb(dst)
        chain = db2.session_chain()
        found = find_leaf_path(db2, chain[0]["index_root_pgno"],
                               lambda r0, r1, cpg, off: (r0, r1) == refno and is_main_record(db2, cpg, off))
        new_rec_pg = found[1]
        da_pg = db2.u32(found[1] * db2.page_size + found[2] * 2 + 24)   # rec[6] of the new record
        cross_page = da_pg != new_rec_pg
        new_name_rd, new_pos = read_name_via_root(db2, ss, chain[0]["index_root_pgno"], refno)
        old_name_rd, old_pos = read_name_via_root(db2, ss, chain[1]["index_root_pgno"], refno)

        edit = open(dst, "rb").read()
        diff = [i for i in range(min(orig_len, len(edit))) if orig_bytes[i] != edit[i]]
        confined = len(diff) >= 1 and all(HDR_LATEST <= d < HDR_LATEST + 4 for d in diff)

        print("\n-- cross-page DA relocation --")
        print("  DA payload %d -> %d words ;  DA page=%d (record page=%d) cross-page=%s ;  nodes=%d ;  appended %d pages"
              % (rep["da_words_old"], rep["da_words_new"], da_pg, new_rec_pg, cross_page,
                 rep["da_nodes"], (len(edit) - orig_len) // db.page_size))
        print("  NEW session: name=%r  POS=%s" % (new_name_rd, new_pos))
        print("  OLD session: name=%r  POS=%s" % (old_name_rd, old_pos))
        print("  diff bytes=%d at %s confined=%s" % (len(diff), [hex(d) for d in diff], confined))

        ok = (new_name_rd == new_name and old_name_rd == name0 and cross_page
              and list(new_pos) == list(pos0) and list(old_pos) == list(pos0)
              and confined and len(chain) == n_sess0 + 1)
        print("\n%s" % ("PASS" if ok else "FAIL"))
        return 0 if ok else 1
    finally:
        if os.path.exists(dst):
            os.remove(dst)


def _demo_da_chained(src, exe, name):
    """Slice 6b demo: COW-rename re-emitting the DA as a FORCED multi-node chain (tiny nodes
    spread across several fresh pages) — exercises the chunked-chain WRITER and the chain-walk
    READER together. Verify the new record's DA chain is genuinely >=2 nodes, the chained-aware
    reader still reads the new name (so writer+reader agree), POS unchanged, the previous
    session shows the old name, and the original bytes stay immutable."""
    dst = "_e3d_write_full_chain_demo.bin"
    shutil.copyfile(src, dst)
    try:
        ss = SchemaSet(exe)
        db = E3DDb(dst)
        bo = find_element(db, ss, name)
        if bo is None:
            print("element %r not found in %s" % (name, src)); return 0
        refno = (db.u32(bo + 4), db.u32(bo + 8))
        name0, pos0 = read_name_via_root(db, ss, db.session_chain()[0]["index_root_pgno"], refno)
        orig_bytes, orig_len, n_sess0 = db.blob, len(db.blob), len(db.session_chain())
        new_name = name + "-CHAINED-DA"
        print("target %s  refno=%r  -> rename to %r as a forced multi-node DA chain" % (name0, refno, new_name))

        buf = bytearray(db.blob)
        rep = cow_commit_da_text_xpage(buf, db, bo, NAME_HASH, new_name, force_chunk=4)  # tiny nodes -> chain
        with open(dst, "wb") as f:
            f.write(buf)

        db2 = E3DDb(dst)
        chain = db2.session_chain()
        found = find_leaf_path(db2, chain[0]["index_root_pgno"],
                               lambda r0, r1, cpg, off: (r0, r1) == refno and is_main_record(db2, cpg, off))
        nodes_walked = _da_node_chain_len(db2, bytearray(db2.blob), found[1] * db2.page_size + found[2] * 2)
        new_name_rd, new_pos = read_name_via_root(db2, ss, chain[0]["index_root_pgno"], refno)
        old_name_rd, old_pos = read_name_via_root(db2, ss, chain[1]["index_root_pgno"], refno)

        edit = open(dst, "rb").read()
        diff = [i for i in range(min(orig_len, len(edit))) if orig_bytes[i] != edit[i]]
        confined = len(diff) >= 1 and all(HDR_LATEST <= d < HDR_LATEST + 4 for d in diff)

        print("\n-- chained multi-page DA (forced 4-word nodes) --")
        print("  emitted nodes=%d ;  walked chain length=%d (>=2 => real cross-page chain) ;  DA payload=%d words"
              % (rep["da_nodes"], nodes_walked, rep["da_words_new"]))
        print("  NEW session: name=%r  POS=%s" % (new_name_rd, new_pos))
        print("  OLD session: name=%r  POS=%s" % (old_name_rd, old_pos))
        print("  diff bytes=%d at %s confined=%s" % (len(diff), [hex(d) for d in diff], confined))

        ok = (rep["da_nodes"] >= 2 and nodes_walked >= 2 and new_name_rd == new_name
              and old_name_rd == name0 and list(new_pos) == list(pos0) and list(old_pos) == list(pos0)
              and confined and len(chain) == n_sess0 + 1)
        print("\n%s" % ("PASS" if ok else "FAIL"))
        return 0 if ok else 1
    finally:
        if os.path.exists(dst):
            os.remove(dst)


def _demo_members_xpage(src, exe, name):
    """Slice 7a demo: relocate an element's MEMBER (child) list onto fresh page(s) AND add a
    child, forcing a multi-node type-2 chain. Verify the new session lists old children + the
    new one, member words grew by 2, the member list is now on a DIFFERENT page (cross-page) and
    a real >=2-node chain, the previous session is unchanged, and the original bytes immutable."""
    dst = "_e3d_write_full_mem_demo.bin"
    shutil.copyfile(src, dst)
    try:
        ss = SchemaSet(exe)
        db = E3DDb(dst)
        hit = _find_member_element(db, 1, 8)
        if hit is None:
            print("no element with members found"); return 0
        refno, bo, children0 = hit
        n_sess0 = len(db.session_chain())
        orig_bytes, orig_len = db.blob, len(db.blob)
        new_child = (0xABCD, 0x1234)                       # a distinctive marker child refno
        new_children = children0 + [new_child]
        print("element refno=(0x%X,0x%X)  has %d children -> add (0x%X,0x%X) + relocate as chain"
              % (refno[0], refno[1], len(children0), new_child[0], new_child[1]))

        buf = bytearray(db.blob)
        rep = cow_members_set(buf, db, bo, new_children, force_chunk=2)   # 1 child/node -> chain
        with open(dst, "wb") as f:
            f.write(buf)

        db2 = E3DDb(dst)
        chain = db2.session_chain()
        found = find_leaf_path(db2, chain[0]["index_root_pgno"],
                               lambda r0, r1, cpg, off: (r0, r1) == refno and is_main_record(db2, cpg, off))
        new_rec_pg = found[1]
        mem_pg = db2.u32(found[1] * db2.page_size + found[2] * 2 + 32)    # rec[8] of new record
        cross_page = mem_pg != new_rec_pg
        nodes = _da_node_chain_len(db2, bytearray(db2.blob), found[1] * db2.page_size + found[2] * 2, 2)
        new_kids = read_members_via_root(db2, ss, chain[0]["index_root_pgno"], refno)
        old_kids = read_members_via_root(db2, ss, chain[1]["index_root_pgno"], refno)

        edit = open(dst, "rb").read()
        diff = [i for i in range(min(orig_len, len(edit))) if orig_bytes[i] != edit[i]]
        confined = len(diff) >= 1 and all(HDR_LATEST <= d < HDR_LATEST + 4 for d in diff)

        print("\n-- member relocation + add child (type-2 chain) --")
        print("  member words %d -> %d ;  member page=%d (record page=%d) cross-page=%s ;  nodes=%d"
              % (rep["member_words_old"], rep["member_words_new"], mem_pg, new_rec_pg, cross_page, rep["member_nodes"]))
        print("  NEW children (%d): %s" % (len(new_kids), ["(0x%X,0x%X)" % c for c in new_kids]))
        print("  OLD children (%d): %s" % (len(old_kids), ["(0x%X,0x%X)" % c for c in old_kids]))
        print("  walked chain nodes=%d ;  diff bytes=%d at %s confined=%s"
              % (nodes, len(diff), [hex(d) for d in diff], confined))

        ok = (new_kids == new_children and old_kids == children0 and cross_page
              and nodes >= 2 and rep["member_nodes"] >= 2
              and rep["member_words_new"] == rep["member_words_old"] + 2
              and confined and len(chain) == n_sess0 + 1)
        print("\n%s" % ("PASS" if ok else "FAIL"))
        return 0 if ok else 1
    finally:
        if os.path.exists(dst):
            os.remove(dst)


def _demo_members_roundtrip(src, exe, name):
    """Slice 7b demo: member resize round-trip — add a child then remove it (stacked commits).
    Verify across the 3 sessions that the child list goes original -> +marker -> original, the
    member words round-trip, and the original bytes stay immutable (diff confined to page0)."""
    dst = "_e3d_write_full_memrt_demo.bin"
    shutil.copyfile(src, dst)
    try:
        ss = SchemaSet(exe)
        db = E3DDb(dst)
        hit = _find_member_element(db, 1, 8)
        if hit is None:
            print("no element with members found"); return 0
        refno, _, children0 = hit
        n_sess0 = len(db.session_chain())
        orig_bytes, orig_len = db.blob, len(db.blob)
        marker = (0xABCD, 0x1234)
        print("element refno=(0x%X,0x%X)  %d children -> add %r then remove it"
              % (refno[0], refno[1], len(children0), marker))

        def _bo(d):
            f = find_leaf_path(d, d.session_chain()[0]["index_root_pgno"],
                               lambda r0, r1, cpg, off: (r0, r1) == refno and is_main_record(d, cpg, off))
            return f[1] * d.page_size + f[2] * 2

        d1 = E3DDb(dst); b1 = bytearray(d1.blob)              # commit #1: add the marker child
        rep1 = cow_members_set(b1, d1, _bo(d1), children0 + [marker])
        open(dst, "wb").write(b1)
        d2 = E3DDb(dst); b2 = bytearray(d2.blob)              # commit #2: remove the marker child
        cur = read_members(d2.blob, _bo(d2), d2.page_size)
        rep2 = cow_members_set(b2, d2, _bo(d2), [c for c in cur if c != marker])
        open(dst, "wb").write(b2)

        db3 = E3DDb(dst)
        chain = db3.session_chain()
        k_after_del = read_members_via_root(db3, ss, chain[0]["index_root_pgno"], refno)
        k_after_add = read_members_via_root(db3, ss, chain[1]["index_root_pgno"], refno)
        k_orig = read_members_via_root(db3, ss, chain[2]["index_root_pgno"], refno)

        edit = open(dst, "rb").read()
        diff = [i for i in range(min(orig_len, len(edit))) if orig_bytes[i] != edit[i]]
        confined = len(diff) >= 1 and all(HDR_LATEST <= d < HDR_LATEST + 4 for d in diff)

        print("\n-- member resize round-trip across sessions --")
        print("  original  (sesno %d): %d children %s" % (chain[2]["sesno"], len(k_orig), ["(0x%X,0x%X)" % c for c in k_orig]))
        print("  after add (sesno %d): %d children (has marker=%s)" % (chain[1]["sesno"], len(k_after_add), marker in k_after_add))
        print("  after del (sesno %d): %d children (has marker=%s)" % (chain[0]["sesno"], len(k_after_del), marker in k_after_del))
        print("  member words %d --add--> %d --del--> %d  (round-trip=%s)"
              % (rep1["member_words_old"], rep1["member_words_new"], rep2["member_words_new"],
                 rep2["member_words_new"] == rep1["member_words_old"]))
        print("  diff bytes=%d confined=%s ;  sessions %d -> %d" % (len(diff), confined, n_sess0, len(chain)))

        ok = (k_orig == children0 and marker in k_after_add and len(k_after_add) == len(children0) + 1
              and k_after_del == children0 and marker not in k_after_del
              and rep2["member_words_new"] == rep1["member_words_old"]
              and confined and len(chain) == n_sess0 + 2)
        print("\n%s" % ("PASS" if ok else "FAIL"))
        return 0 if ok else 1
    finally:
        if os.path.exists(dst):
            os.remove(dst)


def read_uda_via_root(db: "E3DDb", ss: "SchemaSet", root: int, refno: tuple):
    """Resolve ``refno``'s main record via a session ``root``; return its UDA entry list."""
    found = find_leaf_path(db, root, lambda r0, r1, cpg, off: (r0, r1) == refno and is_main_record(db, cpg, off))
    if not found:
        return None
    return read_uda(db.blob, found[1] * db.page_size + found[2] * 2, db.page_size)


def _find_uda_element(db: "E3DDb", want_type: int = None, min_uda: int = 1):
    """Find the first main record carrying >=``min_uda`` UDAs (and a UDA of ``want_type`` if set).
    Returns ``(refno, bo, [uda entries])``."""
    PS = db.page_size
    seen = set()
    for (r0, r1, cpg, off) in db.walk_index(db.session_chain()[0]["index_root_pgno"]):
        if not is_main_record(db, cpg, off):
            continue
        bo = cpg * PS + off * 2
        refno = (db.u32(bo + 4), db.u32(bo + 8))
        if refno in seen:
            continue
        seen.add(refno)
        udas = read_uda(db.blob, bo, PS)
        if len(udas) >= min_uda and (want_type is None or any(u["type"] == want_type for u in udas)):
            return refno, bo, udas
    return None


def _demo_uda_edit(src, exe, name):
    """Slice 8a demo: edit a strongly-typed UDA value (a type-4 ref UDA) on an element that also
    carries another UDA. Verify the new session shows the new ref value, the OTHER UDA(s) on the
    element are preserved byte-for-byte, the previous session keeps the old value, and the
    original bytes are immutable (diff confined to page0)."""
    dst = "_e3d_write_full_uda_demo.bin"
    shutil.copyfile(src, dst)
    try:
        ss = SchemaSet(exe)
        db = E3DDb(dst)
        hit = _find_uda_element(db, want_type=4, min_uda=2)
        if hit is None:
            print("no element with a type-4 UDA + >=2 UDAs found"); return 0
        refno, bo, udas = hit
        target = next(u for u in udas if u["type"] == 4)
        others = {u["hash"]: tuple(u["value"]) for u in udas if u["hash"] != target["hash"]}
        n_sess0, orig_bytes, orig_len = len(db.session_chain()), db.blob, len(db.blob)
        new_ref = [99, 12345]
        print("element refno=(0x%X,0x%X)  UDAs=%s ; edit type-4 UDA 0x%X %s -> %s"
              % (refno[0], refno[1], ["0x%X(t%d)" % (u["hash"], u["type"]) for u in udas],
                 target["hash"], target["value"], new_ref))

        buf = bytearray(db.blob)
        rep = cow_da_set_entry(buf, db, bo, target["hash"], 4, new_ref)
        with open(dst, "wb") as f:
            f.write(buf)

        db2 = E3DDb(dst)
        chain = db2.session_chain()
        new_u = {u["hash"]: tuple(u["value"]) for u in read_uda_via_root(db2, ss, chain[0]["index_root_pgno"], refno)}
        old_u = {u["hash"]: tuple(u["value"]) for u in read_uda_via_root(db2, ss, chain[1]["index_root_pgno"], refno)}
        others_ok = all(new_u.get(h) == v for h, v in others.items())

        edit = open(dst, "rb").read()
        diff = [i for i in range(min(orig_len, len(edit))) if orig_bytes[i] != edit[i]]
        confined = len(diff) >= 1 and all(HDR_LATEST <= d < HDR_LATEST + 4 for d in diff)

        print("\n-- UDA strongly-typed value edit (type-4 ref) --")
        print("  NEW session: UDA 0x%X = %s ;  other UDAs preserved=%s" % (target["hash"], new_u.get(target["hash"]), others_ok))
        print("  OLD session: UDA 0x%X = %s" % (target["hash"], old_u.get(target["hash"])))
        print("  is_uda=%s ;  DA nodes=%d ;  diff bytes=%d at %s confined=%s"
              % (rep["is_uda"], rep["da_nodes"], len(diff), [hex(d) for d in diff], confined))

        ok = (new_u.get(target["hash"]) == tuple(new_ref) and old_u.get(target["hash"]) == tuple(target["value"])
              and others_ok and rep["is_uda"] and confined and len(chain) == n_sess0 + 1)
        print("\n%s" % ("PASS" if ok else "FAIL"))
        return 0 if ok else 1
    finally:
        if os.path.exists(dst):
            os.remove(dst)


def _demo_uda_add_remove(src, exe, name):
    """Slice 8b demo: add a synthetic UDA entry (hash>threshold, type-4 ref) to an element, then
    remove it (stacked commits). Verify across the 3 sessions that the UDA set goes original ->
    +new -> original, and the original bytes stay immutable (diff confined to page0)."""
    dst = "_e3d_write_full_udaar_demo.bin"
    shutil.copyfile(src, dst)
    try:
        ss = SchemaSet(exe)
        db = E3DDb(dst)
        bo = find_element(db, ss, name)
        if bo is None:
            print("element %r not found in %s" % (name, src)); return 0
        refno = (db.u32(bo + 4), db.u32(bo + 8))
        n_sess0, orig_bytes, orig_len = len(db.session_chain()), db.blob, len(db.blob)
        new_hash, new_val = 0x2C00FFFF, [0xAAAA, 0xBBBB]   # synthetic UDA (hash>threshold), type-4 ref
        u0 = read_uda(db.blob, bo, db.page_size)
        print("element %s refno=(0x%X,0x%X)  %d UDAs -> add UDA 0x%X=%s then remove"
              % (name, refno[0], refno[1], len(u0), new_hash, new_val))

        def _bo(d):
            f = find_leaf_path(d, d.session_chain()[0]["index_root_pgno"],
                               lambda r0, r1, cpg, off: (r0, r1) == refno and is_main_record(d, cpg, off))
            return f[1] * d.page_size + f[2] * 2

        d1 = E3DDb(dst); b1 = bytearray(d1.blob)                 # commit #1: add the UDA
        cow_da_set_entry(b1, d1, _bo(d1), new_hash, 4, new_val)
        open(dst, "wb").write(b1)
        d2 = E3DDb(dst); b2 = bytearray(d2.blob)                 # commit #2: remove the UDA
        cow_da_remove_entry(b2, d2, _bo(d2), new_hash)
        open(dst, "wb").write(b2)

        db3 = E3DDb(dst)
        chain = db3.session_chain()

        def uset(root):
            return {u["hash"]: tuple(u["value"]) for u in read_uda_via_root(db3, ss, root, refno)}
        after_del, after_add, orig = (uset(chain[0]["index_root_pgno"]),
                                      uset(chain[1]["index_root_pgno"]), uset(chain[2]["index_root_pgno"]))

        edit = open(dst, "rb").read()
        diff = [i for i in range(min(orig_len, len(edit))) if orig_bytes[i] != edit[i]]
        confined = len(diff) >= 1 and all(HDR_LATEST <= d < HDR_LATEST + 4 for d in diff)

        print("\n-- UDA add / remove round-trip across sessions --")
        print("  original  (sesno %d): %d UDAs (has 0x%X=%s)" % (chain[2]["sesno"], len(orig), new_hash, new_hash in orig))
        print("  after add (sesno %d): %d UDAs (0x%X=%s)" % (chain[1]["sesno"], len(after_add), new_hash, after_add.get(new_hash)))
        print("  after del (sesno %d): %d UDAs (has 0x%X=%s)" % (chain[0]["sesno"], len(after_del), new_hash, new_hash in after_del))
        print("  diff bytes=%d confined=%s ;  sessions %d -> %d" % (len(diff), confined, n_sess0, len(chain)))

        ok = (new_hash not in orig and after_add.get(new_hash) == tuple(new_val) and new_hash not in after_del
              and set(after_del) == set(orig) and confined and len(chain) == n_sess0 + 2)
        print("\n%s" % ("PASS" if ok else "FAIL"))
        return 0 if ok else 1
    finally:
        if os.path.exists(dst):
            os.remove(dst)


def _demo_verify_batch(src, exe, name):
    """Slice 9: multi-edit BATCH (set-pos + insert) committed as ONE session, then verified.
    Mirrors the Rust ``batch`` + ``verify_commit`` tests (FR-019/FR-020, SC-008)."""
    dst = "_e3d_verify_batch_demo.bin"
    shutil.copyfile(src, dst)
    try:
        ss = SchemaSet(exe)
        db = E3DDb(dst)
        bo = find_element(db, ss, name)
        if bo is None:
            print("element %r not found" % name)
            return 0
        refno = (db.u32(bo + 4), db.u32(bo + 8))
        orig_bytes = db.blob
        base_sesno = db.session_chain()[0]["sesno"]
        n0 = len(db.session_chain())
        pos = (100.0, 200.0, 300.5)
        clone_name = name + "-BATCH"

        edits = [
            lambda d: commit_file(d, ss, name, POS_HASH, pos),
            lambda d: commit_file_insert(d, ss, name, clone_name),
        ]
        new_sesno = batch_commit(dst, ss, edits)

        db2 = E3DDb(dst)
        chain = db2.session_chain()
        root = chain[0]["index_root_pgno"]
        newpos, _, _ = read_attr_via_root(db2, ss, root, refno, POS_HASH)
        clone_bo = find_element(db2, ss, clone_name)
        issues = verify_commit(orig_bytes, dst, ss, [(refno, POS_HASH, pos)])
        single = (new_sesno == base_sesno + 1) and (len(chain) == n0 + 1)

        print("-- batch: set-pos %s + insert %s --" % (pos, clone_name))
        print("  sesno %d -> %d ;  single session=%s (chain %d -> %d)"
              % (base_sesno, new_sesno, single, n0, len(chain)))
        print("  %s POS=%s ;  clone present=%s ;  verify issues=%s"
              % (name, newpos, clone_bo is not None, issues))
        ok = (list(newpos or []) == list(pos) and clone_bo is not None and not issues and single)
        print("%s" % ("PASS" if ok else "FAIL"))
        return 0 if ok else 1
    finally:
        if os.path.exists(dst):
            os.remove(dst)


if __name__ == "__main__":
    args = sys.argv[1:]
    exe = r"D:\AVEVA\Everything3D2.10"
    name = "/WB1"
    if "--exe" in args:
        i = args.index("--exe"); exe = args[i + 1]; del args[i:i + 2]
    if "--name" in args:
        i = args.index("--name"); name = args[i + 1]; del args[i:i + 2]
    src = args[0] if args else os.path.join("pdms-test-data", "sam7200_0001")
    print("==== Slice 1: inline value (POS) COW commit ====")
    rc1 = _demo(src, exe, name)
    print("\n==== Slice 2: variable-length DA text (NAME) COW commit ====")
    rc2 = _demo_da(src, exe, name)
    print("\n==== Slice 3: new element via B-tree insert (max-key, no split) ====")
    rc3 = _demo_insert(src, exe, name)
    print("\n==== Slice 4: delete element (full CRUD lifecycle) ====")
    rc4 = _demo_delete(src, exe, name)
    print("\n==== Slice 5a: B-tree recursive split + new root (synthetic, cap=3) ====")
    rc5a = _demo_btree_synthetic()
    print("\n==== Slice 5b: node split on a full leaf (real, sam7200) ====")
    rc5b = _demo_insert_split(src, exe, name)
    print("\n==== Slice 5c: arbitrary middle-key insert (real, sam7200) ====")
    rc5c = _demo_insert_mid(src, exe, name)
    print("\n==== Slice 6a: cross-page DA relocation (real, sam7200) ====")
    rc6a = _demo_da_xpage(src, exe, name)
    print("\n==== Slice 6b: chained multi-page DA rewrite (real, sam7200) ====")
    rc6b = _demo_da_chained(src, exe, name)
    print("\n==== Slice 7a: member-list relocation + add child (real, sam7200) ====")
    rc7a = _demo_members_xpage(src, exe, name)
    print("\n==== Slice 7b: member resize round-trip (add then remove child) ====")
    rc7b = _demo_members_roundtrip(src, exe, name)
    print("\n==== Slice 8a: edit a strongly-typed UDA value (real, sam7200) ====")
    rc8a = _demo_uda_edit(src, exe, name)
    print("\n==== Slice 8b: add then remove a UDA entry (round-trip) ====")
    rc8b = _demo_uda_add_remove(src, exe, name)
    print("\n==== Slice 9: verify_commit + batch (multi-edit single session) ====")
    rc9 = _demo_verify_batch(src, exe, name)
    sys.exit(rc1 or rc2 or rc3 or rc4 or rc5a or rc5b or rc5c or rc6a or rc6b or rc7a or rc7b or rc8a or rc8b or rc9)
