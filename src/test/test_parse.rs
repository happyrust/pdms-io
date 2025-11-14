use deku::prelude::*;
use std::convert::TryFrom;

#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
struct DekuTest {
    should_read: u8,
    #[deku(cond = "*should_read == 1", count = "*should_read * 3")]
    items: Option<Vec<u8>>,
}

#[test]
fn test_option() {
    let test_data: &[u8] = [0x01, 0x02, 0x03, 0x04].as_ref();

    let test_deku = DekuTest::try_from(test_data).unwrap();
    dbg!(&test_deku);

    assert_eq!(
        DekuTest {
            should_read: 1,
            items: Some(vec![0x02, 0x03, 0x04]),
        },
        test_deku
    );

    let test_deku: Vec<u8> = test_deku.try_into().unwrap();
    assert_eq!(test_data.to_vec(), test_deku);
}

#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
#[deku(type = "u8")]
pub enum DekuTestEnum {
    #[deku(id = "0")]
    A,
    #[deku(id_pat = "_")]
    B(#[deku(map = "|_: u8| -> Result<_, DekuError> { Ok(2) }")] u8),
}

#[test]
fn test_enum() {
    let test_data = vec![0, 1, 2];

    let (rest, deku_test) = DekuTestEnum::from_bytes((test_data.as_ref(), 0)).unwrap();
    assert_eq!(DekuTestEnum::A, deku_test);
    // 0 got consumed
    assert_eq!(rest.0, &[1, 2]);
    let output: Vec<u8> = deku_test.try_into().unwrap();
    assert_eq!(output, vec![0]);

    // ---

    let (rest, deku_test) = DekuTestEnum::from_bytes(rest).unwrap();
    assert_eq!(DekuTestEnum::B(2), deku_test);
    // 1 got consumed
    assert_eq!(rest.0, &[2]);
    let output: Vec<u8> = deku_test.try_into().unwrap();
    assert_eq!(output, vec![2]);

    // ---

    let (rest, deku_test) = DekuTestEnum::from_bytes(rest).unwrap();
    assert_eq!(DekuTestEnum::B(2), deku_test);
    // 2 got consumed
    // assert_eq!(rest.0, &[]);
    // let output: Vec<u8> = deku_test.try_into().unwrap();
    // assert_eq!(output, vec![2]);
}
