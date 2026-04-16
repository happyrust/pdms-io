/// 元素引用号 (8 字节双分量)
///
/// 对齐 core.dll 的 RefNo 结构：refno_0 (高32位) + refno_1 (低32位)。
/// B-树索引和元素定位的核心 key。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RefNo {
    pub hi: u32,
    pub lo: u32,
}

impl RefNo {
    pub fn new(hi: u32, lo: u32) -> Self {
        Self { hi, lo }
    }

    pub fn from_u64(v: u64) -> Self {
        Self {
            hi: (v >> 32) as u32,
            lo: v as u32,
        }
    }

    pub fn to_u64(self) -> u64 {
        ((self.hi as u64) << 32) | (self.lo as u64)
    }

    pub fn from_be_bytes(bytes: &[u8]) -> Self {
        assert!(bytes.len() >= 8);
        let hi = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let lo = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        Self { hi, lo }
    }

    pub fn to_be_bytes(self) -> [u8; 8] {
        let mut out = [0u8; 8];
        out[..4].copy_from_slice(&self.hi.to_be_bytes());
        out[4..].copy_from_slice(&self.lo.to_be_bytes());
        out
    }

    pub fn is_null(self) -> bool {
        self.hi == 0 && self.lo == 0
    }
}

impl std::fmt::Display for RefNo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RefNo({:#010X}:{:#010X})", self.hi, self.lo)
    }
}

impl From<u64> for RefNo {
    fn from(v: u64) -> Self {
        Self::from_u64(v)
    }
}

impl From<RefNo> for u64 {
    fn from(r: RefNo) -> u64 {
        r.to_u64()
    }
}
