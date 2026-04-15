#!/usr/bin/env python3
"""
E3D/PDMS NOUN 属性元数据解析工具

功能：
  1. 从 all_attr_info.json 加载 NOUN 属性元数据
  2. 解析元素记录二进制的隐式区
  3. 输出每个属性的名称、类型、偏移和值

用法：
  python parse_noun_attrs.py --noun PIPE              # 显示 PIPE 的属性元数据
  python parse_noun_attrs.py --noun ELBO --dab-only   # 仅显示 DAB 属性（有 offset）
  python parse_noun_attrs.py --list                   # 列出所有已知 NOUN
  python parse_noun_attrs.py --parse record.bin PIPE   # 解析二进制记录
"""

import json
import struct
import argparse
import os
import sys


def db1_hash(name: str) -> int:
    h = 0
    for ch in name.upper():
        h = (h * 26 + ord(ch) - 0x40) & 0xFFFFFFFF
    return h


def db1_dehash(h: int) -> str:
    chars = []
    while h > 0:
        r = h % 26
        h = h // 26
        if r == 0:
            r = 26
            h -= 1
        chars.append(chr(r + 0x40))
    return ''.join(reversed(chars))


def decode_offset(raw_offset: int):
    """解码复合 offset → (word_offset, bit_index)"""
    if raw_offset == 0:
        return (0, -1)  # Pseudo
    if raw_offset < 0x100000:
        return (raw_offset, 0)  # 直接 word offset
    bit_index = raw_offset >> 20
    word_offset = raw_offset & 0xFFFFF
    return (word_offset, bit_index)


ATTR_SIZE_WORDS = {
    'INTEGER': 1, 'WORD': 1, 'BOOL': 0,  # BOOL 位打包
    'DOUBLE': 2, 'ELEMENT': 2,
    'POSITION': 6, 'ORIENTATION': 9, 'DIRECTION': 3,
    'STRING': 0, 'INTVEC': 0, 'RefU64Vec': 0,
}


class NounAttrInfo:
    def __init__(self, json_path: str):
        with open(json_path, 'r', encoding='utf-8') as f:
            data = json.load(f)
        self.noun_map = data.get('noun_attr_info_map', {})
        self.named_map = data.get('named_attr_info_map', {})

    def get_noun(self, noun_name: str) -> dict:
        return self.named_map.get(noun_name.upper(), {})

    def list_nouns(self):
        return sorted(self.named_map.keys())

    def get_dab_attrs(self, noun_name: str):
        """获取有 offset 的 DAB 属性，按 word_offset 排序"""
        attrs = self.get_noun(noun_name)
        result = []
        for a in attrs.values():
            if a['offset'] > 0:
                word_off, bit_idx = decode_offset(a['offset'])
                result.append({
                    'name': a['name'],
                    'hash': a['hash'],
                    'word_offset': word_off,
                    'bit_index': bit_idx,
                    'att_type': a['att_type'],
                    'raw_offset': a['offset'],
                    'default_val': a['default_val'],
                    'size_words': ATTR_SIZE_WORDS.get(a['att_type'], 0),
                })
        result.sort(key=lambda x: (x['word_offset'], x['bit_index']))
        return result

    def get_pseudo_attrs(self, noun_name: str):
        """获取 Pseudo 属性（offset=0）"""
        attrs = self.get_noun(noun_name)
        return sorted(
            [a for a in attrs.values() if a['offset'] == 0],
            key=lambda x: x['name']
        )


def parse_implicit_area(data: bytes, dab_attrs: list) -> dict:
    """解析隐式区二进制数据，提取 DAB 属性值"""
    results = {}

    for attr in dab_attrs:
        word_off = attr['word_offset']
        byte_off = word_off * 4
        att_type = attr['att_type']
        name = attr['name']
        bit_idx = attr['bit_index']

        if byte_off >= len(data):
            results[name] = {'error': f'offset {word_off} beyond data length'}
            continue

        try:
            if att_type == 'INTEGER' or att_type == 'WORD':
                val = struct.unpack_from('>i', data, byte_off)[0]
                results[name] = val

            elif att_type == 'DOUBLE':
                if byte_off + 8 <= len(data):
                    val = struct.unpack_from('>d', data, byte_off)[0]
                    results[name] = val
                else:
                    results[name] = {'error': 'insufficient data for DOUBLE'}

            elif att_type == 'BOOL':
                word_val = struct.unpack_from('>I', data, byte_off)[0]
                if bit_idx > 0:
                    results[name] = bool((word_val >> bit_idx) & 1)
                else:
                    results[name] = bool(word_val & 1)

            elif att_type == 'ELEMENT':
                if byte_off + 8 <= len(data):
                    ref0 = struct.unpack_from('>I', data, byte_off)[0]
                    ref1 = struct.unpack_from('>I', data, byte_off + 4)[0]
                    results[name] = f'RefNo({ref0}/{ref1})'
                else:
                    results[name] = {'error': 'insufficient data for ELEMENT'}

            elif att_type == 'POSITION':
                if byte_off + 24 <= len(data):
                    x, y, z = struct.unpack_from('>ddd', data, byte_off)
                    results[name] = {'x': x, 'y': y, 'z': z}
                else:
                    results[name] = {'error': 'insufficient data for POSITION'}

            elif att_type == 'ORIENTATION':
                if byte_off + 36 <= len(data):
                    vals = struct.unpack_from('>9d', data, byte_off)
                    results[name] = list(vals)
                else:
                    results[name] = {'error': 'insufficient data for ORIENTATION'}

            elif att_type == 'DIRECTION':
                if byte_off + 12 <= len(data):
                    x, y, z = struct.unpack_from('>ddd', data, byte_off)[:3]
                    results[name] = {'x': x, 'y': y, 'z': z}
                else:
                    results[name] = {'error': 'insufficient data for DIRECTION'}

            else:
                results[name] = f'<unsupported type: {att_type}>'

        except Exception as e:
            results[name] = {'error': str(e)}

    return results


def print_noun_schema(info: NounAttrInfo, noun_name: str, dab_only: bool = False):
    """打印 NOUN 的属性 schema"""
    attrs = info.get_noun(noun_name)
    if not attrs:
        print(f'NOUN "{noun_name}" not found in metadata.')
        print(f'Available NOUNs: {", ".join(info.list_nouns()[:20])} ...')
        return

    noun_hash = db1_hash(noun_name)
    dab_attrs = info.get_dab_attrs(noun_name)
    pseudo_attrs = info.get_pseudo_attrs(noun_name)

    print(f'=== {noun_name} (hash=0x{noun_hash:08X}, {len(attrs)} attributes) ===')
    print()

    print(f'DAB Attributes ({len(dab_attrs)} attrs, stored in implicit area):')
    print(f'  {"Location":<18s} {"Name":<12s} {"Type":<14s} {"Hash":<12s} {"Default"}')
    print(f'  {"-"*18} {"-"*12} {"-"*14} {"-"*12} {"-"*20}')

    for a in dab_attrs:
        loc = f'word[{a["word_offset"]}]'
        if a['bit_index'] > 0:
            loc += f' bit{a["bit_index"]}'
        dv = str(list(a['default_val'].values())[0]) if a['default_val'] else ''
        if len(dv) > 20:
            dv = dv[:17] + '...'
        print(f'  {loc:<18s} {a["name"]:<12s} {a["att_type"]:<14s} 0x{a["hash"]:08X}  {dv}')

    if not dab_only:
        print()
        print(f'Pseudo Attributes ({len(pseudo_attrs)} attrs, computed/external):')
        for a in pseudo_attrs:
            dv = str(list(a['default_val'].values())[0]) if a.get('default_val') else ''
            if len(dv) > 25:
                dv = dv[:22] + '...'
            print(f'  {a["name"]:<12s} {a["att_type"]:<14s} 0x{a["hash"]:08X}  {dv}')


def main():
    parser = argparse.ArgumentParser(description='E3D NOUN Attribute Metadata Parser')
    parser.add_argument('--json', default=None,
                        help='Path to all_attr_info.json')
    parser.add_argument('--noun', type=str, help='NOUN name to inspect (e.g. PIPE, ELBO)')
    parser.add_argument('--dab-only', action='store_true', help='Show only DAB attributes')
    parser.add_argument('--list', action='store_true', help='List all known NOUNs')
    parser.add_argument('--parse', nargs=2, metavar=('BINFILE', 'NOUN'),
                        help='Parse a binary implicit area dump')
    parser.add_argument('--hash', type=str, help='Compute db1_hash for a name')
    parser.add_argument('--dehash', type=str, help='Reverse db1_hash (hex or decimal)')
    args = parser.parse_args()

    if args.hash:
        name = args.hash.upper()
        h = db1_hash(name)
        print(f'{name} → 0x{h:08X} ({h})')
        return

    if args.dehash:
        h = int(args.dehash, 16) if args.dehash.startswith('0x') else int(args.dehash)
        name = db1_dehash(h)
        print(f'0x{h:08X} ({h}) → {name}')
        return

    json_path = args.json
    if not json_path:
        candidates = [
            os.path.join(os.path.dirname(__file__), '..', '..', '..', 'rs-core', 'all_attr_info.json'),
            'D:/work/plant-code/rs-core/all_attr_info.json',
            './all_attr_info.json',
        ]
        for c in candidates:
            if os.path.exists(c):
                json_path = c
                break
    if not json_path or not os.path.exists(json_path):
        print('Error: all_attr_info.json not found. Use --json to specify path.')
        sys.exit(1)

    info = NounAttrInfo(json_path)

    if args.list:
        nouns = info.list_nouns()
        print(f'Known NOUNs ({len(nouns)}):')
        for i, n in enumerate(nouns):
            attrs = info.get_noun(n)
            dab = sum(1 for a in attrs.values() if a['offset'] > 0)
            print(f'  {n:<12s}  {len(attrs):3d} attrs ({dab:2d} DAB)   hash=0x{db1_hash(n):08X}')
        return

    if args.noun:
        print_noun_schema(info, args.noun.upper(), args.dab_only)
        return

    if args.parse:
        bin_file, noun_name = args.parse
        noun_name = noun_name.upper()
        dab_attrs = info.get_dab_attrs(noun_name)
        if not dab_attrs:
            print(f'No DAB attributes found for {noun_name}')
            sys.exit(1)

        with open(bin_file, 'rb') as f:
            data = f.read()

        print(f'Parsing {len(data)} bytes as {noun_name} implicit area...')
        results = parse_implicit_area(data, dab_attrs)
        print(json.dumps(results, indent=2, ensure_ascii=False))
        return

    parser.print_help()


if __name__ == '__main__':
    main()
