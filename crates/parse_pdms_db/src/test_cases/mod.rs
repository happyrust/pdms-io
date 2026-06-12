pub fn convert_str_to_bytes(data_str: &str) -> Vec<u8> {
    data_str
        .trim()
        .split_whitespace()
        .map(|s| u8::from_str_radix(s, 16).unwrap())
        .collect()
}

mod test_parse_uda;

mod test_parse_expr;

mod test_chinese;
mod test_data_new;
mod test_expression;
mod test_nom;
mod test_parse_ams;
mod test_parse_catalogue;
mod test_parse_children;
mod test_parse_element;
mod test_uda;

mod binary_data_parser_test;
mod test_ams5054_ptca;
mod test_ams7330;
mod test_amssys;
mod test_axis_explicit_trunc;
mod test_parse_string;
// mod expression_test_utils; // 暂时禁用
mod test_attlib_diag;
mod test_attlib_noun_query;
mod test_collect_explict;
