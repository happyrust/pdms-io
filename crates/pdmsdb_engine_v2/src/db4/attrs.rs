use crate::core::{EngineError, RefNo};

#[derive(Debug, Clone, PartialEq)]
pub enum AttrValue {
    Integer(i32),
    Real(f64),
    Float(f32),
    String(String),
    Reference(RefNo),
    Logical(bool),
    IntArray(Vec<i32>),
    RealArray(Vec<f64>),
    RefArray(Vec<RefNo>),
    Direction([f64; 3]),
    Position([f64; 3]),
    Orientation([f64; 9]),
    Raw(Vec<u8>),
}

impl AttrValue {
    pub fn as_integer(&self) -> Option<i32> {
        match self {
            AttrValue::Integer(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_real(&self) -> Option<f64> {
        match self {
            AttrValue::Real(v) => Some(*v),
            AttrValue::Float(v) => Some(*v as f64),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            AttrValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_reference(&self) -> Option<RefNo> {
        match self {
            AttrValue::Reference(r) => Some(*r),
            _ => None,
        }
    }

    pub fn as_logical(&self) -> Option<bool> {
        match self {
            AttrValue::Logical(b) => Some(*b),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrType {
    Integer,
    Real,
    Double,
    String,
    Reference,
    Logical,
    Direction,
    Position,
    Orientation,
    IntVec,
    FloatVec,
    DoubleVec,
}

#[derive(Debug, Clone)]
pub struct AttrInfo {
    pub name: String,
    pub hash: u32,
    pub att_type: AttrType,
    pub offset: u32,
}

pub fn read_implicit_integer(implicit_data: &[u8], word_offset: u32) -> Result<i32, EngineError> {
    let byte_offset = word_offset as usize * 4;
    if byte_offset + 4 > implicit_data.len() {
        return Err(EngineError::Format(format!(
            "隐式属性偏移越界: offset={}, len={}",
            byte_offset,
            implicit_data.len()
        )));
    }
    Ok(i32::from_be_bytes(
        implicit_data[byte_offset..byte_offset + 4]
            .try_into()
            .unwrap(),
    ))
}

pub fn read_implicit_real_f64(
    implicit_data: &[u8],
    word_offset: u32,
) -> Result<f64, EngineError> {
    let byte_offset = word_offset as usize * 4;
    if byte_offset + 8 > implicit_data.len() {
        return Err(EngineError::Format(format!(
            "隐式 f64 属性偏移越界: offset={}, len={}",
            byte_offset,
            implicit_data.len()
        )));
    }
    Ok(f64::from_be_bytes(
        implicit_data[byte_offset..byte_offset + 8]
            .try_into()
            .unwrap(),
    ))
}

pub fn read_implicit_real_f32(
    implicit_data: &[u8],
    word_offset: u32,
) -> Result<f32, EngineError> {
    let byte_offset = word_offset as usize * 4;
    if byte_offset + 4 > implicit_data.len() {
        return Err(EngineError::Format(format!(
            "隐式 f32 属性偏移越界: offset={}, len={}",
            byte_offset,
            implicit_data.len()
        )));
    }
    Ok(f32::from_be_bytes(
        implicit_data[byte_offset..byte_offset + 4]
            .try_into()
            .unwrap(),
    ))
}

pub fn read_implicit_reference(
    implicit_data: &[u8],
    word_offset: u32,
) -> Result<RefNo, EngineError> {
    let byte_offset = word_offset as usize * 4;
    if byte_offset + 8 > implicit_data.len() {
        return Err(EngineError::Format(format!(
            "隐式引用属性偏移越界: offset={}, len={}",
            byte_offset,
            implicit_data.len()
        )));
    }
    let hi = u32::from_be_bytes(
        implicit_data[byte_offset..byte_offset + 4]
            .try_into()
            .unwrap(),
    );
    let lo = u32::from_be_bytes(
        implicit_data[byte_offset + 4..byte_offset + 8]
            .try_into()
            .unwrap(),
    );
    Ok(RefNo::from_parts(hi, lo))
}

pub fn read_implicit_direction(
    implicit_data: &[u8],
    word_offset: u32,
    is_f32: bool,
) -> Result<[f64; 3], EngineError> {
    if is_f32 {
        let x = read_implicit_real_f32(implicit_data, word_offset)? as f64;
        let y = read_implicit_real_f32(implicit_data, word_offset + 1)? as f64;
        let z = read_implicit_real_f32(implicit_data, word_offset + 2)? as f64;
        Ok([x, y, z])
    } else {
        let x = read_implicit_real_f64(implicit_data, word_offset)?;
        let y = read_implicit_real_f64(implicit_data, word_offset + 2)?;
        let z = read_implicit_real_f64(implicit_data, word_offset + 4)?;
        Ok([x, y, z])
    }
}

pub fn read_implicit_logical(
    implicit_data: &[u8],
    word_offset: u32,
) -> Result<bool, EngineError> {
    let v = read_implicit_integer(implicit_data, word_offset)?;
    Ok(v != 0)
}

pub fn read_implicit_attr(
    implicit_data: &[u8],
    attr_info: &AttrInfo,
    is_f32: bool,
) -> Result<AttrValue, EngineError> {
    match attr_info.att_type {
        AttrType::Integer => {
            read_implicit_integer(implicit_data, attr_info.offset).map(AttrValue::Integer)
        }
        AttrType::Real | AttrType::Double => {
            if is_f32 {
                read_implicit_real_f32(implicit_data, attr_info.offset)
                    .map(AttrValue::Float)
            } else {
                read_implicit_real_f64(implicit_data, attr_info.offset)
                    .map(AttrValue::Real)
            }
        }
        AttrType::String => {
            read_implicit_integer(implicit_data, attr_info.offset)
                .map(|v| AttrValue::String(format!("{}", v)))
        }
        AttrType::Reference => {
            read_implicit_reference(implicit_data, attr_info.offset).map(AttrValue::Reference)
        }
        AttrType::Logical => {
            read_implicit_logical(implicit_data, attr_info.offset).map(AttrValue::Logical)
        }
        AttrType::Direction | AttrType::Position => {
            read_implicit_direction(implicit_data, attr_info.offset, is_f32)
                .map(AttrValue::Direction)
        }
        AttrType::Orientation => {
            let mut vals = [0.0f64; 9];
            if is_f32 {
                for i in 0..9 {
                    vals[i] = read_implicit_real_f32(implicit_data, attr_info.offset + i as u32)?
                        as f64;
                }
            } else {
                for i in 0..9 {
                    vals[i] =
                        read_implicit_real_f64(implicit_data, attr_info.offset + i as u32 * 2)?;
                }
            }
            Ok(AttrValue::Orientation(vals))
        }
        _ => {
            let byte_offset = attr_info.offset as usize * 4;
            let end = (byte_offset + 4).min(implicit_data.len());
            Ok(AttrValue::Raw(implicit_data[byte_offset..end].to_vec()))
        }
    }
}
