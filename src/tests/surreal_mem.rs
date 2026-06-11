//! specs/003 T101——kv-mem 测试基建(契约 D 系列的验收地基)。
//!
//! - `SUL_DB` 为 `Surreal<Any>` 全局单例:进程内**仅连接一次** `mem://`
//!   (kv-mem 内嵌引擎,全离线;连接串决定后端,生产 ws 路径不受影响)。
//! - 全局单例意味着 ns/db 是**会话级**状态:并发测试切换 ns 会互相串库,
//!   故 [`isolated`] 返回持有的串行锁 + 每测试独立 db——锁存活期 = 测试体。
//! - SurrealDB schemaless,表无需 DEFINE;幂等由确定性记录 ID + upsert 承担(契约 D2)。

use aios_core::{SUL_DB, use_ns_db_compat};
use surrealdb_types::SurrealValue;
use tokio::sync::{Mutex, MutexGuard, OnceCell};

static MEM_INIT: OnceCell<()> = OnceCell::const_new();
static TEST_LOCK: Mutex<()> = Mutex::const_new(());

/// 003 测试共享运行时:`SUL_DB`(全局单例)的 mem 引擎后台任务随首个连接所在
/// 运行时存亡——若用 `#[tokio::test]` 每测一个运行时,首测结束即拖死连接
/// (实测 "sending into a closed channel")。故所有 surreal 测试共用本静态运行时。
pub fn rt() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("build shared test runtime")
    })
}

/// 确保 `SUL_DB` 已连接 kv-mem 内嵌引擎(进程内一次)。
async fn ensure_mem_connected() {
    MEM_INIT
        .get_or_init(|| async {
            SUL_DB
                .connect("mem://")
                .await
                .expect("connect mem:// (requires surrealdb kv-mem feature)");
        })
        .await;
}

/// 进入隔离的测试数据库:串行锁 + `t003`/`<test_name>` 独立 ns/db。
/// 返回的 guard 必须存活到测试体结束(`let _g = isolated("...").await;`)。
pub async fn isolated(test_name: &str) -> MutexGuard<'static, ()> {
    let guard = TEST_LOCK.lock().await;
    ensure_mem_connected().await;
    use_ns_db_compat(&SUL_DB, "t003", test_name)
        .await
        .expect("switch isolated ns/db");
    guard
}

/// `SELECT count() FROM <table> GROUP ALL` 的便捷断言素材。
/// SurrealDB 3.x 对不存在的表报 NotFound(而非空集)——按 0 处理。
pub async fn table_count(table: &str) -> i64 {
    #[derive(surrealdb_types::SurrealValue)]
    struct Row {
        count: i64,
    }
    let res = SUL_DB.query(format!("SELECT count() FROM {table} GROUP ALL")).await;
    let mut res = match res {
        Ok(r) => r,
        Err(e) if e.to_string().contains("does not exist") => return 0,
        Err(e) => panic!("count query failed: {e}"),
    };
    match res.take::<Option<Row>>(0) {
        Ok(row) => row.map(|r| r.count).unwrap_or(0),
        Err(e) if e.to_string().contains("does not exist") => 0,
        Err(e) => panic!("count take failed: {e}"),
    }
}

// ---------------------------------------------------------------------------
// T102——幂等冒烟(合成;契约 D2 I1/I2 最小例)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(surrealdb_types::SurrealValue, Clone, Debug, PartialEq)]
    struct PeSesH {
        refno_0: u32,
        refno_1: u32,
        sesno: u32,
        offset: u64,
        dbnum: i32,
    }

    /// D2 I1/I2:确定性 ID 的 upsert 重放 ≥3 次——count 恒 1、内容与末次一致。
    /// (共享静态运行时:单线程 rt 卡死、每测私有 rt 拖死全局连接,见 `rt()` 注释)
    #[test]
    fn kv_mem_upsert_replay_is_idempotent() {
        rt().block_on(kv_mem_upsert_replay_is_idempotent_inner());
    }

    async fn kv_mem_upsert_replay_is_idempotent_inner() {
        let _g = isolated("upsert_replay").await;

        let rec =
            PeSesH { refno_0: 0x5C20, refno_1: 0x3D8F, sesno: 36, offset: 0x1234, dbnum: 7200 };
        // 确定性 ID(冒烟用字符串形;正式 ID 形态在 T201 按契约 D1/D2 落)
        let id = format!("{}_{}_{}_{}", rec.dbnum, rec.refno_0, rec.refno_1, rec.sesno);

        for round in 0..3 {
            let stored: Option<PeSesH> = SUL_DB
                .upsert(("pe_ses_h", id.as_str()))
                .content(rec.clone())
                .await
                .expect("upsert pe_ses_h");
            assert_eq!(stored.as_ref(), Some(&rec), "round {round}: upsert returns content");
        }

        assert_eq!(table_count("pe_ses_h").await, 1, "replay must not duplicate");

        // 同 ID 内容更新 = 覆盖(upsert 语义,非 INSERT IGNORE 的跳过)
        let newer = PeSesH { offset: 0x5678, ..rec.clone() };
        let _: Option<PeSesH> = SUL_DB
            .upsert(("pe_ses_h", id.as_str()))
            .content(newer.clone())
            .await
            .expect("upsert newer");
        assert_eq!(table_count("pe_ses_h").await, 1);
        let read: Option<PeSesH> =
            SUL_DB.select(("pe_ses_h", id.as_str())).await.expect("select back");
        assert_eq!(read, Some(newer), "upsert must overwrite, not ignore");
    }

    /// ns/db 隔离自检:另一隔离库中看不到上个测试的表数据。
    #[test]
    fn kv_mem_isolation_between_test_dbs() {
        rt().block_on(async {
            let _g = isolated("isolation_probe").await;
            assert_eq!(table_count("pe_ses_h").await, 0, "fresh db must be empty");
        });
    }
}
