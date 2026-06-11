//! specs/004 —— SurrealDB → E3D 写回核心(T101 `EditOp` / T103 纯函数入口 / T104 文件包装)。
//!
//! 契约 `specs/004-surreal-e3d-writeback/contracts/writeback-contract.md`:
//! - E1:六原语强类型,refno 第一寻址(无名元素可编辑;经 e3d_io 决策 A 薄变体);
//!   `InlineValue` 禁裸字节;serde + `schema_version`(E1-I2,队列 payload 同源)。
//! - E3-A1:一批 = 单新会话(`EdbWriter::batch` 原子);任一笔失败整批回滚、无输出。
//! - E3-A2:返回前 `verify_commit` 强制通过(结构四类校验)+ 逐笔 refno 读回核验。
//! - E3-A3:纯函数——本模块核心不触盘、不连库;文件包装在 [`apply_writeback_file`],
//!   默认写副本(原文件零字节变化),in-place 须显式确认(E3-A5,宪法 IV)。
//! - E1 Delete:`force=false` 时 `delete_guards`(HasMembers/Referenced)必拦;
//!   `force=true` 仍受 verify 兜底(删有子父 ⇒ DanglingRef ⇒ 整体失败)。
//!
//! 默认特性可用(不依赖 surrealdb);队列层(Phase 2)在其上喂 `EditOp`。

use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow, bail};
use e3d_io::{
    Edb, EdbWriter, SchemaSet, Val, delete_guards, element_diff, verify_commit,
};
use serde::{Deserialize, Serialize};

/// `EditOp` 载荷格式版本(契约 E1-I2;队列 payload 与内存类型同源)。
pub const EDITOP_SCHEMA_VERSION: u32 = 1;

/// 定长内联值(契约 E1/R3:禁裸字节;与 `e3d_io::Val` 的 inline 子集一一对应)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InlineValue {
    Reals(Vec<f64>),
    Ints(Vec<u32>),
    Refs(Vec<(u32, u32)>),
}

impl InlineValue {
    fn to_val(&self) -> Val {
        match self {
            InlineValue::Reals(v) => Val::Reals(v.clone()),
            InlineValue::Ints(v) => Val::Ints(v.clone()),
            InlineValue::Refs(v) => Val::Refs(v.clone()),
        }
    }
}

/// 强类型编辑意图(契约 E1 六原语 + specs/005 增补 SetName;refno 第一寻址)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EditOp {
    /// NAME 条目改写(目标须已有 NAME;严格同构 e3d_io rename 语义)。
    Rename { refno: (u32, u32), new_name: String },
    /// NAME 设置(specs/005 T301,契约 F3):无 NAME 则新增(含 DA 首链创建,
    /// 无名元素首次命名的安全入口),已有则改写。
    SetName { refno: (u32, u32), name: String },
    /// POS 实数三元组。
    SetPos { refno: (u32, u32), pos: [f64; 3] },
    /// 任意定长内联属性(real/int/ref,数量不变)。
    SetInline { refno: (u32, u32), attr_hash: u32, value: InlineValue },
    /// 成员列表全量替换。
    SetMembers { refno: (u32, u32), children: Vec<(u32, u32)> },
    /// 删除主记录;`force=false` 时护栏(有子/被引用)必拦。
    Delete { refno: (u32, u32), force: bool },
    /// 克隆既有元素到新 refno(该 dbno 最大 refseq+1,自动分配并回带)。
    InsertClone { template_refno: (u32, u32), new_name: String },
}

impl EditOp {
    fn kind(&self) -> &'static str {
        match self {
            EditOp::Rename { .. } => "rename",
            EditOp::SetName { .. } => "set_name",
            EditOp::SetPos { .. } => "set_pos",
            EditOp::SetInline { .. } => "set_inline",
            EditOp::SetMembers { .. } => "set_members",
            EditOp::Delete { .. } => "delete",
            EditOp::InsertClone { .. } => "insert_clone",
        }
    }

    fn target(&self) -> (u32, u32) {
        match self {
            EditOp::Rename { refno, .. }
            | EditOp::SetName { refno, .. }
            | EditOp::SetPos { refno, .. }
            | EditOp::SetInline { refno, .. }
            | EditOp::SetMembers { refno, .. }
            | EditOp::Delete { refno, .. } => *refno,
            EditOp::InsertClone { template_refno, .. } => *template_refno,
        }
    }
}

/// 一批编辑 + 格式版本(队列行 `edits` 字段的序列化形态,E1-I2)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditBatch {
    pub schema_version: u32,
    pub edits: Vec<EditOp>,
}

impl EditBatch {
    pub fn new(edits: Vec<EditOp>) -> Self {
        EditBatch { schema_version: EDITOP_SCHEMA_VERSION, edits }
    }
}

/// 逐笔结果(InsertClone 回带新 refno,契约 E3-A4/R2)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditResult {
    pub kind: String,
    pub refno: (u32, u32),
    pub new_refno: Option<(u32, u32)>,
}

/// 元素级 diff 摘要(可序列化,供队列回执;明细 `Diff` 不出模块)。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DiffSummary {
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
}

/// 写回报告(契约 E3-A4)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WritebackReport {
    pub new_sesno: u32,
    pub results: Vec<EditResult>,
    pub diff: DiffSummary,
}

/// 写回核心(契约 E3-A1~A4;纯函数:输入字节 + 编辑 → 输出字节 + 报告)。
///
/// 失败语义:任一笔编辑失败/护栏拦截/verify 不过 ⇒ `Err`,**无输出字节**。
pub fn apply_writeback(
    db_bytes: Vec<u8>,
    ss: &SchemaSet,
    edits: &[EditOp],
) -> anyhow::Result<(Vec<u8>, WritebackReport)> {
    if edits.is_empty() {
        bail!("empty edit batch");
    }
    let orig = db_bytes.clone();
    let mut w = EdbWriter::from_bytes(db_bytes, ss);
    let mut results: Vec<EditResult> = Vec::with_capacity(edits.len());

    let new_sesno = w
        .batch(|w| {
            for op in edits {
                let mut new_refno = None;
                match op {
                    EditOp::Rename { refno, new_name } => {
                        w.rename_at(*refno, new_name)?;
                    }
                    EditOp::SetName { refno, name } => {
                        w.set_name_at(*refno, name)?;
                    }
                    EditOp::SetPos { refno, pos } => {
                        w.set_pos_at(*refno, *pos)?;
                    }
                    EditOp::SetInline { refno, attr_hash, value } => {
                        w.set_inline_at(*refno, *attr_hash, &value.to_val())?;
                    }
                    EditOp::SetMembers { refno, children } => {
                        w.set_members_at(*refno, children)?;
                    }
                    EditOp::Delete { refno, force } => {
                        if !force {
                            let guards = delete_guards(w.db(), ss, *refno);
                            if !guards.is_empty() {
                                return Err(e3d_io::E3dError::Write(format!(
                                    "delete ({:#x},{:#x}) blocked by guards: {guards:?} (use force)",
                                    refno.0, refno.1
                                )));
                            }
                        }
                        w.delete_at(*refno)?;
                    }
                    EditOp::InsertClone { template_refno, new_name } => {
                        let (_, refno) = w.insert_clone_at(*template_refno, new_name)?;
                        new_refno = Some(refno);
                    }
                }
                results.push(EditResult {
                    kind: op.kind().to_string(),
                    refno: op.target(),
                    new_refno,
                });
            }
            Ok(())
        })
        .map_err(|e| anyhow!("writeback batch failed (rolled back): {e}"))?;

    // E3-A2:结构四类校验强制(B 树不变式 / COW 不可变 / append-only / owner 引用不悬挂)。
    verify_commit(&orig, w.db(), ss, &[])
        .map_err(|issues| anyhow!("verify_commit rejected the writeback: {issues:?}"))?;

    // 逐笔 refno 读回核验(Expect 为 name 导向,无名目标在此以 refno 路径补强)。
    for op in edits {
        match op {
            EditOp::SetPos { refno, pos } => {
                let e = w.element_at(*refno).map_err(|e| anyhow!("readback {refno:?}: {e}"))?;
                if e.pos() != Some(&pos[..]) {
                    bail!("readback mismatch: POS of {refno:?} = {:?}, want {pos:?}", e.pos());
                }
            }
            EditOp::Rename { refno, new_name } => {
                let e = w.element_at(*refno).map_err(|e| anyhow!("readback {refno:?}: {e}"))?;
                if e.name.as_deref() != Some(new_name.as_str()) {
                    bail!("readback mismatch: NAME of {refno:?} = {:?}, want {new_name}", e.name);
                }
            }
            EditOp::SetName { refno, name } => {
                let e = w.element_at(*refno).map_err(|e| anyhow!("readback {refno:?}: {e}"))?;
                if e.name.as_deref() != Some(name.as_str()) {
                    bail!("readback mismatch: NAME of {refno:?} = {:?}, want {name}", e.name);
                }
            }
            EditOp::Delete { refno, .. } => {
                if w.element_at(*refno).is_ok() {
                    bail!("readback mismatch: {refno:?} still present after delete");
                }
            }
            _ => {} // SetInline/SetMembers/InsertClone 由 verify + diff 间接覆盖
        }
    }

    let diff = element_diff(&Edb::from_bytes(orig), w.db(), ss);
    let report = WritebackReport {
        new_sesno,
        results,
        diff: DiffSummary {
            added: diff.added.len(),
            removed: diff.removed.len(),
            modified: diff.modified.len(),
        },
    };
    Ok((w.into_bytes(), report))
}

/// 落盘模式(契约 E3-A5;宪法 IV)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    /// 默认:写副本 `<db>.e3dout`,原文件零字节变化。
    Copy,
    /// 显式 in-place 覆盖;`confirmed=false` 直接拒绝(二次确认由调用方/CLI 收集)。
    InPlace { confirmed: bool },
}

/// 文件包装:读文件 → [`apply_writeback`] → 按模式落盘。返回 (输出路径, 报告)。
pub fn apply_writeback_file(
    db_path: impl AsRef<Path>,
    ss: &SchemaSet,
    edits: &[EditOp],
    mode: WriteMode,
) -> anyhow::Result<(PathBuf, WritebackReport)> {
    let db_path = db_path.as_ref();
    if let WriteMode::InPlace { confirmed: false } = mode {
        bail!("in-place writeback requires explicit confirmation (E3-A5)");
    }
    let bytes = std::fs::read(db_path).with_context(|| format!("read {}", db_path.display()))?;
    let (out_bytes, report) = apply_writeback(bytes, ss, edits)?;
    let out_path = match mode {
        WriteMode::Copy => {
            let mut p = db_path.as_os_str().to_owned();
            p.push(".e3dout");
            PathBuf::from(p)
        }
        WriteMode::InPlace { .. } => db_path.to_path_buf(),
    };
    std::fs::write(&out_path, &out_bytes)
        .with_context(|| format!("write {}", out_path.display()))?;
    Ok((out_path, report))
}

// ---------------------------------------------------------------------------
// Tests(specs/004 T105;sam7200,缺样本优雅跳过)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use e3d_io::index_db;
    use std::collections::HashMap;

    const EXE: &str = r"D:\AVEVA\Everything3D2.10";
    const DBF: &str = r"D:\work\plant\pdms-io\pdms-test-data\sam7200_0001";

    fn data_present() -> bool {
        std::path::Path::new(&format!(r"{}\desvir.dat", EXE)).exists()
            && std::path::Path::new(DBF).exists()
    }

    fn load() -> (SchemaSet, Vec<u8>) {
        (SchemaSet::load(EXE), std::fs::read(DBF).unwrap())
    }

    /// SC-001/SC-002 前半:六原语混合一批 → 单新会话,逐项读回一致(含无名元素)。
    #[test]
    fn writeback_six_primitives_single_session_roundtrip() {
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let (ss, bytes) = load();
        let db0 = Edb::from_bytes(bytes.clone());
        let mut rm = HashMap::new();
        let elems = index_db(&db0, &ss, true, &mut rm);

        let named = elems.iter().find(|e| e.name.as_deref() == Some("/WB1")).unwrap().refno;
        let unnamed = elems
            .iter()
            .find(|e| e.name.is_none() && e.pos().is_some())
            .expect("unnamed with POS")
            .refno;
        let unnamed2 = elems
            .iter()
            .filter(|e| e.name.is_none() && e.pos().is_some())
            .nth(1)
            .expect("second unnamed with POS")
            .refno;
        let parent = elems
            .iter()
            .find(|p| {
                p.name.is_some() && {
                    let kids: Vec<_> = elems.iter().filter(|c| c.owner == p.refno).collect();
                    kids.len() >= 2
                }
            })
            .expect("a named parent with >=2 children")
            .refno;
        let children: Vec<(u32, u32)> =
            elems.iter().filter(|c| c.owner == parent).map(|c| c.refno).collect();

        // 批内顺序语义(契约 E3-A1 注):克隆置于其模板被编辑**之前**——
        // 同批先改模板再克隆会触发 e3d_io 克隆的 DA 布局前置条件拒绝(实测边界)。
        let edits = vec![
            EditOp::InsertClone { template_refno: named, new_name: "/WBCORE-CLONE".to_string() },
            EditOp::SetPos { refno: unnamed, pos: [11.0, 22.0, 33.5] },
            EditOp::SetInline {
                refno: unnamed2,
                attr_hash: e3d_io::POS_HASH,
                value: InlineValue::Reals(vec![44.0, 55.0, 66.5]),
            },
            EditOp::Rename { refno: named, new_name: "/WB1-WBCORE".to_string() },
            EditOp::SetMembers { refno: parent, children: children.clone() },
        ];

        let (out, report) = apply_writeback(bytes.clone(), &ss, &edits).unwrap();
        assert_eq!(report.results.len(), edits.len());
        let clone_refno = report.results[0].new_refno.expect("clone refno");

        // 读回(独立重载输出字节,模拟下游消费)。
        let db1 = Edb::from_bytes(out.clone());
        let mut rm1 = HashMap::new();
        let elems1 = index_db(&db1, &ss, true, &mut rm1);
        let by_refno = |r: (u32, u32)| elems1.iter().find(|e| e.refno == r);

        assert_eq!(by_refno(unnamed).unwrap().pos(), Some(&[11.0, 22.0, 33.5][..]));
        assert_eq!(by_refno(unnamed2).unwrap().pos(), Some(&[44.0, 55.0, 66.5][..]));
        assert_eq!(by_refno(named).unwrap().name.as_deref(), Some("/WB1-WBCORE"));
        let kids1: Vec<(u32, u32)> =
            elems1.iter().filter(|c| c.owner == parent).map(|c| c.refno).collect();
        assert_eq!(kids1, children, "members preserved");
        assert_eq!(by_refno(clone_refno).unwrap().name.as_deref(), Some("/WBCORE-CLONE"));

        // 单新会话:输出比输入恰多一个会话根。
        assert_eq!(
            e3d_io::session_roots(&db1).len(),
            e3d_io::session_roots(&db0).len() + 1,
            "exactly one new session"
        );
        assert!(report.diff.modified >= 2 && report.diff.added >= 1);

        // 第二批:删除克隆(无子无引用,force=false 应直通)。
        let (out2, _r2) = apply_writeback(
            out,
            &ss,
            &[EditOp::Delete { refno: clone_refno, force: false }],
        )
        .unwrap();
        let db2 = Edb::from_bytes(out2);
        let mut rm2 = HashMap::new();
        assert!(
            index_db(&db2, &ss, true, &mut rm2).iter().all(|e| e.refno != clone_refno),
            "clone gone after delete"
        );
    }

    /// SC-002 后半:坏批(不存在的 refno)⇒ Err 且无输出(纯函数无副作用可断言:Err 即无字节)。
    #[test]
    fn writeback_bad_batch_yields_no_output() {
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let (ss, bytes) = load();
        let r = apply_writeback(
            bytes,
            &ss,
            &[
                EditOp::SetPos { refno: (0x5C20, 1), pos: [1.0, 2.0, 3.0] },
                EditOp::Delete { refno: (0x5C20, 0xFFFF_FFF0), force: true },
            ],
        );
        assert!(r.is_err(), "bad batch must fail as a whole");
    }

    /// SC-003:护栏与 verify 双闸——force=false 删有子父被护栏拦;force=true 被 verify
    /// (DanglingRef)拦;两路均零输出。
    #[test]
    fn writeback_guards_and_verify_block_unsafe_delete() {
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let (ss, bytes) = load();
        let db0 = Edb::from_bytes(bytes.clone());
        let mut rm = HashMap::new();
        let elems = index_db(&db0, &ss, true, &mut rm);
        let parent = elems
            .iter()
            .find(|p| elems.iter().any(|c| c.owner == p.refno))
            .expect("a parent")
            .refno;

        let guarded =
            apply_writeback(bytes.clone(), &ss, &[EditOp::Delete { refno: parent, force: false }]);
        assert!(guarded.is_err(), "guards must block parent delete without force");
        assert!(format!("{:#}", guarded.unwrap_err()).contains("guards"));

        let forced =
            apply_writeback(bytes, &ss, &[EditOp::Delete { refno: parent, force: true }]);
        assert!(forced.is_err(), "verify must catch dangling owners on forced parent delete");
        assert!(format!("{:#}", forced.unwrap_err()).contains("verify"));
    }

    /// E3-A5:文件包装——默认副本(原文件零字节变化);未确认的 in-place 直接拒绝。
    #[test]
    fn writeback_file_copy_mode_and_inplace_guard() {
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let orig = std::fs::read(DBF).unwrap();

        // 用临时副本当"原文件",避免污染样本目录。
        let tmp = std::env::temp_dir().join(format!("wbcore_{}_sam7200", std::process::id()));
        std::fs::write(&tmp, &orig).unwrap();

        let db0 = Edb::from_bytes(orig.clone());
        let mut rm = HashMap::new();
        let unnamed = index_db(&db0, &ss, true, &mut rm)
            .iter()
            .find(|e| e.name.is_none() && e.pos().is_some())
            .unwrap()
            .refno;
        let edits = [EditOp::SetPos { refno: unnamed, pos: [7.0, 8.0, 9.5] }];

        assert!(
            apply_writeback_file(&tmp, &ss, &edits, WriteMode::InPlace { confirmed: false })
                .is_err(),
            "unconfirmed in-place must be rejected"
        );
        assert_eq!(std::fs::read(&tmp).unwrap(), orig, "source untouched after rejection");

        let (out_path, report) =
            apply_writeback_file(&tmp, &ss, &edits, WriteMode::Copy).unwrap();
        assert!(out_path.to_string_lossy().ends_with(".e3dout"));
        assert_eq!(std::fs::read(&tmp).unwrap(), orig, "copy mode leaves source byte-identical");
        assert!(report.new_sesno > 0);
        let out_bytes = std::fs::read(&out_path).unwrap();
        assert!(out_bytes.len() > orig.len(), "COW output must append");

        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_file(&out_path);
    }
}
