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
}
