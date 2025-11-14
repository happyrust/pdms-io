use crate::watch::PdmsWatcher;

pub async fn sync_e3d_files() {
    let ip_dst = "http://50c170h624.zicp.vip:56785";
    //get db file header info from remote server
    //直接下载数据文件，而不是每次都扫描，通过json文件来完成
    let _db_headers = get_remote_db_headers(ip_dst).await;
}

async fn get_remote_db_headers(ip: &str) {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/watchers.json", ip))
        .send()
        .await
        .unwrap();
    //得到resp，然后反序列化
    let remote_db_headers: PdmsWatcher = resp.json().await.unwrap();
    println!("db headers: {:#?}", remote_db_headers);
}
