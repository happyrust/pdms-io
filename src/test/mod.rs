pub mod test_data;
pub mod test_data_with_members;


// 其余模块仅用于单元/集成测试；避免影响库本身的编译与发布。
#[cfg(test)]
pub mod test_parse;
#[cfg(test)]
pub mod test_max_att_version;
#[cfg(test)]
pub mod test_parse_ele;
#[cfg(test)]
pub mod test_ses_data;
#[cfg(test)]
pub mod test_history_data;
#[cfg(test)]
pub mod test_refno_status;
#[cfg(test)]
pub mod test_collect_latest_eles;
#[cfg(test)]
pub mod test_write_integration;
#[cfg(test)]
pub mod test_ses_validate;
#[cfg(test)]
pub mod expression_test_utils;
#[cfg(test)]
pub mod test_case_loader;
#[cfg(test)]
pub mod test_desp_attribute;
