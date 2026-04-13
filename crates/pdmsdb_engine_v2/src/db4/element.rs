use crate::core::{EngineError, RefNo};

pub const ELEMENT_HEADER_WORDS: usize = 6;

pub fn serialize_element_header(
    total_words: u32,
    refno: RefNo,
    noun_hash: u32,
    owner: RefNo,
) -> Vec<u8> {
    let mut header = Vec::with_capacity(ELEMENT_HEADER_WORDS * 4);
    header.extend_from_slice(&(total_words as i32).to_be_bytes());
    header.extend_from_slice(&refno.hi().to_be_bytes());
    header.extend_from_slice(&refno.lo().to_be_bytes());
    header.extend_from_slice(&noun_hash.to_be_bytes());
    header.extend_from_slice(&owner.hi().to_be_bytes());
    header.extend_from_slice(&owner.lo().to_be_bytes());
    header
}

pub fn serialize_terminator() -> [u8; 8] {
    let mut term = [0u8; 8];
    term[4..8].copy_from_slice(&7u32.to_be_bytes());
    term
}

pub struct ElementBuilder {
    refno: RefNo,
    noun_hash: u32,
    owner: RefNo,
    implicit_words: Vec<u32>,
    members: Vec<RefNo>,
    explicit_blocks: Vec<Vec<u8>>,
}

impl ElementBuilder {
    pub fn new(refno: RefNo, noun_hash: u32, owner: RefNo) -> Self {
        Self {
            refno,
            noun_hash,
            owner,
            implicit_words: Vec::new(),
            members: Vec::new(),
            explicit_blocks: Vec::new(),
        }
    }

    pub fn set_implicit_word(&mut self, word_offset: usize, value: u32) {
        if word_offset >= self.implicit_words.len() {
            self.implicit_words
                .resize(word_offset + 1, 0);
        }
        self.implicit_words[word_offset] = value;
    }

    pub fn set_implicit_i32(&mut self, word_offset: usize, value: i32) {
        self.set_implicit_word(word_offset, value as u32);
    }

    pub fn set_implicit_f64(&mut self, word_offset: usize, value: f64) {
        let bytes = value.to_be_bytes();
        let hi = u32::from_be_bytes(bytes[0..4].try_into().unwrap());
        let lo = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
        self.set_implicit_word(word_offset, hi);
        self.set_implicit_word(word_offset + 1, lo);
    }

    pub fn set_implicit_ref(&mut self, word_offset: usize, refno: RefNo) {
        self.set_implicit_word(word_offset, refno.hi());
        self.set_implicit_word(word_offset + 1, refno.lo());
    }

    pub fn add_member(&mut self, refno: RefNo) {
        self.members.push(refno);
    }

    pub fn add_explicit_block(&mut self, block: Vec<u8>) {
        self.explicit_blocks.push(block);
    }

    pub fn build(&self) -> Result<Vec<u8>, EngineError> {
        let implicit_data_words = ELEMENT_HEADER_WORDS + self.implicit_words.len();
        let mut record = serialize_element_header(
            implicit_data_words as u32,
            self.refno,
            self.noun_hash,
            self.owner,
        );

        for &word in &self.implicit_words {
            record.extend_from_slice(&word.to_be_bytes());
        }

        if !self.members.is_empty() {
            record.extend_from_slice(&self.serialize_members());
        }

        for block in &self.explicit_blocks {
            record.extend_from_slice(block);
        }

        record.extend_from_slice(&serialize_terminator());

        Ok(record)
    }

    fn serialize_members(&self) -> Vec<u8> {
        let payload_words = 3 + self.members.len() * 2;
        let total_words = payload_words + 1;

        let mut data = Vec::with_capacity(total_words * 4);

        data.push(0x00);
        data.push(0x02);
        data.extend_from_slice(&(total_words as u16).to_be_bytes());

        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&self.refno.hi().to_be_bytes());
        data.extend_from_slice(&self.refno.lo().to_be_bytes());

        for member in &self.members {
            data.extend_from_slice(&member.hi().to_be_bytes());
            data.extend_from_slice(&member.lo().to_be_bytes());
        }

        data
    }
}
