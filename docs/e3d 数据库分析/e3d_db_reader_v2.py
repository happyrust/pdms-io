#!/usr/bin/env python3
"""
E3D / PDMS 数据库(.db)参考读取器 v2  —— 修正版

基于 AVEVA Everything3D 2.10 core.dll 实时逆向 + 真实样本核验编写。
相对旧版 e3d_db_reader 的关键修正:
  1. 页大小 = header[0x34] * 4  (该字段是"32位字数",不是字节数;实测均为 2048)
  2. noun/属性名哈希 = base-27 + 偏移 0x81BF1 (db1_hash/db1_dehash),非 base-26

能力:
  - 解析文件头 (PdmsHeader)
  - 页类型直方图
  - 沿会话链 (last_ses_pgno) 回溯所有会话 (sesno/时间/计算机名/索引根)
  - 从最新会话的索引根出发遍历 B-树,枚举 refno -> 物理位置
  - 解析元素记录 (refno, noun 名, owner, page_no) 并解码隐式区中的 POS 双精度

用法: python e3d_db_reader_v2.py <db_file> [max_elements]
"""
import os
import struct
import sys

try:  # optional: named-attribute decoding via schema (*vir.dat) typedefs
    from e3d_attr_decoder import SchemaSet, decode_element
except Exception:  # noqa: BLE001
    SchemaSet = None
    decode_element = None

BASE27_OFFSET = 0x81BF1
INDEX_NOUN = 0xCC47DF   # 索引页 noun 魔数 (IndexPageData.noun 断言)
DEFAULT_EXE_DIR = r"D:\AVEVA\Everything3D2.10"   # %AVEVA_DESIGN_EXE% (schema *vir.dat)


def db1_dehash(h: int) -> str:
    """base-27 + 0x81BF1 可逆哈希反查 (core.dll rs-core 一致)。"""
    if h <= BASE27_OFFSET:
        return ""
    if h > 0x171FAD39:  # UDA 特例
        k = (h - 0x171FAD39) % 0x1000000
        s = ":"
        for _ in range(6):
            if k <= 0:
                break
            s += chr(k % 64 + 32)
            k //= 64
        return s
    k = h - BASE27_OFFSET
    s = ""
    while k > 0:
        d = k % 27
        s += " " if d == 0 else chr(d + 64)
        k //= 27
    return s


def looks_like_noun(h: int) -> bool:
    name = db1_dehash(h)
    return 1 <= len(name) <= 8 and all(c == " " or "A" <= c <= "Z" for c in name)


class E3DDb:
    def __init__(self, path: str):
        with open(path, "rb") as f:
            self.blob = f.read()
        self.parse_header()

    def u32(self, off: int) -> int:
        return struct.unpack_from(">I", self.blob, off)[0]

    def f64(self, off: int) -> float:
        return struct.unpack_from(">d", self.blob, off)[0]

    def parse_header(self):
        b = self.blob
        self.version = self.u32(0x04)
        self.db_num = self.u32(0x08)
        # 0x20 = schema/template type id(选定 *vir.dat),非创建时间;0x24 = schema 版本(见 §2 更正)
        self.schema_type_id = self.u32(0x20)
        self.schema_version = self.u32(0x24)
        self.latest_ses_pgno = self.u32(0x28)
        self.ext_no = self.u32(0x2C)
        self.extent_counter = self.u32(0x30)        # 0x30 = extract/extent 计数,非会话页号(见 §2)
        self.page_size = self.u32(0x34) * 4         # <-- 关键修正: *4
        if self.page_size not in (512, 2048, 4096):
            self.page_size = 2048
        # 0x38/0x3C = db 根引用 refno (dbno, refseq);非存储页计数/恒2(见 §2 / §2.1)
        self.dbno = self.u32(0x38)
        self.dbno_refseq = self.u32(0x3C)
        self.n_pages = len(b) // self.page_size
        # 向后兼容别名(旧误称字段名,保留以免破坏调用方)
        self.creation_time = self.schema_type_id
        self.session_page_no = self.extent_counter
        self.stored_page_count = self.dbno

    def page_off(self, pgno: int) -> int:
        return pgno * self.page_size

    def read_words(self, byte_off: int, n: int) -> list:
        """Read n big-endian 32-bit words from byte_off (bounded by blob)."""
        n = max(0, min(n, (len(self.blob) - byte_off) // 4))
        return list(struct.unpack_from(">%dI" % n, self.blob, byte_off)) if n else []

    def page_type(self, pgno: int) -> int:
        return self.u32(self.page_off(pgno))

    # ---- 会话页 ----
    def parse_session(self, pgno: int) -> dict:
        base = self.page_off(pgno)
        u = lambda i: self.u32(base + i)
        year, month, hours, seconds = u(0x34), u(0x38), u(0x3C), u(0x40)
        days = hours // 24
        hh = hours % 24
        mm = seconds // 60
        ss = seconds % 60
        nwl = u(0x78)
        name = b""
        if 0 < nwl <= 9:
            name = self.blob[base + 0x7C: base + 0x7C + nwl * 4].rstrip(b"\x00")
        return {
            "pgno": pgno,
            "page_type": u(0),
            "last_ses_pgno": u(0x04),
            "sesno": u(0x0C),
            "end_pgno": u(0x14),
            "index_root_pgno": u(0x1C),
            "claim_pgno": u(0x24),
            "date": f"{year:04d}-{month:02d}-{days:02d} {hh:02d}:{mm:02d}:{ss:02d}",
            "computer": name.decode("latin1", "ignore"),
        }

    def session_chain(self) -> list:
        out = []
        pg = self.latest_ses_pgno
        seen = set()
        while pg and pg not in seen and 0 < pg < self.n_pages:
            seen.add(pg)
            if self.page_type(pg) != 3:
                break
            s = self.parse_session(pg)
            out.append(s)
            pg = s["last_ses_pgno"]
        return out

    # ---- B-树索引枚举: refno -> (pgno, offset_words) ----
    def walk_index(self, root_pgno: int, max_entries=4_000_000) -> list:  # generous: large dbs (ams1112) exceed 300k leaves
        """枚举 B+ 树叶子条目 (refno -> 数据位置)。

        权威遍历 (对齐 db3_split_node/db3_change_table_entry 反编译):
          1. 条目数由页头 **word6**(空闲字数)界定 = (page_words-7-word6)/4;
             **不是空终止**(空终止会越读 word6 之后的脏槽)。
          2. 内部节点最左子页的分隔键是哨兵 0x80000001(= −∞),其子树是
             **最小键子树**(WORLD/SITE/ZONE… 低 refno 元素),必须**下降**。
        旧版"空终止 + 跳过哨兵"会**漏掉最左脊**(sam7200 约 38% 叶条目:
        6536→10392 主元素)且越读脏槽,二者部分相互掩盖。详见 findings §16。
        """
        result = []
        visited = set()
        pw = self.page_size // 4

        def is_index_page(pg):
            return 0 < pg < self.n_pages and self.u32(self.page_off(pg) + 4) == INDEX_NOUN

        def recurse(pg, depth=0):
            if pg in visited or depth > 40 or len(result) >= max_entries:
                return
            visited.add(pg)
            if not is_index_page(pg):
                return
            base = self.page_off(pg)
            # 条目自 word7(+0x1C)起, 每条 4 字 [r0,r1,pgno, off20|flag12];
            # word6 = 空闲字数 -> 有效条目数 = (pw-7-word6)/4
            nent = (pw - 7 - self.u32(base + 24)) // 4
            for k in range(nent):
                if len(result) >= max_entries:
                    break
                wo = base + (7 + 4 * k) * 4
                r0 = self.u32(wo)
                r1 = self.u32(wo + 4)
                cpg = self.u32(wo + 8)
                off = self.u32(wo + 12) >> 12
                if off == 0 and is_index_page(cpg):
                    recurse(cpg, depth + 1)   # 内部节点 -> 子索引页(含哨兵最左子页)
                elif (r0, r1) != (0x80000001, 0x80000001):
                    result.append((r0, r1, cpg, off))  # 叶子 -> 元素位置

        recurse(root_pgno)
        return result

    # ---- 元素记录 ----
    def parse_element(self, byte_off: int) -> dict:
        u = lambda i: self.u32(byte_off + i)
        impl = u(0) & 0xFFFF
        noun = u(0x0C)
        ele = {
            "impl_words": impl,
            "refno": (u(0x04), u(0x08)),
            "noun_hash": noun,
            "noun": db1_dehash(noun),
            "owner": (u(0x10), u(0x14)),
            "page_no": u(0x18),
            "pos": None,
        }
        # 隐式区中常见: count=3 标记后跟 3 个 double = POS
        # 启发式扫描 [0..impl-6) 找连续 3 个 "合理坐标" double
        for i in range(7, max(7, impl - 5)):
            o = byte_off + i * 4
            if o + 24 > len(self.blob):
                break
            try:
                xs = [self.f64(o), self.f64(o + 8), self.f64(o + 16)]
            except struct.error:
                break
            if all(abs(x) < 1e7 and (x == 0 or abs(x) > 1e-3) for x in xs) and any(x != 0 for x in xs):
                ele["pos"] = tuple(round(x, 3) for x in xs)
                break
        return ele


def main():
    args = sys.argv[1:]
    json_out = None
    if "--json" in args:
        i = args.index("--json")
        json_out = args[i + 1] if i + 1 < len(args) else "e3d_dump.json"
        del args[i:i + 2]
    attrs = False
    exe_dir = DEFAULT_EXE_DIR
    if "--exe" in args:
        i = args.index("--exe")
        exe_dir = args[i + 1]
        del args[i:i + 2]
    if "--attrs" in args:
        attrs = True
        args.remove("--attrs")
    if not args:
        print(__doc__)
        return
    path = args[0]
    max_el = int(args[1]) if len(args) > 1 else 10
    db = E3DDb(path)

    print(f"== 文件头 ==")
    print(f"  version={db.version} db_num={db.db_num} ext_no={db.ext_no}")
    print(f"  page_size={db.page_size} (header[0x34]*4)  n_pages={db.n_pages}")
    print(f"  latest_ses_pgno={db.latest_ses_pgno}")
    print(f"  dbno(0x38)=0x{db.dbno:X} refseq(0x3C)={db.dbno_refseq}  schema_type_id(0x20)=0x{db.schema_type_id:X}  extent_counter(0x30)={db.extent_counter}")

    print(f"\n== 页类型直方图 ==")
    hist = {}
    for pg in range(db.n_pages):
        t = db.page_type(pg)
        # 一级类型 1/3/5/7/8; 其它归为数据页魔数
        key = t if t in (0, 1, 3, 5, 7, 8) else "magic"
        hist[key] = hist.get(key, 0) + 1
    for k in sorted(hist, key=str):
        print(f"  type {k}: {hist[k]}")

    print(f"\n== 会话链 (最新 -> 最旧, 最多 8) ==")
    chain = db.session_chain()
    for s in chain[:8]:
        print(f"  pg{s['pgno']:>6} sesno={s['sesno']:<5} {s['date']}  user={s['computer']!r}  index_root={s['index_root_pgno']}")
    print(f"  (共 {len(chain)} 个会话)")

    if not chain:
        return
    root = chain[0]["index_root_pgno"]
    print(f"\n== 从索引根 {root} 枚举 refno (最多统计 100000) ==")
    entries = db.walk_index(root)
    leaf = [e for e in entries if e[3] != 0]
    print(f"  索引条目总数={len(entries)}, 其中叶子(指向元素)={len(leaf)}")

    from collections import Counter
    noun_count = Counter()
    sample = []
    all_elems = []
    for r0, r1, pgno, off in leaf:
        bo = pgno * db.page_size + off * 2
        if bo + 28 > len(db.blob):
            continue
        el = db.parse_element(bo)
        if not looks_like_noun(el["noun_hash"]):
            continue
        noun = el["noun"].strip()
        noun_count[noun] += 1
        if len(sample) < max_el:
            sample.append((r0, r1, el))
        if json_out:
            all_elems.append({
                "refno": f"{r0}/{r1}",
                "noun": noun,
                "noun_hash": el["noun_hash"],
                "owner": f"{el['owner'][0]}/{el['owner'][1]}",
                "page_no": el["page_no"],
                "pos": list(el["pos"]) if el["pos"] else None,
            })

    print(f"\n== 元素 noun 类型直方图 (Top 25) ==")
    for name, cnt in noun_count.most_common(25):
        print(f"  {name:<8} {cnt}")
    print(f"  (合计 {sum(noun_count.values())} 个可识别元素, {len(noun_count)} 种类型)")

    print(f"\n== 元素样本 (前 {max_el} 个) ==")
    for r0, r1, el in sample:
        pos = f" POS={el['pos']}" if el["pos"] else ""
        print(f"  refno=({r0:#x},{r1:#x}) noun={el['noun'].strip()!r:>8} owner=({el['owner'][0]:#x},{el['owner'][1]:#x}){pos}")

    if attrs:
        if SchemaSet is None:
            print("\n[--attrs] e3d_attr_decoder 不可用(确认同目录存在 e3d_attr_decoder.py)")
        elif not os.path.isdir(exe_dir):
            print(f"\n[--attrs] schema 目录不存在: {exe_dir}(用 --exe <dir> 指定)")
        else:
            ss = SchemaSet(exe_dir)
            primary = ss.schema_for_db(db.blob)
            print(f"\n== 命名属性解码 (schema={len(ss.schemas)} 库/{len(ss.noun2schema)} 类型; "
                  f"本库主 schema(由 header 0x20=0x{db.schema_type_id:X} 自动选定)={primary.name if primary else '?'}; "
                  f"每种 noun 取一个元素, 上限 {max_el}) ==")
            seen = set()
            for r0, r1, pgno, off in leaf:
                if len(seen) >= max_el:
                    break
                bo = pgno * db.page_size + off * 2
                if bo + 28 > len(db.blob):
                    continue
                el = db.parse_element(bo)
                noun = el["noun"].strip()
                if noun in seen or not looks_like_noun(el["noun_hash"]):
                    continue
                # record-validity: word0 must be a clean u16 implicit-word count.
                # Mis-located/secondary index entries have word0 = refno fragment
                # (e.g. 0x????5C20 -> impl 23584); skip them.
                if (db.u32(bo) >> 16) != 0 or not (8 <= el["impl_words"] <= 512):
                    continue
                W = db.read_words(bo, min(el["impl_words"], 256))
                if len(W) < 11:
                    continue
                r = decode_element(ss, W)
                if not r["schema"]:
                    continue
                seen.add(noun)
                shown = [a for a in r["attrs"] if a["value"] not in (None, [], [0], [0.0])]
                print(f"\n  [{noun}] refno=({r0:#x},{r1:#x}) schema={r['schema']} sel={r['sel']} 内联属性={len(r['attrs'])}")
                for a in shown[:14]:
                    print(f"    {(a['name'] or hex(a['hash'])):<6} t={a['type']:<2} = {a['value']}")

    if json_out:
        import json
        doc = {
            "file": path,
            "header": {
                "version": db.version, "db_num": db.db_num, "ext_no": db.ext_no,
                "page_size": db.page_size, "n_pages": db.n_pages,
                "latest_ses_pgno": db.latest_ses_pgno,
                "dbno": db.dbno, "dbno_refseq": db.dbno_refseq,
                "schema_type_id": db.schema_type_id, "extent_counter": db.extent_counter,
            },
            "sessions": [
                {"pgno": s["pgno"], "sesno": s["sesno"], "date": s["date"],
                 "computer": s["computer"], "index_root_pgno": s["index_root_pgno"]}
                for s in chain
            ],
            "noun_histogram": dict(noun_count.most_common()),
            "element_count": len(all_elems),
            "elements": all_elems,
        }
        with open(json_out, "w", encoding="utf-8") as f:
            json.dump(doc, f, ensure_ascii=False, indent=1)
        print(f"\n== 已导出 JSON: {json_out} ({len(all_elems)} 元素) ==")


if __name__ == "__main__":
    main()
