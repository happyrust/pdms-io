use crate::core::RefNo;

pub fn parse_member_refs(members_data: &[u8]) -> Vec<RefNo> {
    if members_data.len() < 16 {
        return Vec::new();
    }

    let payload_start = 16;
    let payload = &members_data[payload_start..];
    let mut refs = Vec::new();
    let mut pos = 0;

    while pos + 8 <= payload.len() {
        let hi = u32::from_be_bytes(payload[pos..pos + 4].try_into().unwrap());
        let lo = u32::from_be_bytes(payload[pos + 4..pos + 8].try_into().unwrap());
        if hi == 0 && lo == 0 {
            break;
        }
        refs.push(RefNo::from_parts(hi, lo));
        pos += 8;
    }

    refs
}

#[derive(Debug, Clone)]
pub struct ElementRefs {
    pub owner: RefNo,
    pub children: Vec<RefNo>,
}

impl ElementRefs {
    pub fn new(owner: RefNo, children: Vec<RefNo>) -> Self {
        Self { owner, children }
    }

    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    pub fn add_member(&mut self, refno: RefNo) {
        if !self.children.contains(&refno) {
            self.children.push(refno);
        }
    }

    pub fn remove_member(&mut self, refno: RefNo) -> bool {
        if let Some(idx) = self.children.iter().position(|r| *r == refno) {
            self.children.remove(idx);
            true
        } else {
            false
        }
    }

    pub fn serialize_members_block(&self, self_ref: RefNo) -> Vec<u8> {
        if self.children.is_empty() {
            return Vec::new();
        }
        let payload_words = 3 + self.children.len() * 2;
        let total_words = payload_words + 1;

        let mut data = Vec::with_capacity(total_words * 4);
        data.push(0x00);
        data.push(0x02);
        data.extend_from_slice(&(total_words as u16).to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&self_ref.hi().to_be_bytes());
        data.extend_from_slice(&self_ref.lo().to_be_bytes());
        for member in &self.children {
            data.extend_from_slice(&member.hi().to_be_bytes());
            data.extend_from_slice(&member.lo().to_be_bytes());
        }
        data
    }
}
