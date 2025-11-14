// pub fn get_parsed_data<'a, T: Clone + Debug + DekuRead<'a> + DekuWrite + TryFrom<&'a [u8]>>(file: &mut File, start: u64) -> anyhow::Result<T>{
//
//     let mut data = vec![];
//     data.resize(size_of::<T>(), 0u8);
//     file.seek(SeekFrom::Start(start))?;
//     file.read_exact(&mut data)?;
//     //println!("{:#04X?}", &ses_start_part);
//     T::try_from(data.as_ref()).map_err(|_| anyhow!("not ok"))
// }
