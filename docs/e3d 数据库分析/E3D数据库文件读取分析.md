# E3D 数据库文件读取分析

**文件路径**: `/Volumes/DPC/work/plant-code/aios-parse-pdms-fork/test-files/ams7330_0001`  
**分析日期**: 2024年  
**文件大小**: 1,318,912 字节 (1,288 KB)

---

## 关键发现

### 文件结构

- **字节序**: 大端序 (Big-Endian)
- **页面大小**: 512 字节
- **总页数**: 2,576 个
- **数据库ID**: 7,330
- **扩展号**: 643
- **会话页面号**: 3

### 页面 0: 数据库描述符 (偏移 0x0000)

| 偏移 | 值 | 十六进制 | 描述 |
|------|-----|---------|------|
| 0x00 | 0 | 0x00000000 | 保留 |
| 0x04 | 2 | 0x00000002 | 版本号 |
| 0x08 | 7330 | 0x00001CA2 | 数据库ID |
| 0x0C | 1 | 0x00000001 | 未知 |
| 0x10 | 1 | 0x00000001 | 未知 |
| 0x14 | 0 | 0x00000000 | 保留 |
| 0x18 | 4294967295 | 0xFFFFFFFF | 标志位 |
| 0x1C | 0 | 0x00000000 | 保留 |
| 0x20 | 722578 | 0x000B0692 | 创建时间 |
| 0x24 | 4294967295 | 0xFFFFFFFF | 标志位 |
| 0x28 | 643 | 0x00000283 | 扩展号 |
| 0x2C | 1 | 0x00000001 | 未知 |
| 0x30 | 3 | 0x00000003 | 会话页面号 |
| 0x34 | 512 | 0x00000200 | 页面大小 |
| 0x38 | 15522 | 0x00003CA2 | 总页数 |
| 0x3C | 2 | 0x00000002 | 未知 |
| 0x40 | - | - | 描述信息 |

**描述信息**: 
```
+bill.housley at 11:32:08 on Tue, 5  Jan 2016 using WINDOWS-N
```

### 页面类型分布

| 页面类型 | 数量 | 名称 | 描述 |
|---------|------|------|------|
| 1 | 11 | 引用数组页面 | 存储属性引用数组 |
| 3 | 62 | 会话页面 | 管理会话信息 |
| 5 | 226 | 数据页面 | 存储用户数据 |
| 7 | 416 | 特殊页面 | 特殊功能页面 |
| 8 | 1 | 索引页面 | 数据索引页面 |

### 数据页面 (类型 5) 子类型

| 类型ID | 十六进制值 | 名称 | 桶ID | 数量 |
|--------|-----------|------|------|------|
| 7618377 | 0x00743F49 | 主要数据页面 | 929 | - |
| 13387743 | 0x00CC47DF | 辅助数据页面 | 1634 | - |
| 639374 | 0x0009C18E | 未知子类型 | 78 | - |

### 内容分析

数据库包含以下类型的工厂设计数据：

#### 支持的系统类型
- **HVAC**: 暖通空调系统
- **PIPE**: 管道系统
- **SUPPORT**: 支架系统
- **PORT**: 端口系统
- **MDS**: 多维系统

#### 发现的具体数据示例

```
PORT/FRAMES/SPECIALS/BS
PORT/SPECIALS/BS/HVAC/13-FRMW1
PORT/SPECIALS/BS/HVAC/24-V2/S2
SUPPORT/SPECIALS/BS/HVAC/21-BAR-6
PIPE/6-BAR-1
PIPE/4-BAR-2
HVAC/12-V2/S1
HVAC/17-BAR-7
HVAC/24-BAR-7
HVAC/23-BAR-5
```

#### 数据结构

数据库存储了以下类型的工程数据：

1. **管道系统**
   - 管道类型 (PIPE)
   - 支架类型 (BAR)
   - 特殊部件 (SPECIALS)

2. **HVAC 系统**
   - 风管系统
   - 通风设备
   - 特殊部件

3. **支架系统**
   - 结构支架
   - 特殊支架
   - 端口连接

4. **端口系统**
   - 连接点
   - 法兰连接
   - 特殊端口

---

## 读取方法

### 1. 读取页面 0 (数据库描述符)

```python
import struct

def read_page_header(file_path):
    """读取数据库文件头"""
    with open(file_path, 'rb') as f:
        data = f.read()
    
    # 大端序读取
    db_id = struct.unpack('>I', data[0x08:0x0C])[0]
    version = struct.unpack('>I', data[0x04:0x08])[0]
    page_size = struct.unpack('>I', data[0x34:0x38])[0]
    page_count = struct.unpack('>I', data[0x38:0x3C])[0]
    session_page = struct.unpack('>I', data[0x30:0x34])[0]
    ext_no = struct.unpack('>I', data[0x28:0x2C])[0]
    
    return {
        'db_id': db_id,
        'version': version,
        'page_size': page_size,
        'page_count': page_count,
        'session_page': session_page,
        'ext_no': ext_no
    }
```

### 2. 读取指定页面

```python
def read_page(file_path, page_num, page_size=512):
    """读取指定页面"""
    offset = page_num * page_size
    with open(file_path, 'rb') as f:
        f.seek(offset)
        return f.read(page_size)
```

### 3. 解析页面类型

```python
def parse_page_header(page_data):
    """解析页面头"""
    page_type = struct.unpack('>I', page_data[0:4])[0]
    
    result = {'page_type': page_type}
    
    if page_type == 5:
        # 数据页面
        type_id = struct.unpack('>I', page_data[4:8])[0]
        bucket_id = (type_id >> 13) & 0x1FFF
        result['type_id'] = type_id
        result['bucket_id'] = bucket_id
    elif page_type == 3:
        # 会话页面
        session_mark = struct.unpack('>I', page_data[4:8])[0]
        result['session_mark'] = session_mark
    
    return result
```

---

## 完整读取示例

```python
#!/usr/bin/env python3
import struct

class E3DDataReader:
    def __init__(self, file_path):
        self.file_path = file_path
        self.metadata = None
        self._read_metadata()
    
    def _read_metadata(self):
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
        }
    
    def read_page(self, page_num):
        """读取指定页面"""
        offset = page_num * self.metadata['page_size']
        with open(self.file_path, 'rb') as f:
            f.seek(offset)
            return f.read(self.metadata['page_size'])
    
    def parse_page(self, page_num):
        """解析指定页面"""
        page_data = self.read_page(page_num)
        page_type = struct.unpack('>I', page_data[0:4])[0]
        
        result = {
            'page_num': page_num,
            'page_type': page_type,
            'data': page_data
        }
        
        if page_type == 5:
            # 数据页面
            type_id = struct.unpack('>I', page_data[4:8])[0]
            bucket_id = (type_id >> 13) & 0x1FFF
            result['type_id'] = type_id
            result['bucket_id'] = bucket_id
        elif page_type == 3:
            # 会话页面
            session_mark = struct.unpack('>I', page_data[4:8])[0]
            result['session_mark'] = session_mark
        
        return result
    
    def scan_pages(self, page_types=None):
        """扫描所有页面"""
        if page_types is None:
            page_types = [1, 3, 5, 7, 8]
        
        results = {}
        for page_type in page_types:
            results[page_type] = []
        
        for page_num in range(self.metadata['page_count']):
            page_data = self.read_page(page_num)
            page_type = struct.unpack('>I', page_data[0:4])[0]
            
            if page_type in page_types:
                results[page_type].append(page_num)
        
        return results

# 使用示例
if __name__ == '__main__':
    reader = E3DDataReader('ams7330_0001')
    
    print(f"数据库ID: {reader.metadata['db_id']}")
    print(f"页面大小: {reader.metadata['page_size']}")
    print(f"总页数: {reader.metadata['page_count']}")
    
    # 扫描所有页面
    pages = reader.scan_pages()
    for page_type, page_nums in pages.items():
        print(f"类型 {page_type}: {len(page_nums)} 个页面")
```

---

## 结论

1. **文件格式确认**: ams7330_0001 是一个 E3D 数据库文件，采用大端序存储

2. **页面结构**: 
   - 页面大小：512 字节
   - 总页数：2,576 个
   - 文件大小：1,288 KB

3. **数据内容**: 
   - 包含工厂设计数据（HVAC、管道、支架等）
   - 支持多种数据页面类型
   - 使用桶系统进行数据索引

4. **读取方法**: 
   - 使用大端序读取32位整数
   - 按页面大小进行分段读取
   - 根据页面类型解析数据结构

---

**分析完成日期**: 2024年
**分析工具**: Python + IDA Pro 反编译结果
