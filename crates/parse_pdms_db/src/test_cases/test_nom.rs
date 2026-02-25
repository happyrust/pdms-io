use crate::test_cases::convert_str_to_bytes;
use aios_core::tool::db_tool::db1_dehash;
use nom::IResult;
use nom::Parser;
use nom::bytes::complete::tag;
use nom::combinator::{map, verify};
use nom::multi::many_till;
use nom::number::complete::be_i32;

fn parser(s: &[u8]) -> IResult<&[u8], (Vec<i32>, &[u8])> {
    many_till(map(be_i32, |x| x), tag(&[0x0, 0x0, 0x0, 0x7][..])).parse(s)
}

#[test]
fn test_noun() {
    // dbg!(db1_dehash(convert_to_hash(&[0xFF, 0xFF, 0xFF, 0xFB])));
    dbg!(db1_dehash(u32::from_be_bytes([0x0, 0xB, 0x20, 0x9F])));
    dbg!(db1_dehash(u32::from_be_bytes([0x0, 0xC, 0xD2, 0x42])));
    dbg!(db1_dehash(u32::from_be_bytes([0x0, 0xD, 0xDF, 0x8A])));
}

#[test]
pub fn test_take_till() {
    let test_data = "\
    00 00 00 00 00 00 00 07 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 01";
    let data = convert_str_to_bytes(test_data);
    // let s = parser(data.as_slice());
    let _s: IResult<&[u8], (Vec<i32>, i32)> =
        many_till(verify(be_i32, |x| *x == 0), verify(be_i32, |x| *x == 7)).parse(data.as_slice());
    // let s: IResult<&[u8], (Vec<bool>, &[u8])>  = pmany_till(map(be_i32, |x| x == 0), tag([0x0, 0x0, 0x0, 0x7]))(data.as_slice());
    // let s: IResult<&[u8], &[u8]> = take_until( map(be_i32, |x| x != 0))(data.as_slice());
    //dbg!(s);
    // many_till( tag( "ab", () ), tag("ef", ()))("ababefg");
}
