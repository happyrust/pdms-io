#[cfg(test)]
mod tests {
    use crate::defines::*;
    use crate::element_serializer::EleSerializer;
    use crate::io::PdmsIO;
    use crate::writer::{DataPageWriter, DatabaseWriter, ElementWriter};
    use aios_core::RefU64;
    use std::env;
    use std::fs;
    use std::fs::OpenOptions;
    use std::io::Write;

    #[test]
    fn test_end_to_end_write() {
        let temp_dir = env::temp_dir();
        let file_path = temp_dir.join("test_write_integration_db.db");
        let _ = fs::remove_file(&file_path);

        // 1. 初始化一个极简的数据库文件 (至少包含头和索引页)
        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&file_path)
                .expect("无法创建临时文件");

            // 写入 512 字节的全零数据作为文件头 (实际生产中需要正确的头)
            let mut header = vec![0u8; 512];
            header[0x04..0x08].copy_from_slice(&2u32.to_be_bytes()); // version
            header[0x08..0x0C].copy_from_slice(&1u32.to_be_bytes()); // db_num / ext_no
            header[0x2C..0x30].copy_from_slice(&1u32.to_be_bytes()); // ext_no
            // 设置一些基础字段，比如页面大小
            header[0x34..0x38].copy_from_slice(&512u32.to_be_bytes()); // page_size
            file.write_all(&header).expect("写入头失败");

            // 写入一个空的索引页面 (Page 1)
            let mut index_page = vec![0u8; 512];
            index_page[0..4].copy_from_slice(&8u32.to_be_bytes()); // Index type
            index_page[4..8].copy_from_slice(&0xCC47DFu32.to_be_bytes()); // Noun
            file.write_all(&index_page).expect("写入索引页失败");
        }

        // 2. 使用 DatabaseWriter 进行写入
        let mut writer = DatabaseWriter::new_512();
        writer.begin_session(101);
        // 读取侧 IndexPageData 目前以首个 u32=0 视作终止；测试需使用高 32 位非 0 的 refno。
        let refno = ((0x1234u64) << 32) | 0x5678_9ABCu64;
        let element_header = EleSerializer::serialize_element_header(6, refno, 10, 0);
        let members = EleSerializer::serialize_members(refno, &[((0x4321u64) << 32) | 0x1000u64]);
        let element_data = [
            element_header.as_slice(),
            members.as_slice(),
            [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07].as_slice(),
        ]
        .concat();
        let expected_offset: u64;

        {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&file_path)
                .expect("无法打开临时文件");

            // 使用 DataPageWriter 分配数据页并写入元素
            // 假设我们从页 2 开始写数据
            let mut dp_writer = DataPageWriter::new_512(2);
            let loc = dp_writer
                .write_element(&element_data)
                .expect("写入元素失败");

            // 将生产的数据页写入文件
            let pages = dp_writer.finish();
            for (pgno, data) in pages {
                writer
                    .element_writer
                    .write_page(&mut file, 1, pgno, &data)
                    .expect("写入数据页失败");
            }

            // 更新索引
            let index_loc = ElementWriter::create_refno_loc(
                refno,
                loc.start.page_no,
                (loc.start.offset / 2) as u32,
                0,
            );
            expected_offset =
                loc.start.page_no as u64 * PAGE_SIZE_512 as u64 + loc.start.offset as u64;
            writer
                .element_writer
                .insert_index_entry(&mut file, 1, 1, &index_loc)
                .expect("插入索引失败");

            // 提交会话
            writer
                .commit_session(&mut file, 0, Some("TEST-PC"), Some("Integration Test"))
                .expect("提交会话失败");
        }

        // 3. 验证写入结果
        let metadata = std::fs::metadata(&file_path).expect("读取元数据失败");
        assert!(metadata.len() >= 512 * 4);

        let mut io = PdmsIO::new("ams", &file_path, true);
        io.open().expect("PdmsIO 打开失败");
        io.init_ses_range_map().expect("初始化会话范围失败");
        assert_eq!(io.page_size, PAGE_SIZE_512);

        let refno_obj = RefU64::from_two_nums((refno >> 32) as u32, refno as u32);
        let (sesno, offset) = io
            .search_latest_refno(refno_obj, None)
            .expect("重开后应能检索到新写入 refno");
        assert_eq!(sesno, 101);
        assert_eq!(offset, expected_offset);

        let record = io
            .read_element_record_cached(offset)
            .expect("应能读回刚写入的元素记录");
        assert_eq!(record, element_data);
    }

    #[test]
    fn test_batch_write_elements() {
        let temp_dir = env::temp_dir();
        let file_path = temp_dir.join("test_batch_write_db.db");
        let _ = fs::remove_file(&file_path);

        // 1. 初始化数据库文件
        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&file_path)
                .expect("无法创建临时文件");

            let mut header = vec![0u8; 512];
            header[0x34..0x38].copy_from_slice(&512u32.to_be_bytes()); // page_size
            file.write_all(&header).expect("写入头失败");

            let mut index_page = vec![0u8; 512];
            index_page[0..4].copy_from_slice(&8u32.to_be_bytes()); // Index type
            index_page[4..8].copy_from_slice(&0xCC47DFu32.to_be_bytes()); // Noun
            file.write_all(&index_page).expect("写入索引页失败");
        }

        // 2. 批量写入
        let mut writer = DatabaseWriter::new_512();
        writer.begin_session(102);

        {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&file_path)
                .expect("无法打开临时文件");

            let mut dp_writer = DataPageWriter::new_512(2);

            for i in 1..=10 {
                let refno = 1000 + i as u64;
                let element_data = EleSerializer::serialize_element_header(6, refno, 10, 0);
                let loc = dp_writer
                    .write_element(&element_data)
                    .expect("写入元素失败");

                let index_loc = ElementWriter::create_refno_loc(
                    refno,
                    loc.start.page_no,
                    (loc.start.offset / 2) as u32,
                    0,
                );
                writer
                    .element_writer
                    .insert_index_entry(&mut file, 1, 1, &index_loc)
                    .expect("插入索引失败");
            }

            let pages = dp_writer.finish();
            for (pgno, data) in pages {
                writer
                    .element_writer
                    .write_page(&mut file, 1, pgno, &data)
                    .expect("写入数据页失败");
            }

            writer
                .commit_session(&mut file, 0, Some("BATCH-PC"), Some("Batch Write Test"))
                .expect("提交会话失败");
        }

        // 3. 验证
        let metadata = std::fs::metadata(&file_path).expect("读取元数据失败");
        assert!(metadata.len() >= 512 * 4);
    }
}
