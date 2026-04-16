#!/usr/bin/env python3
"""
attlib.dat 二进制解析器 — 从二进制文件直接提取 NOUN 属性元数据

功能：
  1. 解析 attlib.dat 的 Page 1 目录页，定位各表起始页
  2. 解析 ATNAIN 表（NounHash → AttrIndex 映射）
  3. 解析 ATGTDF 表（属性 hash → DataType/DefiKind）
  4. 解析属性记录区（属性名称、描述等）
  5. 合并构建完整的 NOUN → [AttributeMeta] 映射

用法：
  python parse_attlib_binary.py attlib.dat                      # 全量解析
  python parse_attlib_binary.py attlib.dat --noun PIPE          # 查看 PIPE
  python parse_attlib_binary.py attlib.dat --noun ELBO --json   # JSON 输出
  python parse_attlib_binary.py attlib.dat --stats              # 统计
  python parse_attlib_binary.py attlib.dat --export out.json    # 导出全部
"""

import struct
import json
import argparse
import sys
from collections import defaultdict
from typing import List, Dict, Tuple, Optional

PAGE_SIZE = 2048
DELIMITER = 0xFFFFFFFF


def db1_hash(name: str) -> int:
    h = 0
    for ch in name.upper():
        h = (h * 26 + ord(ch) - 0x40) & 0xFFFFFFFF
    return h


def db1_dehash(h: int) -> str:
    chars = []
    val = h
    while val > 0:
        r = val % 26
        val = val // 26
        if r == 0:
            r = 26
            val -= 1
        chars.append(chr(r + 0x40))
    return ''.join(reversed(chars))


def read_page(data: bytes, page_num: int) -> List[int]:
    offset = page_num * PAGE_SIZE
    if offset + PAGE_SIZE > len(data):
        return []
    page_bytes = data[offset:offset + PAGE_SIZE]
    return list(struct.unpack(f'>{PAGE_SIZE // 4}I', page_bytes))


def read_directory(data: bytes) -> Dict[str, int]:
    """读取 Page 1 目录，返回各表起始页号
    
    Directory layout (observed from attlib.dat):
      [0] = 3 (unknown/version)
      [1] = attr_records_start
      [2] = atgtix_start (or first index table)
      [3] = atnain_start
      [4] = atgtdf_start (属性定义表)
      [5] = atgtsx_start (辅助表)
      [6] = page range 1
      [7] = page range 2
    """
    page = read_page(data, 1)
    if not page:
        return {}, []

    result = {
        'attr_records_start': page[1] if len(page) > 1 else 0,
        'atgtix_start': page[2] if len(page) > 2 else 0,
        'atnain_start': page[3] if len(page) > 3 else 0,
        'atgtdf_start': page[4] if len(page) > 4 else 0,
        'atgtsx_start': page[5] if len(page) > 5 else 0,
    }

    candidates = sorted(set(v for v in page[:8] if 0 < v < DELIMITER))
    return result, candidates


def collect_records(data: bytes, start_page: int, max_pages: int = 1500) -> List[List[int]]:
    """收集以 0xFFFFFFFF 分隔的属性记录"""
    records = []
    current = []
    found_first = False

    for page_idx in range(start_page, start_page + max_pages):
        page = read_page(data, page_idx)
        if not page:
            break

        for val in page:
            if val == DELIMITER:
                if found_first and current:
                    records.append(current)
                    current = []
                found_first = True
            elif found_first:
                current.append(val)

    if current:
        records.append(current)

    return records


def extract_string(vals: List[int], start: int) -> Tuple[Optional[str], int]:
    """从 u32 数组中提取字符串（长度前缀编码）"""
    if start >= len(vals):
        return None, start
    length = vals[start]
    if length > 500 or start + 1 + length > len(vals):
        return None, start + 1

    chars = []
    for code in vals[start + 1: start + 1 + length]:
        if 0x20 <= code <= 0x7E:
            chars.append(chr(code))
        else:
            chars.append('.')
    return ''.join(chars), start + 1 + length


def parse_attr_record(vals: List[int]) -> Optional[dict]:
    """解析单个属性记录"""
    idx = 0
    while idx < len(vals) and vals[idx] == 0:
        idx += 1
    if idx >= len(vals):
        return None

    attr_id = vals[idx]
    idx += 1

    name, idx = extract_string(vals, idx)
    if not name:
        return None

    type_code = 0
    if idx < len(vals):
        raw = vals[idx]
        type_code = raw if raw <= 20 else -1
        idx += 1

    strings = []
    while idx < len(vals):
        val = vals[idx]
        if 0 < val < 500:
            is_str = all(
                idx + 1 + k < len(vals) and 0x20 <= vals[idx + 1 + k] <= 0x7E
                for k in range(val)
            )
            if is_str:
                s, idx = extract_string(vals, idx)
                if s:
                    strings.append(s)
                    continue
        idx += 1

    return {
        'id': attr_id,
        'name': name,
        'type_code': type_code,
        'description': strings[0] if strings else '',
        'short_name': strings[1] if len(strings) >= 2 else '',
        'ui_name': strings[2] if len(strings) >= 3 else '',
        'category': strings[-1] if len(strings) >= 5 else '',
    }


DATA_TYPE_NAMES = {
    1: 'Integer', 2: 'Real', 3: 'Boolean', 4: 'Reference',
    5: 'Text', 6: 'Enum', 7: 'Position', 8: 'Direction',
    9: 'Orientation', 10: 'IntArray', 11: 'RealArray', 12: 'RefArray',
}

DEFI_NAMES = {1: 'DAB', 4: 'Pseudo'}

UNIT_NAMES = {
    1: 'Dimensionless', 2: 'Distance/Area/Bore',
    3: 'Temperature/Pressure', 4: 'Volume', 5: 'Angle', 6: 'Mass',
}


def guess_table_start(data: bytes, candidates: List[int],
                       parser_fn, min_count: int = 5) -> Optional[int]:
    """猜测某个表的起始页"""
    for p in candidates:
        try:
            result = parser_fn(data, p, 32)
            if len(result) >= min_count:
                return p
        except:
            pass
    return None


def parse_atnain(data: bytes, start_page: int, max_pages: int = 30) -> List[dict]:
    """解析 ATNAIN: [NounHash, AttrIndex, TypeCode] 三元组"""
    entries = []
    for page_idx in range(start_page, start_page + max_pages):
        page = read_page(data, page_idx)
        if not page:
            break
        for i in range(0, len(page) - 2, 3):
            nh = page[i]
            ai = page[i + 1]
            tc = page[i + 2]
            if nh == 0 or nh == DELIMITER:
                continue
            if ai == 0 or ai == DELIMITER:
                continue
            entries.append({'noun_hash': nh, 'attr_index': ai, 'type_code': tc})
    return entries


def parse_atgtdf(data: bytes, start_page: int, max_entries: int = 8192) -> List[dict]:
    """解析 ATGTDF: [AttrHash, DataType, DefiKind(, ext...)]"""
    entries = []
    page_idx = start_page
    while len(entries) < max_entries:
        page = read_page(data, page_idx)
        if not page:
            break
        i = 0
        while i + 2 < len(page):
            w0 = page[i]
            if w0 == 0:
                break
            if w0 == DELIMITER:
                return entries
            if w0 < 531_442 or w0 > 387_951_929:
                break
            w1 = page[i + 1]
            kind = page[i + 2]
            i += 3
            ext = 0
            if kind == 2:
                if w1 == 4 and i < len(page):
                    n = page[i]
                    i += 1 + n
                elif i < len(page):
                    ext = page[i]
                    i += 1
            entries.append({
                'hash': w0, 'data_type': w1, 'defi': kind, 'ext': ext
            })
            if len(entries) >= max_entries:
                break
        page_idx += 1
    return entries


def parse_atgtsx(data: bytes, start_page: int, max_entries: int = 8192) -> List[dict]:
    """解析 ATGTSX: [Key, V1, V2] 三元组"""
    entries = []
    page_idx = start_page
    while len(entries) < max_entries:
        page = read_page(data, page_idx)
        if not page:
            break
        for i in range(0, len(page) - 2, 3):
            w0 = page[i]
            if w0 == 0:
                break
            if w0 == DELIMITER:
                return entries
            entries.append({'key': w0, 'v1': page[i + 1], 'v2': page[i + 2]})
            if len(entries) >= max_entries:
                break
        page_idx += 1
    return entries


def parse_atgtix(data: bytes, start_page: int, max_entries: int = 8192) -> List[dict]:
    """解析 ATGTIX: [Code, Displacement] 二元组"""
    entries = []
    page_idx = start_page
    while len(entries) < max_entries:
        page = read_page(data, page_idx)
        if not page:
            break
        for i in range(0, len(page) - 1, 2):
            w0 = page[i]
            if w0 == 0:
                break
            if w0 == DELIMITER:
                return entries
            if w0 < 531_442 or w0 > 387_951_929:
                break
            disp = page[i + 1]
            entries.append({
                'code': w0, 'page': disp // 512, 'offset': disp % 512
            })
            if len(entries) >= max_entries:
                break
        page_idx += 1
    return entries


def build_full_metadata(attr_records, atnain_entries, atgtdf_entries):
    """构建完整的 NOUN → 属性元数据映射"""

    # 属性 hash → ATGTDF 条目
    atgtdf_map = {}
    for e in atgtdf_entries:
        atgtdf_map[e['hash']] = e

    # 属性名称 → hash
    attr_by_index = {}
    attr_by_name = {}
    for i, rec in enumerate(attr_records):
        h = db1_hash(rec['name'])
        attr_by_index[i + 1] = rec  # ATNAIN 的 attr_index 是 1-based
        attr_by_name[rec['name']] = (i + 1, h, rec)

    # NOUN hash → 属性列表
    noun_attrs = defaultdict(list)
    for entry in atnain_entries:
        nh = entry['noun_hash']
        ai = entry['attr_index']
        tc = entry['type_code']

        rec = attr_by_index.get(ai)
        if not rec:
            continue

        attr_hash = db1_hash(rec['name'])
        df = atgtdf_map.get(attr_hash, {})

        data_type_code = df.get('data_type', 0)
        defi_code = df.get('defi', 0)
        ext = df.get('ext', 0)

        meta = {
            'name': rec['name'],
            'hash': attr_hash,
            'attr_index': ai,
            'data_type': DATA_TYPE_NAMES.get(data_type_code, f'Unknown({data_type_code})'),
            'data_type_code': data_type_code,
            'defi': DEFI_NAMES.get(defi_code, f'Unknown({defi_code})'),
            'defi_code': defi_code,
            'size': ext if ext > 0 else 1,
            'unit_type': UNIT_NAMES.get(rec['type_code'], f'Unknown({rec["type_code"]})'),
            'description': rec['description'],
            'category': rec['category'],
            'atnain_type_code': tc,
        }
        noun_attrs[nh].append(meta)

    return noun_attrs


def print_noun_attrs(noun_attrs, noun_name: str, show_all: bool = True):
    """打印某个 NOUN 的属性列表"""
    nh = db1_hash(noun_name)
    attrs = noun_attrs.get(nh, [])
    if not attrs:
        print(f'NOUN "{noun_name}" (hash=0x{nh:08X}) not found in ATNAIN.')
        known = sorted(set(db1_dehash(h) for h in noun_attrs.keys()))
        print(f'Known NOUNs ({len(known)}): {", ".join(known[:30])} ...')
        return

    print(f'=== {noun_name} (hash=0x{nh:08X}) — {len(attrs)} attributes ===\n')

    dab = [a for a in attrs if a['defi'] == 'DAB']
    pseudo = [a for a in attrs if a['defi'] != 'DAB']

    print(f'DAB Attributes ({len(dab)}):')
    print(f'  {"Name":<12s} {"DataType":<14s} {"Defi":<8s} {"Size":<5s} {"Unit":<22s} {"Hash":<12s} {"Desc"}')
    print(f'  {"-"*12} {"-"*14} {"-"*8} {"-"*5} {"-"*22} {"-"*12} {"-"*20}')
    for a in sorted(dab, key=lambda x: x['name']):
        desc = a['description'][:25] + '...' if len(a['description']) > 25 else a['description']
        print(f'  {a["name"]:<12s} {a["data_type"]:<14s} {a["defi"]:<8s} {a["size"]:<5d} {a["unit_type"]:<22s} 0x{a["hash"]:08X}  {desc}')

    if show_all and pseudo:
        print(f'\nPseudo/Other ({len(pseudo)}):')
        for a in sorted(pseudo, key=lambda x: x['name']):
            desc = a['description'][:25] if a['description'] else ''
            print(f'  {a["name"]:<12s} {a["data_type"]:<14s} {a["defi"]:<8s} 0x{a["hash"]:08X}  {desc}')


def export_json(noun_attrs, attr_records, output_path: str):
    """导出为 JSON"""
    result = {
        'source': 'attlib.dat binary parser',
        'attr_count': len(attr_records),
        'noun_count': len(noun_attrs),
        'nouns': {}
    }
    for nh, attrs in sorted(noun_attrs.items()):
        noun_name = db1_dehash(nh)
        result['nouns'][noun_name] = {
            'hash': nh,
            'hash_hex': f'0x{nh:08X}',
            'attributes': {a['name']: a for a in attrs}
        }

    with open(output_path, 'w', encoding='utf-8') as f:
        json.dump(result, f, indent=2, ensure_ascii=False)
    print(f'Exported to {output_path}')
    print(f'  {len(attr_records)} attributes, {len(noun_attrs)} NOUNs')


def main():
    parser = argparse.ArgumentParser(
        description='attlib.dat Binary Parser — Extract NOUN Attribute Metadata')
    parser.add_argument('attlib', help='Path to attlib.dat')
    parser.add_argument('--noun', type=str, help='Show attributes for a specific NOUN')
    parser.add_argument('--stats', action='store_true', help='Show parsing statistics')
    parser.add_argument('--export', type=str, metavar='FILE', help='Export all data to JSON')
    parser.add_argument('--json', action='store_true', help='Output in JSON format')
    parser.add_argument('--dab-only', action='store_true', help='Show only DAB attributes')
    parser.add_argument('--dir', action='store_true', help='Show directory page contents')
    args = parser.parse_args()

    with open(args.attlib, 'rb') as f:
        data = f.read()

    print(f'File: {args.attlib} ({len(data)} bytes, {len(data) // PAGE_SIZE} pages)\n')

    # 1. Read directory
    dir_info, candidates = read_directory(data)
    attr_start = dir_info['attr_records_start']
    atnain_start = dir_info['atnain_start']

    if args.dir:
        print(f'Directory Page 1:')
        print(f'  Attr records start: page {attr_start}')
        print(f'  ATNAIN start: page {atnain_start}')
        print(f'  Candidate pages: {candidates}')
        print()

    # 2. Parse tables
    print('Parsing tables...')
    attr_records = []
    if attr_start > 0:
        raw = collect_records(data, attr_start)
        for rec_data in raw:
            parsed = parse_attr_record(rec_data)
            if parsed:
                attr_records.append(parsed)
    print(f'  Attribute records: {len(attr_records)}')

    atnain = []
    if atnain_start > 0:
        atnain = parse_atnain(data, atnain_start)
    print(f'  ATNAIN entries: {len(atnain)}')

    atgtdf_start = dir_info.get('atgtdf_start', 0)
    atgtdf = parse_atgtdf(data, atgtdf_start) if atgtdf_start > 0 else []
    print(f'  ATGTDF entries: {len(atgtdf)} (start page: {atgtdf_start})')

    atgtsx_start = dir_info.get('atgtsx_start', 0)
    atgtsx = parse_atgtsx(data, atgtsx_start) if atgtsx_start > 0 else []
    print(f'  ATGTSX entries: {len(atgtsx)} (start page: {atgtsx_start})')

    atgtix_start = dir_info.get('atgtix_start', 0)
    atgtix = parse_atgtix(data, atgtix_start) if atgtix_start > 0 else []
    print(f'  ATGTIX entries: {len(atgtix)} (start page: {atgtix_start})')

    # 3. Build metadata
    noun_attrs = build_full_metadata(attr_records, atnain, atgtdf)
    noun_names = sorted(set(db1_dehash(h) for h in noun_attrs.keys()))
    print(f'\n  Total NOUNs with attributes: {len(noun_attrs)}')
    print()

    # 4. Stats
    if args.stats:
        print(f'=== Statistics ===')
        print(f'File size: {len(data):,} bytes')
        print(f'Total pages: {len(data) // PAGE_SIZE}')
        print(f'Attribute definitions: {len(attr_records)}')
        print(f'ATNAIN entries: {len(atnain)}')
        print(f'ATGTDF entries: {len(atgtdf)}')
        print(f'ATGTSX entries: {len(atgtsx)}')
        print(f'ATGTIX entries: {len(atgtix)}')
        print(f'NOUNs: {len(noun_attrs)}')

        # Type distribution
        types = defaultdict(int)
        defis = defaultdict(int)
        for attrs in noun_attrs.values():
            for a in attrs:
                types[a['data_type']] += 1
                defis[a['defi']] += 1

        total_attrs = sum(len(v) for v in noun_attrs.values())
        print(f'Total NOUN-attribute pairs: {total_attrs}')
        print(f'\nBy data type:')
        for t, c in sorted(types.items(), key=lambda x: -x[1]):
            print(f'  {t:<16s} {c}')
        print(f'\nBy storage:')
        for d, c in sorted(defis.items(), key=lambda x: -x[1]):
            print(f'  {d:<16s} {c}')

        print(f'\nTop 20 NOUNs by attribute count:')
        top = sorted(noun_attrs.items(), key=lambda x: -len(x[1]))[:20]
        for nh, attrs in top:
            name = db1_dehash(nh)
            dab_count = sum(1 for a in attrs if a['defi'] == 'DAB')
            print(f'  {name:<12s} {len(attrs):3d} attrs ({dab_count:2d} DAB)')
        return

    # 5. Noun query
    if args.noun:
        noun = args.noun.upper()
        if args.json:
            nh = db1_hash(noun)
            attrs = noun_attrs.get(nh, [])
            print(json.dumps({
                'noun': noun,
                'hash': nh,
                'hash_hex': f'0x{nh:08X}',
                'attribute_count': len(attrs),
                'attributes': attrs
            }, indent=2, ensure_ascii=False))
        else:
            print_noun_attrs(noun_attrs, noun, show_all=not args.dab_only)
        return

    # 6. Export
    if args.export:
        export_json(noun_attrs, attr_records, args.export)
        return

    # Default: show summary
    print('Known NOUNs:')
    for i, name in enumerate(noun_names):
        nh = db1_hash(name)
        attrs = noun_attrs[nh]
        dab = sum(1 for a in attrs if a['defi'] == 'DAB')
        print(f'  {name:<12s} {len(attrs):3d} attrs ({dab:2d} DAB)  hash=0x{nh:08X}')
        if i >= 50:
            print(f'  ... ({len(noun_names) - 51} more)')
            break


if __name__ == '__main__':
    main()
