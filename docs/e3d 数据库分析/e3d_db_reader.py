#!/usr/bin/env python3
"""
E3D 数据库文件读取器

用于读取和解析 AVEVA E3D/PDMS 数据库文件。

使用方法:
    python e3d_db_reader.py <database_file> [command]

命令:
    info          - 显示数据库信息
    scan          - 扫描所有页面
    page <num>    - 显示指定页面的信息
    extract       - 提取所有可读数据
"""

import struct
import sys
import os
from typing import Dict, List, Optional, Tuple, Any


class E3DDatabase:
    """E3D 数据库文件类"""
    
    # 页面类型名称映射
    PAGE_TYPE_NAMES = {
        1: "引用数组页面",
        3: "会话页面",
        5: "数据页面",
        7: "特殊页面",
        8: "索引页面"
    }
    
    # 数据页面子类型名称
    DATA_PAGE_SUBTYPES = {
        7618377: "主要数据页面",
        13387743: "辅助数据页面",
        86284645: "索引数据页面",
        63068511: "属性数据页面",
        66156832: "扩展数据页面"
    }
    
    def __init__(self, file_path: str):
        """
        初始化数据库
        
        Args:
            file_path: 数据库文件路径
        """
        self.file_path = file_path
        self.file_size = os.path.getsize(file_path)
        self.metadata = None
        self._read_metadata()
    
    def _read_metadata(self) -> None:
        """读取数据库元数据"""
        with open(self.file_path, 'rb') as f:
            data = f.read(512)  # 读取页面 0
        
        self.metadata = {
            'db_id': struct.unpack('>I', data[0x08:0x0C])[0],
            'version': struct.unpack('>I', data[0x04:0x08])[0],
            'page_size': struct.unpack('>I', data[0x34:0x38])[0],
            'page_count': struct.unpack('>I', data[0x38:0x3C])[0],
            'session_page': struct.unpack('>I', data[0x30:0x34])[0],
            'ext_no': struct.unpack('>I', data[0x28:0x2C])[0],
            'creation_time': struct.unpack('>I', data[0x20:0x24])[0],
            'description': data[0x40:0x80].decode('ascii').strip('\x00').strip()
        }
        
        # 验证页面大小
        if self.metadata['page_size'] == 0:
            # 如果页面大小为0，使用文件大小除以默认值
            self.metadata['page_size'] = 512
            self.metadata['page_count'] = self.file_size // 512
    
    def read_page(self, page_num: int) -> bytes:
        """
        读取指定页面
        
        Args:
            page_num: 页面号
            
        Returns:
            页面数据
        """
        offset = page_num * self.metadata['page_size']
        with open(self.file_path, 'rb') as f:
            f.seek(offset)
            return f.read(self.metadata['page_size'])
    
    def parse_page_header(self, page_data: bytes) -> Dict[str, Any]:
        """
        解析页面头
        
        Args:
            page_data: 页面数据
            
        Returns:
            页面头信息
        """
        page_type = struct.unpack('>I', page_data[0:4])[0]
        
        result = {
            'page_type': page_type,
            'page_type_name': self.PAGE_TYPE_NAMES.get(page_type, f"未知类型({page_type})")
        }
        
        if page_type == 5:
            # 数据页面
            type_id = struct.unpack('>I', page_data[4:8])[0]
            bucket_id = (type_id >> 13) & 0x1FFF
            subtype_name = self.DATA_PAGE_SUBTYPES.get(type_id, "未知子类型")
            
            result['type_id'] = type_id
            result['bucket_id'] = bucket_id
            result['subtype_name'] = subtype_name
        elif page_type == 3:
            # 会话页面
            session_mark = struct.unpack('>I', page_data[4:8])[0]
            result['session_mark'] = session_mark
        
        return result
    
    def scan_pages(self, page_types: Optional[List[int]] = None) -> Dict[int, List[int]]:
        """
        扫描所有页面
        
        Args:
            page_types: 要扫描的页面类型列表，None 表示扫描所有类型
            
        Returns:
            按页面类型分组的页面号列表
        """
        if page_types is None:
            page_types = [1, 3, 5, 7, 8]
        
        results = {page_type: [] for page_type in page_types}
        
        for page_num in range(self.metadata['page_count']):
            page_data = self.read_page(page_num)
            page_type = struct.unpack('>I', page_data[0:4])[0]
            
            if page_type in page_types:
                results[page_type].append(page_num)
        
        return results
    
    def find_ascii_pages(self, min_chars: int = 64) -> List[Tuple[int, int]]:
        """
        查找包含 ASCII 文本的页面
        
        Args:
            min_chars: 最少 ASCII 字符数
            
        Returns:
            (页面号, ASCII字符数) 列表
        """
        ascii_pages = []
        
        for page_num in range(self.metadata['page_count']):
            page_data = self.read_page(page_num)
            ascii_count = sum(1 for b in page_data if 32 <= b < 127)
            
            if ascii_count >= min_chars:
                ascii_pages.append((page_num, ascii_count))
        
        ascii_pages.sort(key=lambda x: x[1], reverse=True)
        return ascii_pages
    
    def get_page_info(self, page_num: int) -> Dict[str, Any]:
        """
        获取页面信息
        
        Args:
            page_num: 页面号
            
        Returns:
            页面信息
        """
        page_data = self.read_page(page_num)
        page_info = self.parse_page_header(page_data)
        page_info['page_num'] = page_num
        page_info['page_data'] = page_data
        
        return page_info
    
    def print_info(self) -> None:
        """打印数据库信息"""
        print("=" * 80)
        print("E3D 数据库信息")
        print("=" * 80)
        print(f"文件路径:      {self.file_path}")
        print(f"文件大小:      {self.file_size:,} 字节 ({self.file_size / 1024:.2f} KB)")
        print()
        print(f"数据库ID:      {self.metadata['db_id']}")
        print(f"版本:          {self.metadata['version']}")
        print(f"页面大小:      {self.metadata['page_size']} 字节")
        print(f"总页数:        {self.metadata['page_count']}")
        print(f"会话页面号:    {self.metadata['session_page']}")
        print(f"扩展号:        {self.metadata['ext_no']}")
        print(f"创建时间:      {self.metadata['creation_time']}")
        print()
        print(f"描述:")
        print(f"  {self.metadata['description']}")
    
    def print_scan_results(self, results: Dict[int, List[int]]) -> None:
        """打印扫描结果"""
        print()
        print("=" * 80)
        print("页面扫描结果")
        print("=" * 80)
        
        for page_type in sorted(results.keys()):
            page_nums = results[page_type]
            page_type_name = self.PAGE_TYPE_NAMES.get(page_type, f"未知类型({page_type})")
            print(f"  类型 {page_type} ({page_type_name}): {len(page_nums)} 个页面")
    
    def print_page_info(self, page_num: int, show_data: bool = False) -> None:
        """
        打印页面信息
        
        Args:
            page_num: 页面号
            show_data: 是否显示页面数据
        """
        page_info = self.get_page_info(page_num)
        
        print()
        print("=" * 80)
        print(f"页面 {page_num} 信息")
        print("=" * 80)
        print(f"  页面类型:      {page_info['page_type']} ({page_info['page_type_name']})")
        
        if 'type_id' in page_info:
            print(f"  类型ID:        {page_info['type_id']} = 0x{page_info['type_id']:08X}")
            print(f"  桶ID:          {page_info['bucket_id']}")
            print(f"  子类型:        {page_info['subtype_name']}")
        
        if 'session_mark' in page_info:
            print(f"  会话标记:      {page_info['session_mark']}")
        
        if show_data:
            print()
            print(f"  页面数据 (前128字节):")
            page_data = page_info['page_data']
            
            for i in range(0, min(128, len(page_data)), 16):
                snippet = page_data[i:i+16]
                hex_str = ' '.join(f'{b:02X}' for b in snippet)
                ascii_str = ''.join(chr(b) if 32 <= b < 127 else '.' for b in snippet)
                print(f"    +0x{i:02X}: {hex_str:<48} | {ascii_str}")
    
    def print_ascii_pages(self, ascii_pages: List[Tuple[int, int]], limit: int = 10) -> None:
        """
        打印包含 ASCII 文本的页面
        
        Args:
            ascii_pages: (页面号, ASCII字符数) 列表
            limit: 显示的页面数量限制
        """
        print()
        print("=" * 80)
        print(f"包含 ASCII 文本的页面 (前 {limit} 个)")
        print("=" * 80)
        
        for page_num, ascii_count in ascii_pages[:limit]:
            page_data = self.read_page(page_num)
            
            # 显示前64字节的 ASCII 内容
            ascii_str = ''.join(chr(b) if 32 <= b < 127 else '.' for b in page_data[:64])
            
            print()
            print(f"  页面 {page_num}: {ascii_count} 个 ASCII 字符")
            print(f"    ASCII 内容: {ascii_str}")
    
    def extract_data(self, output_dir: str = "extracted") -> None:
        """
        提取数据库数据
        
        Args:
            output_dir: 输出目录
        """
        os.makedirs(output_dir, exist_ok=True)
        
        # 扫描所有页面
        scan_results = self.scan_pages()
        
        print(f"提取数据到 {output_dir}...")
        
        # 按页面类型提取数据
        for page_type, page_nums in scan_results.items():
            page_type_name = self.PAGE_TYPE_NAMES.get(page_type, f"type_{page_type}")
            type_dir = os.path.join(output_dir, page_type_name)
            os.makedirs(type_dir, exist_ok=True)
            
            print(f"  提取类型 {page_type} ({page_type_name}) 数据...")
            
            for page_num in page_nums:
                page_data = self.read_page(page_num)
                
                # 保存页面数据
                output_file = os.path.join(type_dir, f"page_{page_num:04d}.bin")
                with open(output_file, 'wb') as f:
                    f.write(page_data)
                
                # 如果包含 ASCII 文本，也保存文本版本
                ascii_count = sum(1 for b in page_data if 32 <= b < 127)
                if ascii_count > 64:
                    ascii_file = os.path.join(type_dir, f"page_{page_num:04d}.txt")
                    ascii_str = ''.join(chr(b) if 32 <= b < 127 else '.' for b in page_data)
                    with open(ascii_file, 'w', encoding='utf-8') as f:
                        f.write(f"页面 {page_num} ASCII 内容\n")
                        f.write("=" * 80 + "\n")
                        f.write(ascii_str)
        
        print(f"提取完成！数据保存在 {output_dir}")


def main():
    """主函数"""
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)
    
    file_path = sys.argv[1]
    command = sys.argv[2] if len(sys.argv) > 2 else 'info'
    
    # 打开数据库
    try:
        db = E3DDatabase(file_path)
    except Exception as e:
        print(f"错误: 无法打开数据库文件: {e}")
        sys.exit(1)
    
    # 执行命令
    if command == 'info':
        db.print_info()
    
    elif command == 'scan':
        db.print_info()
        scan_results = db.scan_pages()
        db.print_scan_results(scan_results)
        
        # 显示包含 ASCII 文本的页面
        ascii_pages = db.find_ascii_pages()
        if ascii_pages:
            db.print_ascii_pages(ascii_pages, limit=10)
    
    elif command == 'page':
        if len(sys.argv) < 4:
            print("错误: 请指定页面号")
            print("使用方法: python e3d_db_reader.py <file> page <page_num>")
            sys.exit(1)
        
        page_num = int(sys.argv[3])
        show_data = '--show-data' in sys.argv or '-s' in sys.argv
        
        db.print_page_info(page_num, show_data=show_data)
    
    elif command == 'extract':
        output_dir = sys.argv[3] if len(sys.argv) > 3 else 'extracted'
        db.print_info()
        db.extract_data(output_dir)
    
    else:
        print(f"错误: 未知命令 '{command}'")
        print(__doc__)
        sys.exit(1)


if __name__ == '__main__':
    main()
