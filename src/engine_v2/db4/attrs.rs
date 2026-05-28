use crate::engine_v2::types::{DbResult, RefNo};

/// 属性类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrType {
    Integer,
    Real,
    String,
    Reference,
    Logical,
    IntArray,
    RealArray,
    RefArray,
}

/// 属性值
#[derive(Debug, Clone)]
pub enum AttrValue {
    Integer(i32),
    Real(f64),
    String(String),
    Reference(RefNo),
    Logical(bool),
    IntArray(Vec<i32>),
    RealArray(Vec<f64>),
    RefArray(Vec<RefNo>),
    Null,
}

/// 属性读取分派 (对齐 db4 的 get_integer/get_string/get_real/get_reference/get_logical)
pub struct AttrReader;

impl AttrReader {
    /// 读取整型属性 (opcode 80)
    pub fn get_integer(data: &[u8], offset: usize) -> i32 {
        if offset + 4 <= data.len() {
            i32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ])
        } else {
            0
        }
    }

    /// 读取实数属性
    pub fn get_real(data: &[u8], offset: usize) -> f64 {
        if offset + 8 <= data.len() {
            f64::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ])
        } else {
            0.0
        }
    }

    /// 读取引用属性
    pub fn get_reference(data: &[u8], offset: usize) -> RefNo {
        if offset + 8 <= data.len() {
            RefNo::from_be_bytes(&data[offset..])
        } else {
            RefNo::new(0, 0)
        }
    }

    /// 读取逻辑属性
    pub fn get_logical(data: &[u8], offset: usize) -> bool {
        if offset < data.len() {
            data[offset] != 0
        } else {
            false
        }
    }

    /// 读取变长字符串属性 (opcode 106)
    pub fn get_string(data: &[u8], offset: usize) -> String {
        if offset + 4 > data.len() {
            return String::new();
        }
        let len = i32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        let str_start = offset + 4;
        let str_end = (str_start + len).min(data.len());
        if str_start >= data.len() {
            return String::new();
        }
        String::from_utf8_lossy(&data[str_start..str_end])
            .trim_end_matches('\0')
            .to_string()
    }

    /// 读取整型数组
    pub fn get_int_array(data: &[u8], offset: usize) -> Vec<i32> {
        if offset + 4 > data.len() {
            return Vec::new();
        }
        let count = Self::get_integer(data, offset) as usize;
        let mut result = Vec::with_capacity(count);
        for i in 0..count {
            let off = offset + 4 + i * 4;
            if off + 4 <= data.len() {
                result.push(Self::get_integer(data, off));
            }
        }
        result
    }

    /// 读取引用数组
    pub fn get_ref_array(data: &[u8], offset: usize) -> Vec<RefNo> {
        if offset + 4 > data.len() {
            return Vec::new();
        }
        let count = Self::get_integer(data, offset) as usize;
        let mut result = Vec::with_capacity(count);
        for i in 0..count {
            let off = offset + 4 + i * 8;
            if off + 8 <= data.len() {
                result.push(Self::get_reference(data, off));
            }
        }
        result
    }
}

/// 属性写入分派 (对齐 db4 的 put_integer/put_string/put_reference)
pub struct AttrWriter;

impl AttrWriter {
    pub fn put_integer(data: &mut [u8], offset: usize, value: i32) {
        if offset + 4 <= data.len() {
            data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        }
    }

    pub fn put_real(data: &mut [u8], offset: usize, value: f64) {
        if offset + 8 <= data.len() {
            data[offset..offset + 8].copy_from_slice(&value.to_be_bytes());
        }
    }

    pub fn put_reference(data: &mut [u8], offset: usize, refno: RefNo) {
        if offset + 8 <= data.len() {
            data[offset..offset + 8].copy_from_slice(&refno.to_be_bytes());
        }
    }

    pub fn put_string(data: &mut [u8], offset: usize, s: &str) -> usize {
        let bytes = s.as_bytes();
        let len = bytes.len();
        if offset + 4 + len <= data.len() {
            data[offset..offset + 4].copy_from_slice(&(len as i32).to_be_bytes());
            data[offset + 4..offset + 4 + len].copy_from_slice(bytes);
            4 + len
        } else {
            0
        }
    }
}
