//! 元素序列化模块
//!
//! 将 EleData 序列化为 PDMS 二进制格式，用于写入数据库文件。
//!
//! # 二进制格式结构
//! ```text
//! +------------------+
//! | impl_len (4B)    |  隐式属性长度（以 4 字节为单位）
//! +------------------+
//! | refno (8B)       |  参考号 (refno_0 + refno_1)
//! +------------------+
//! | type_hash (4B)   |  类型哈希值
//! +------------------+
//! | owner (8B)       |  所有者参考号
//! +------------------+
//! | implicit attrs   |  隐式属性数据（固定偏移量）
//! +------------------+
//! | members block    |  成员列表（可选）
//! +------------------+
//! | explicit attrs   |  显式属性数据（变长）
//! +------------------+
//! ```

use std::collections::HashMap;

/// 序列化错误类型
#[derive(Debug, Clone)]
pub enum SerializeError {
    /// 属性类型不支持
    UnsupportedAttrType(String),
    /// 数据溢出
    DataOverflow { max: usize, actual: usize },
    /// 缺失必要的属性信息
    MissingAttrInfo(String),
    /// 序列化失败
    SerializationFailed(String),
}

impl std::fmt::Display for SerializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SerializeError::UnsupportedAttrType(t) => write!(f, "不支持的属性类型: {}", t),
            SerializeError::DataOverflow { max, actual } => {
                write!(f, "数据溢出: 最大 {} 字节, 实际 {} 字节", max, actual)
            }
            SerializeError::MissingAttrInfo(name) => write!(f, "缺失属性信息: {}", name),
            SerializeError::SerializationFailed(msg) => write!(f, "序列化失败: {}", msg),
        }
    }
}

impl std::error::Error for SerializeError {}

/// 序列化统计信息
#[derive(Debug, Default, Clone)]
pub struct SerializeStats {
    /// 序列化的元素数量
    pub elements_serialized: u64,
    /// 总字节数
    pub total_bytes: u64,
    /// 隐式属性字节数
    pub implicit_bytes: u64,
    /// 显式属性字节数
    pub explicit_bytes: u64,
    /// 成员列表字节数
    pub members_bytes: u64,
}

/// 元素序列化器
///
/// 将 EleData 序列化为 PDMS 二进制格式
#[derive(Debug, Default)]
pub struct EleSerializer {
    /// 页面大小
    page_size: usize,
    /// 序列化统计
    stats: SerializeStats,
}

impl EleSerializer {
    /// 创建新的序列化器
    pub fn new(page_size: usize) -> Self {
        Self {
            page_size,
            stats: SerializeStats::default(),
        }
    }

    /// 使用 512 字节页面创建
    pub fn new_512() -> Self {
        Self::new(512)
    }

    /// 使用 2K 字节页面创建
    pub fn new_2k() -> Self {
        Self::new(2048)
    }

    /// 获取统计信息
    pub fn stats(&self) -> &SerializeStats {
        &self.stats
    }

    /// 序列化 RefU64 为大端字节序
    #[inline]
    pub fn serialize_refu64(refno: u64) -> [u8; 8] {
        refno.to_be_bytes()
    }

    /// 序列化 u32 为大端字节序
    #[inline]
    pub fn serialize_u32(value: u32) -> [u8; 4] {
        value.to_be_bytes()
    }

    /// 序列化 i32 为大端字节序
    #[inline]
    pub fn serialize_i32(value: i32) -> [u8; 4] {
        value.to_be_bytes()
    }

    /// 序列化 f64 为大端字节序
    #[inline]
    pub fn serialize_f64(value: f64) -> [u8; 8] {
        value.to_be_bytes()
    }

    /// 序列化 f32 为大端字节序
    #[inline]
    pub fn serialize_f32(value: f32) -> [u8; 4] {
        value.to_be_bytes()
    }

    /// 序列化成员列表头部
    ///
    /// 格式: flag(2B) + len(2B) + refno(8B) + ...
    pub fn serialize_members_header(refno: u64, member_count: usize) -> Vec<u8> {
        let mut data = Vec::with_capacity(12);
        
        // flag = 0x0002 表示成员列表
        data.extend_from_slice(&[0x00, 0x02]);
        
        // 长度（以 4 字节为单位）: 头部(3 words) + 成员数据
        let len_words = 3 + member_count * 2; // 每个成员占 8 字节 = 2 words
        data.extend_from_slice(&(len_words as u16).to_be_bytes());
        
        // refno
        data.extend_from_slice(&Self::serialize_refu64(refno));
        
        data
    }

    /// 序列化成员列表
    ///
    /// 格式: header + member_refnos
    pub fn serialize_members(refno: u64, members: &[u64]) -> Vec<u8> {
        if members.is_empty() {
            return Vec::new();
        }

        let mut data = Self::serialize_members_header(refno, members.len());
        
        // 写入成员参考号
        for &member_refno in members {
            data.extend_from_slice(&Self::serialize_refu64(member_refno));
        }
        
        data
    }

    /// 序列化显式属性头部
    ///
    /// 格式: flag(2B) + len(2B) + refno(8B) + attr_hash(4B) + attr_type(4B) + data...
    pub fn serialize_explicit_attr_header(
        refno: u64,
        attr_hash: i32,
        attr_type: u16,
        data_len_words: u16,
    ) -> Vec<u8> {
        let mut header = Vec::with_capacity(20);
        
        // flag = 0x0001 表示显式属性
        header.extend_from_slice(&[0x00, 0x01]);
        
        // 总长度（以 4 字节为单位）
        let total_words = 5 + data_len_words; // header(5 words) + data
        header.extend_from_slice(&total_words.to_be_bytes());
        
        // refno
        header.extend_from_slice(&Self::serialize_refu64(refno));
        
        // 属性哈希
        header.extend_from_slice(&Self::serialize_i32(attr_hash));
        
        // 属性类型
        header.extend_from_slice(&(attr_type as u32).to_be_bytes());
        
        header
    }

    /// 序列化字符串值
    ///
    /// 格式: length(4B) + chars (每个字符 4B 大端)
    pub fn serialize_string(s: &str) -> Vec<u8> {
        let chars: Vec<char> = s.chars().collect();
        let mut data = Vec::with_capacity(4 + chars.len() * 4);
        
        // 字符数量
        data.extend_from_slice(&Self::serialize_u32(chars.len() as u32));
        
        // 每个字符以 4 字节存储（大端序）
        for c in chars {
            data.extend_from_slice(&(c as u32).to_be_bytes());
        }
        
        data
    }

    /// 序列化 Vec3 (Position/Direction/Orientation)
    ///
    /// 格式: x(8B) + y(8B) + z(8B), 全部为 f64 大端序
    pub fn serialize_vec3_f64(x: f64, y: f64, z: f64) -> [u8; 24] {
        let mut data = [0u8; 24];
        data[0..8].copy_from_slice(&Self::serialize_f64(x));
        data[8..16].copy_from_slice(&Self::serialize_f64(y));
        data[16..24].copy_from_slice(&Self::serialize_f64(z));
        data
    }

    /// 序列化 Vec3 (f32 版本)
    pub fn serialize_vec3_f32(x: f32, y: f32, z: f32) -> [u8; 12] {
        let mut data = [0u8; 12];
        data[0..4].copy_from_slice(&Self::serialize_f32(x));
        data[4..8].copy_from_slice(&Self::serialize_f32(y));
        data[8..12].copy_from_slice(&Self::serialize_f32(z));
        data
    }

    /// 序列化页面填充
    ///
    /// 使用 0x00000007 填充剩余空间
    pub fn serialize_padding(&self, current_size: usize) -> Vec<u8> {
        let remaining = if current_size % self.page_size == 0 {
            0
        } else {
            self.page_size - (current_size % self.page_size)
        };
        
        let mut padding = Vec::with_capacity(remaining);
        // 使用 0x00000007 作为填充标记
        for _ in 0..(remaining / 4) {
            padding.extend_from_slice(&[0x00, 0x00, 0x00, 0x07]);
        }
        // 处理不能被 4 整除的情况
        for _ in 0..(remaining % 4) {
            padding.push(0x00);
        }
        
        padding
    }

    /// 序列化元素头部
    ///
    /// 格式: impl_len(4B) + refno(8B) + type_hash(4B) + owner(8B)
    pub fn serialize_element_header(
        impl_len_words: u32,
        refno: u64,
        type_hash: u32,
        owner: u64,
    ) -> Vec<u8> {
        let mut header = Vec::with_capacity(24);
        
        // 隐式属性长度（以 4 字节为单位）
        header.extend_from_slice(&Self::serialize_u32(impl_len_words));
        
        // 参考号
        header.extend_from_slice(&Self::serialize_refu64(refno));
        
        // 类型哈希
        header.extend_from_slice(&Self::serialize_u32(type_hash));
        
        // 所有者参考号
        header.extend_from_slice(&Self::serialize_refu64(owner));
        
        header
    }

    /// 序列化隐式属性缓冲区
    ///
    /// 创建固定大小的隐式属性区域，属性按偏移量写入
    ///
    /// # 参数
    /// * `total_words` - 隐式属性区域的总 word 数（包含头部 6 words）
    pub fn create_implicit_buffer(total_words: usize) -> Vec<u8> {
        vec![0u8; total_words * 4]
    }

    /// 在隐式属性缓冲区中写入 i32 值
    ///
    /// # 参数
    /// * `buffer` - 隐式属性缓冲区
    /// * `offset` - 偏移量（以 word 为单位，从头部开始计算）
    /// * `value` - 要写入的值
    pub fn write_i32_at_offset(buffer: &mut [u8], offset: usize, value: i32) {
        let byte_offset = offset * 4;
        if byte_offset + 4 <= buffer.len() {
            buffer[byte_offset..byte_offset + 4].copy_from_slice(&Self::serialize_i32(value));
        }
    }

    /// 在隐式属性缓冲区中写入 u32 值
    pub fn write_u32_at_offset(buffer: &mut [u8], offset: usize, value: u32) {
        let byte_offset = offset * 4;
        if byte_offset + 4 <= buffer.len() {
            buffer[byte_offset..byte_offset + 4].copy_from_slice(&Self::serialize_u32(value));
        }
    }

    /// 在隐式属性缓冲区中写入 f64 值（占用 2 words）
    pub fn write_f64_at_offset(buffer: &mut [u8], offset: usize, value: f64) {
        let byte_offset = offset * 4;
        if byte_offset + 8 <= buffer.len() {
            buffer[byte_offset..byte_offset + 8].copy_from_slice(&Self::serialize_f64(value));
        }
    }

    /// 在隐式属性缓冲区中写入 f32 值
    pub fn write_f32_at_offset(buffer: &mut [u8], offset: usize, value: f32) {
        let byte_offset = offset * 4;
        if byte_offset + 4 <= buffer.len() {
            buffer[byte_offset..byte_offset + 4].copy_from_slice(&Self::serialize_f32(value));
        }
    }

    /// 在隐式属性缓冲区中写入 RefU64 值（占用 2 words）
    pub fn write_refu64_at_offset(buffer: &mut [u8], offset: usize, value: u64) {
        let byte_offset = offset * 4;
        if byte_offset + 8 <= buffer.len() {
            buffer[byte_offset..byte_offset + 8].copy_from_slice(&Self::serialize_refu64(value));
        }
    }

    /// 在隐式属性缓冲区中写入 Vec3 值（占用 6 words，每个分量为 f64）
    pub fn write_vec3_f64_at_offset(buffer: &mut [u8], offset: usize, x: f64, y: f64, z: f64) {
        let byte_offset = offset * 4;
        if byte_offset + 24 <= buffer.len() {
            buffer[byte_offset..byte_offset + 24].copy_from_slice(&Self::serialize_vec3_f64(x, y, z));
        }
    }

    /// 序列化显式属性块（完整）
    ///
    /// 格式: flag(2B) + len(2B) + refno(8B) + attr_hash(4B) + attr_type(4B) + data
    pub fn serialize_explicit_attr_block(
        refno: u64,
        attr_hash: i32,
        attr_type: u16,
        data: &[u8],
    ) -> Vec<u8> {
        // 计算数据长度（以 word 为单位，向上取整）
        let data_words = (data.len() + 3) / 4;
        let header = Self::serialize_explicit_attr_header(refno, attr_hash, attr_type, data_words as u16);
        
        let mut block = header;
        block.extend_from_slice(data);
        
        // 填充到 4 字节对齐
        let padding_bytes = data_words * 4 - data.len();
        for _ in 0..padding_bytes {
            block.push(0x00);
        }
        
        block
    }

    /// 序列化元素结束标记
    ///
    /// 在显式属性后添加 0x00000000 和 0x00000007 标记
    pub fn serialize_end_marker() -> Vec<u8> {
        vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_refu64() {
        let refno: u64 = 0x0000001C_00000001; // 28_1
        let bytes = EleSerializer::serialize_refu64(refno);
        assert_eq!(bytes, [0x00, 0x00, 0x00, 0x1C, 0x00, 0x00, 0x00, 0x01]);
    }

    #[test]
    fn test_serialize_u32() {
        let value: u32 = 0x00C8AAEE; // 类型哈希示例
        let bytes = EleSerializer::serialize_u32(value);
        assert_eq!(bytes, [0x00, 0xC8, 0xAA, 0xEE]);
    }

    #[test]
    fn test_serialize_string() {
        let s = "AB";
        let bytes = EleSerializer::serialize_string(s);
        // 长度(2) + 'A'(0x41) + 'B'(0x42)
        assert_eq!(bytes, [
            0x00, 0x00, 0x00, 0x02, // length = 2
            0x00, 0x00, 0x00, 0x41, // 'A'
            0x00, 0x00, 0x00, 0x42, // 'B'
        ]);
    }

    #[test]
    fn test_serialize_vec3_f64() {
        let bytes = EleSerializer::serialize_vec3_f64(1.0, 2.0, 3.0);
        assert_eq!(bytes.len(), 24);
        
        // 验证 x = 1.0
        let x = f64::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3], 
                                     bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(x, 1.0);
    }

    #[test]
    fn test_serialize_members() {
        let refno: u64 = 0x0000001C_00000001;
        let members = vec![0x0000001C_00000002u64, 0x0000001C_00000003u64];
        
        let bytes = EleSerializer::serialize_members(refno, &members);
        
        // flag(2) + len(2) + refno(8) + 2 members(16) = 28 bytes
        assert_eq!(bytes.len(), 28);
        
        // 验证 flag
        assert_eq!(&bytes[0..2], &[0x00, 0x02]);
    }

    #[test]
    fn test_serialize_padding() {
        let serializer = EleSerializer::new_512();
        
        // 当前大小 100，需要填充到 512
        let padding = serializer.serialize_padding(100);
        assert_eq!(padding.len(), 412); // 512 - 100 = 412
        
        // 验证填充内容
        assert_eq!(&padding[0..4], &[0x00, 0x00, 0x00, 0x07]);
    }

    #[test]
    fn test_serialize_element_header() {
        let refno: u64 = 0x0000001C_00000001;
        let type_hash: u32 = 0x00C8AAEE;
        let owner: u64 = 0x0000001C_00000000;
        
        let header = EleSerializer::serialize_element_header(10, refno, type_hash, owner);
        
        // impl_len(4) + refno(8) + type_hash(4) + owner(8) = 24 bytes
        assert_eq!(header.len(), 24);
        
        // 验证 impl_len = 10
        assert_eq!(&header[0..4], &[0x00, 0x00, 0x00, 0x0A]);
        
        // 验证 refno
        assert_eq!(&header[4..12], &[0x00, 0x00, 0x00, 0x1C, 0x00, 0x00, 0x00, 0x01]);
    }

    #[test]
    fn test_write_at_offset() {
        let mut buffer = EleSerializer::create_implicit_buffer(10);
        assert_eq!(buffer.len(), 40);
        
        // 在 offset 2 写入 i32
        EleSerializer::write_i32_at_offset(&mut buffer, 2, 0x12345678);
        assert_eq!(&buffer[8..12], &[0x12, 0x34, 0x56, 0x78]);
        
        // 在 offset 4 写入 f64
        EleSerializer::write_f64_at_offset(&mut buffer, 4, 1.5);
        let f = f64::from_be_bytes([buffer[16], buffer[17], buffer[18], buffer[19],
                                     buffer[20], buffer[21], buffer[22], buffer[23]]);
        assert_eq!(f, 1.5);
    }

    #[test]
    fn test_serialize_explicit_attr_block() {
        let refno: u64 = 0x0000001C_00000001;
        let attr_hash: i32 = 0x00ABCDEF;
        let data = vec![0x01, 0x02, 0x03, 0x04, 0x05]; // 5 bytes
        
        let block = EleSerializer::serialize_explicit_attr_block(refno, attr_hash, 0x01, &data);
        
        // header(20) + data(5 -> 8 with padding) = 28 bytes
        assert_eq!(block.len(), 28);
        
        // 验证 flag
        assert_eq!(&block[0..2], &[0x00, 0x01]);
    }

    #[test]
    fn test_serialize_end_marker() {
        let marker = EleSerializer::serialize_end_marker();
        assert_eq!(marker.len(), 8);
        assert_eq!(marker, vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07]);
    }
}

