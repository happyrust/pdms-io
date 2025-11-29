use crate::defines::{DbPageBasicInfo, PdmsHeader};
use crate::io::PdmsIO;
use dashmap::DashMap;
use futures::{
    channel::mpsc::{channel, Receiver},
    future::ok,
    SinkExt, StreamExt,
};
use indexmap::IndexMap;
use log::warn;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::path::PathBuf;
use walkdir::WalkDir;
// use dpcsync::chunker;
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;
// use crate::sync::compress::{CompressOptions, execute_compress};
// use crate::sync::sync::compress_archive;

#[test]
fn test_watch() {
    // let path = std::env::args()
    //     .nth(1)
    //     .expect("Argument 1 needs to be a path");
    // println!("watching {}", path);
    // let mut watch_files: Vec<PathBuf> = Vec::new();
    // watch_files.push(r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000"#.into());
    // let path = watch_files[0].clone();
    // //scan_dbs_version(path.clone());

    // futures::executor::block_on(async {
    //     if let Err(e) = async_watch(path).await {
    //         println!("error: {:?}", e)
    //     }return;
    // });
}

///文件的监控
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PdmsWatcher {
    pub watch_dirs: Vec<PathBuf>,
    #[serde(skip)]
    pub headers: DashMap<PathBuf, DbPageBasicInfo>,
    //还需要存储一下每个文件对应的完整目录
    #[serde(skip)]
    pub file_name_full_path_map: DashMap<String, PathBuf>,
}

impl PdmsWatcher {
    pub fn new<P: AsRef<Path>>(dirs: Vec<P>) -> Self {
        Self {
            watch_dirs: dirs.into_iter().map(|x| x.as_ref().to_path_buf()).collect(),
            headers: Default::default(),
            file_name_full_path_map: Default::default(),
        }
    }

    pub fn save(&self, path: Option<&str>) -> anyhow::Result<()> {
        let mut file = File::create(path.unwrap_or("watcher.json"))?;
        file.write_all(serde_json::to_string(self)?.as_bytes())?;
        Ok(())
    }

    pub fn get_dbno(&self, path: &PathBuf) -> Option<u32> {
        self.headers.get(path).map(|x| x.pdms_header.db_num as u32)
    }

    // pub fn load_from_json(path: Option<&str>) -> anyhow::Result<Self> {
    //     let mut file = File::open(path.unwrap_or("watcher.json"))?;
    //     let mut string = String::new();
    //     file.read_to_string(&mut string)?;
    //     let w = serde_json::from_str(string.as_str())?;
    //     Ok(w)
    // }

    pub async fn init_local_watcher(&self) -> anyhow::Result<()> {
        for watch_dir in &self.watch_dirs {
            let mut join_set = JoinSet::new();
            //在dir_entry 下创建 cbas目录
            let mut cbas_dir = watch_dir.clone();
            cbas_dir.push("cbas");
            let cbas_dir_path = cbas_dir.to_string_lossy().to_string();
            if !cbas_dir.exists() {
                std::fs::create_dir_all(cbas_dir)?;
            }
            for entry in WalkDir::new(watch_dir).sort_by(|a, b| {
                let a_len = a.path().metadata().map(|m| m.len()).unwrap_or(0);
                let b_len = b.path().metadata().map(|m| m.len()).unwrap_or(0);
                a_len.cmp(&b_len)
            }) {
                let dir_entry = match entry {
                    Ok(entry) => entry,
                    Err(err) => {
                        warn!("skip entry under {}: {}", watch_dir.display(), err);
                        continue;
                    }
                };
                let path = dir_entry.path();
                if path.is_dir() {
                    continue;
                }
                let Some(file_name) = path.file_stem().and_then(|s| s.to_str()) else {
                    warn!("skip path without valid file name: {}", path.display());
                    continue;
                };
                self.file_name_full_path_map
                    .insert(file_name.to_owned(), path.to_path_buf());
                let mut io = PdmsIO::new("ams", path, true);
                if let Err(err) = io.open() {
                    warn!("skip {}: open failed: {}", path.display(), err);
                    continue;
                };
                match io.get_page_basic_info() {
                    Ok(basic_info) => {
                        if let Some(old) = self.headers.get_mut(&path.to_path_buf()) {
                            //未发生修改，直接跳过
                            if old.pdms_header.latest_ses_pgno
                                == basic_info.pdms_header.latest_ses_pgno
                            {
                                continue;
                            }
                        }
                        self.headers.insert(path.to_path_buf(), basic_info);
                    }
                    Err(err) => {
                        warn!("skip {}: read page basic info failed: {}", path.display(), err);
                        continue;
                    }
                };

                //初始化CBA的Archive文件，来保证后续增量下载
                let input = path.to_path_buf();
                let output: PathBuf =
                    format!("{}/{}.cba", cbas_dir_path.as_str(), file_name).into();
                let tmp_path = cbas_dir_path.clone();
                join_set.spawn(async move {
                    // let compress_opt = CompressOptions::new(input, output, tmp_path.as_str());
                    // execute_compress(compress_opt).await.unwrap();
                });
            }
            while let Some(_) = join_set.join_next().await {}
        }

        anyhow::Ok(())
    }

    ///扫描出来每个db文件的 header信息
    pub fn scan_db_headers<P: AsRef<Path>>(
        paths: &Vec<P>,
    ) -> anyhow::Result<IndexMap<PathBuf, DbPageBasicInfo>> {
        let mut result = IndexMap::new();
        for path in paths {
            let mut io = PdmsIO::new("ams", path, true);
            io.open()?;
            let basic_info = io.get_page_basic_info()?;
            // println!("basic info: {:#4X?}", &basic_info);
            let _new_ses_no = basic_info.latest_ses_pageno + 1;
            result.insert(path.as_ref().to_path_buf(), basic_info);
        }

        Ok(result)
    }

    ///创建一个异步的watcher
    pub fn async_watcher() -> notify::Result<(RecommendedWatcher, Receiver<notify::Result<Event>>)>
    {
        let (mut tx, rx) = channel(1);

        // Automatically select the best implementation for your platform.
        // You can also access each implementation directly e.g. INotifyWatcher.
        let watcher = RecommendedWatcher::new(
            move |res| {
                futures::executor::block_on(async {
                    tx.send(res).await.unwrap();
                })
            },
            Config::default(),
        )?;

        Ok((watcher, rx))
    }
}
