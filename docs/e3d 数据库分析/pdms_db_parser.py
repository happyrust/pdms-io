#!/usr/bin/env python3
"""
PDMS 数据库文件解析器

基于 IDA Pro 逆向分析 db1-db5 模块编写。

用法:
    python pdms_db_parser.py <database_file>
"""

import struct
import sys
import os
from dataclasses import dataclass
from typing import List, Optional, Dict, Any


@dataclass
class PageHeader:
    """页面头部结构"""
    page_type: int      # 页面类型 (1,3,5,7,8)
    type_id: int        # 类型标识
    db_handle: int      # 数据库标记
    ext_no: int         # 扩展号
    page_no: int        # 页面号
    bucket_id: int      # 桶ID


@dataclass  
class DatabaseHeader:
    """数据库描述符 (Page 0)"""
    version: int
    db_id: int
    ext_no: int
    page_size: int
    page_count: int
    session_page: int
    creator_info: str


PAGE_TYPE_NAMES = {
    0: "描述符页",
    1: "引用数组页",
    3: "会话页",
    5: "数据页",
    7: "特殊页",
    8: "索引页"
}

DATA_SUBTYPES = {
    0x743F11: "主要数据 (7618377)",
    0x743F49: "主要数据变体",
    0xCC5D1F: "辅助数据 (13387743)",
    0xCC47DF: "辅助数据变体",
    0x5256C75: "索引数据 (86284645)",
    0x3C0A13F: "属性数据 (63068511)",
    0x3F22C60: "扩展数据 (66156832)"
}


class PDMSDatabase:
    """PDMS 数据库解析器"""
    
    def __init__(self, file_path: str):
        self.file_path = file_path
        self.file_size = os.path.getsize(file_path)
        self.header: Optional[DatabaseHeader] = None
        self.page_size = 0x800  # 默认 2KB
        self._parse_header()
    
    def _read_bytes(self, offset: int, size: int) -> bytes:
        """读取指定偏移的字节"""
        with open(self.file_path, 'rb') as f:
            f.seek(offset)
            return f.read(size)
    
    def _read_u32_be(self, data: bytes, offset: int) -> int:
        """读取大端序 32 位整数"""
        return struct.unpack('>I', data[offset:offset+4])[0]
    
    def _read_i32_be(self, data: bytes, offset: int) -> int:
        """读取大端序 32 位有符号整数"""
        return struct.unpack('>i', data[offset:offset+4])[0]
    
    def _parse_header(self) -> None:
        """解析数据库头部 (Page 0)"""
        data = self._read_bytes(0, 0x200)
        
        # 从 Page 0 解析基本信息
        version = self._read_u32_be(data, 0x04)
        db_id = self._read_u32_be(data, 0x08)
        ext_no = self._read_u32_be(data, 0x0C)
        
        # 获取页面大小和数量 - 从偏移 0x30-0x3C 附近
        page_size = self._read_u32_be(data, 0x34)
        page_count = self._read_u32_be(data, 0x38)
        session_page = self._read_u32_be(data, 0x30)
        
        # PDMS 数据库固定使用 2KB 页面大小
        # 头部字段的 page_size 通常为 0 或无效值
        page_size = 0x800  # 固定 2KB
        page_count = self.file_size // page_size
        
        self.page_size = page_size
        
        # 读取创建者信息 (偏移 0x40-0xF0)
        creator_bytes = data[0x40:0xF0]
        creator_info = creator_bytes.decode('ascii', errors='replace').strip('\x00').strip()
        
        self.header = DatabaseHeader(
            version=version,
            db_id=db_id,
            ext_no=ext_no,
            page_size=page_size,
            page_count=page_count,
            session_page=session_page,
            creator_info=creator_info
        )
    
    def read_page(self, page_num: int) -> bytes:
        """读取指定页面"""
        offset = page_num * self.page_size
        return self._read_bytes(offset, self.page_size)
    
    def parse_page_header(self, page_data: bytes) -> PageHeader:
        """解析页面头部"""
        page_type = self._read_u32_be(page_data, 0)
        type_id = self._read_u32_be(page_data, 4)
        db_handle = self._read_u32_be(page_data, 8)
        ext_no = self._read_u32_be(page_data, 12)
        page_no = self._read_u32_be(page_data, 16)
        bucket_id = (type_id >> 13) & 0x1FFF
        
        return PageHeader(
            page_type=page_type,
            type_id=type_id,
            db_handle=db_handle,
            ext_no=ext_no,
            page_no=page_no,
            bucket_id=bucket_id
        )
    
    def parse_session_page(self, page_data: bytes) -> Dict[str, Any]:
        """解析会话页 (type 3)"""
        result = {
            'page_type': 3,
            'session_mark': self._read_i32_be(page_data, 4),
            'unknown1': self._read_i32_be(page_data, 8),
            'session_count': self._read_u32_be(page_data, 12),
            'prev_session': self._read_i32_be(page_data, 16),
            'next_page': self._read_u32_be(page_data, 20),
        }
        
        # 读取用户名 (偏移 0x74)
        username_bytes = page_data[0x74:0x94]
        result['username'] = username_bytes.decode('ascii', errors='replace').strip('\x00').strip()
        
        return result
    
    def parse_data_page(self, page_data: bytes) -> Dict[str, Any]:
        """解析数据页 (type 5)"""
        header = self.parse_page_header(page_data)
        
        subtype_name = DATA_SUBTYPES.get(header.type_id, f"未知子类型 (0x{header.type_id:X})")
        
        result = {
            'page_type': 5,
            'type_id': header.type_id,
            'type_id_hex': f"0x{header.type_id:08X}",
            'subtype_name': subtype_name,
            'bucket_id': header.bucket_id,
            'db_handle': header.db_handle,
            'ext_no': header.ext_no,
            'page_no': header.page_no,
        }
        
        # 读取数据区域的一些关键字段
        result['data_field1'] = self._read_u32_be(page_data, 20)
        result['data_field2'] = self._read_u32_be(page_data, 24)
        
        return result
    
    def parse_special_page(self, page_data: bytes) -> Dict[str, Any]:
        """解析特殊页 (type 7)"""
        result = {
            'page_type': 7,
            'flags': self._read_u32_be(page_data, 4),
            'db_handle': self._read_u32_be(page_data, 8),
            'ext_no': self._read_u32_be(page_data, 12),
            'ref_count': self._read_u32_be(page_data, 16),
        }
        return result
    
    def scan_all_pages(self) -> List[Dict[str, Any]]:
        """扫描所有页面"""
        pages = []
        total_pages = self.file_size // self.page_size
        
        for page_num in range(total_pages):
            page_data = self.read_page(page_num)
            page_type = self._read_u32_be(page_data, 0)
            
            page_info = {
                'page_num': page_num,
                'offset': f"0x{page_num * self.page_size:08X}",
                'page_type': page_type,
                'type_name': PAGE_TYPE_NAMES.get(page_type, f"未知({page_type})")
            }
            
            # 根据页面类型解析详细信息
            if page_type == 3:
                page_info.update(self.parse_session_page(page_data))
            elif page_type == 5:
                page_info.update(self.parse_data_page(page_data))
            elif page_type == 7:
                page_info.update(self.parse_special_page(page_data))
            elif page_type == 1:
                # 引用数组页
                page_info['array_count'] = self._read_u32_be(page_data, 4)
            
            pages.append(page_info)
        
        return pages
    
    def print_info(self) -> None:
        """打印数据库信息"""
        print("=" * 80)
        print("PDMS 数据库文件分析")
        print("=" * 80)
        print(f"文件路径:      {self.file_path}")
        print(f"文件大小:      {self.file_size:,} 字节 ({self.file_size / 1024:.2f} KB)")
        print()
        print("-" * 40)
        print("数据库头部信息 (Page 0)")
        print("-" * 40)
        print(f"版本:          {self.header.version} (0x{self.header.version:08X})")
        print(f"数据库ID:      {self.header.db_id} (0x{self.header.db_id:08X})")
        print(f"扩展号:        {self.header.ext_no}")
        print(f"页面大小:      {self.header.page_size} 字节 (0x{self.header.page_size:X})")
        print(f"总页数:        {self.header.page_count}")
        print(f"会话页:        {self.header.session_page}")
        print()
        print(f"创建者信息:")
        print(f"  {self.header.creator_info}")
    
    def print_page_summary(self) -> None:
        """打印页面摘要"""
        pages = self.scan_all_pages()
        
        # 统计各类型页面数量
        type_counts: Dict[int, int] = {}
        for page in pages:
            pt = page['page_type']
            type_counts[pt] = type_counts.get(pt, 0) + 1
        
        print()
        print("-" * 40)
        print("页面类型统计")
        print("-" * 40)
        for pt in sorted(type_counts.keys()):
            name = PAGE_TYPE_NAMES.get(pt, f"未知({pt})")
            print(f"  类型 {pt:2d} ({name:12s}): {type_counts[pt]:5d} 个页面")
        
        print()
        print("-" * 40)
        print(f"前 20 个页面详情")
        print("-" * 40)
        
        for page in pages[:20]:
            page_num = page['page_num']
            offset = page['offset']
            type_name = page['type_name']
            
            extra_info = ""
            if page['page_type'] == 3 and 'username' in page:
                extra_info = f" | 用户: {page['username']}"
            elif page['page_type'] == 5 and 'subtype_name' in page:
                extra_info = f" | {page['subtype_name']}"
            elif page['page_type'] == 1 and 'array_count' in page:
                extra_info = f" | 数组长度: {page['array_count']}"
            
            print(f"  Page {page_num:4d} @ {offset}: {type_name}{extra_info}")


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        print("用法: python pdms_db_parser.py <database_file>")
        sys.exit(1)
    
    file_path = sys.argv[1]
    
    if not os.path.exists(file_path):
        print(f"错误: 文件不存在: {file_path}")
        sys.exit(1)
    
    try:
        db = PDMSDatabase(file_path)
        db.print_info()
        db.print_page_summary()
    except Exception as e:
        print(f"错误: 解析失败: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)


if __name__ == '__main__':
    main()
