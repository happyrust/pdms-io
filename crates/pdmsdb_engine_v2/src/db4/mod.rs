pub mod attrs;
pub mod ce;
pub mod element;
pub mod explicit_attrs;
pub mod page_layout;
pub mod refs;
mod record_reader;
mod record_writer;

pub use attrs::{AttrInfo, AttrType, AttrValue};
pub use ce::{CurrentElement, ElementHandle};
pub use element::ElementBuilder;
pub use explicit_attrs::{ExplicitBlock, parse_explicit_blocks};
pub use page_layout::ElementRecordView;
pub use refs::{ElementRefs, parse_member_refs};
pub use record_reader::{RecordReaderV2, read_record_from_loc};
pub use record_writer::{
    DATA_PAGE_HEADER_SIZE, DATA_PAGE_TYPE, DataPageBuilderV2, MAIN_DATA_SUBTYPE, RecordWriterV2,
    SPECIAL_PAGE_TYPE, SPECIAL_SEGMENT_HEADER_SIZE, write_record,
};
