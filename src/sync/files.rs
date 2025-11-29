use crate::watch::PdmsWatcher;
use anyhow::{Context, Result};
use std::time::Duration;

pub async fn sync_e3d_files() -> Result<PdmsWatcher> {
    let ip_dst = "http://50c170h624.zicp.vip:56785";
    //get db file header info from remote server
    //直接下载数据文件，而不是每次都扫描，通过json文件来完成
    let db_headers = get_remote_db_headers(ip_dst).await?;

    Ok(db_headers)
}

async fn get_remote_db_headers(ip: &str) -> Result<PdmsWatcher> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("build reqwest client")?;
    let resp = client
        .get(format!("{}/watchers.json", ip))
        .send()
        .await
        .context("request watchers.json")?;
    //得到resp，然后反序列化
    let remote_db_headers: PdmsWatcher = resp
        .json()
        .await
        .context("deserialize watchers.json response")?;
    println!("db headers: {:#?}", remote_db_headers);

    Ok(remote_db_headers)
}
