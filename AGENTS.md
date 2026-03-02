## Cursor Cloud specific instructions

### Project overview

This is a Rust crate (`pdms_io`) for parsing PDMS (Plant Design Management System) binary database files. See `CLAUDE.md` for the full project overview and development commands.

### External dependency setup

The project depends on `aios_core` (`/rs-core`) and `indextree` (`/indextree`) as sibling path dependencies. These are NOT part of the workspace repo and must be cloned separately:

- **aios_core**: `git clone https://github.com/happyrust/rs-core.git /rs-core` — must be checked out at commit `65fd9124` (dev-3.1 branch) with cherry-picked commit `cc159b16` for rkyv derive fixes.
- **indextree**: `git clone https://github.com/saschagrunert/indextree.git /indextree` — upstream indextree with a locally-added `rkyv` feature (rkyv derives for Arena, Node, NodeId, NodeStamp, NodeData types).

Several GitHub repos under `happyrust/` have been deleted. Git URL redirects are configured globally to route some of them to gitee mirrors:

```
git config --global url."https://gitee.com/happydpc/dpc-sync".insteadOf "https://github.com/happyrust/dpc-sync"
git config --global url."https://gitee.com/happydpc/calamine".insteadOf "https://github.com/happyrust/calamine"
```

### Build prerequisites

- **Rust nightly** is required (edition 2024). Set via `rustup default nightly`.
- **protoc** must be installed (`sudo apt-get install -y protobuf-compiler`). The `.cargo/config.toml` has Windows-only PROTOC paths; override with env vars:
  ```
  export PROTOC=/usr/bin/protoc
  export PROTOC_INCLUDE=/usr/include
  ```
  These are set in `~/.bashrc` for persistence.

### Build & test commands

```bash
cargo build                        # Build all targets
cargo test --lib                   # Run unit tests (71 pass)
cargo test                         # Run all tests (4 integration tests fail due to missing DbOption config — expected)
cargo clippy                       # Lint (1 pre-existing warning in main.rs)
cargo run --bin test_page_types    # Quick smoke test for PDMS page type parsing
cargo run --bin test_get_refno_status -- pdms-test-data/sam7200_0001  # Test refno lookup
```

### Known environment caveats

- Some integration tests (`debug_trim_lowcase`) and binaries (`test_increment_eles`) require a `db_options/DbOption` config file from `aios_core` which is not present in the Cloud VM. These tests fail with "configuration file not found" and can be safely ignored.
- The `test_meilisearch` and `test_search_integration` binaries require `--features meilisearch` and a running Meilisearch server on `localhost:7700`. These are optional.
- No external database servers (SurrealDB, ArangoDB, TiDB) are needed for basic compilation or unit tests — SurrealDB uses in-memory mode (`kv-mem` feature).
