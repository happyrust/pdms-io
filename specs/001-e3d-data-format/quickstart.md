# Quickstart: E3D 离线读写工具链

> 所有命令纯离线运行。示例数据:`pdms-test-data/sam7200_0001`(设计库)、`test-file/acp7002_0001`(目录库)、模式库默认在 AVEVA EXE 目录(`*vir.dat`)。

## Rust(单一真源:`crates/e3d_io`)

```bash
# 跑全部测试(读计数/POS、resolve_refs、S1–S8 写、设计库 + catalogue 库)
cd crates/e3d_io
cargo test --release            # 期望:20 passed

# 读:显示某元素的隐式+DA属性+解析引用
cargo run --bin e3d-io -- <exe_dir> <db> show /WB1
cargo run --bin e3d-io -- <exe_dir> <db> refs /WB1

# 写(默认写副本 <db>.e3dout;--inplace 显式覆盖)
cargo run --bin e3d-io -- <exe_dir> <db> rename /WB1 /WB1-NEW --out tmp
cargo run --bin e3d-io -- <exe_dir> <db> set-pos /WB1 9630 8072 5282.5
cargo run --bin e3d-io -- <exe_dir> <db> insert /WB1 /WB1-CLONE
cargo run --bin e3d-io -- <exe_dir> <db> delete /WB1-CLONE
```

## Rust(读取/导出 CLI:`tools/e3d_decode_rs`,消费 e3d_io)

```bash
cargo run --manifest-path tools/e3d_decode_rs/Cargo.toml --release -- \
  <exe_dir> pdms-test-data/sam7200_0001 --json out.json --cat test-file/acp7002_0001
# 期望:elements=10392(全 walk),合法 JSON
```

## Python 参考工具链(`docs/e3d 数据库分析/`)

```bash
# 整库导出(含跨库引用解析)
python "docs/e3d 数据库分析/e3d_export.py" \
  pdms-test-data/sam7200_0001 out.json --cat test-file/acp7002_0001

# owner 层级树
python "docs/e3d 数据库分析/e3d_tree.py" out.json

# 写侧全量自检(S1–S8b,13 demo,仅副本)
python "docs/e3d 数据库分析/e3d_write_full.py"     # 期望:全 PASS

# UDA / 表达式 UDA 巡检
python "docs/e3d 数据库分析/uda_probe.py" pdms-test-data/sam7200_0001
python "docs/e3d 数据库分析/uda_expr_probe.py" pdms-test-data/sam7200_0001
```

## 验收快验(对照 contracts/decode-contract.md C4)

1. `show /WB1` → POS 含 `(9630, 8072, 5282.5)`。
2. `cargo test` → 20 passed。
3. `rename` 后回读副本 → 新名,refno 不变,原文件不变。
4. `e3d_write_full.py` → S1–S8b 全 PASS,字节 diff 仅 page0。

## 已知前置 / 阻塞

- 需 `*vir.dat` 模式库(随 AVEVA EXE);无则隐式属性 offset 不可解。
- 真实 running-E3D round-trip 取证需用户侧 E3D(gated)。
- `pdms_io` 整 crate 构建待 `rs-core↔surrealdb-3.1`(项目外);E3D I/O 经 `crates/e3d_io` 旁路独立可测。
