# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust crate (`pdms_io`) for parsing and analyzing PDMS (Plant Design Management System) database files. The project provides tools for reading PDMS binary data, performing B+tree index searches, and managing incremental data updates.

## Development Commands

### Building and Testing
- `cargo build` - Build the project
- `cargo build --release` - Build optimized release version
- `cargo test` - Run all tests
- `cargo run` - Run the main application (PDMS watcher)

### Binary Executables
The project includes several binary utilities in `src/bin/`:
- `cargo run --bin test_get_refno_status -- <db_path> [refno]` - Test reference number status lookup
- `cargo run --bin test_increment_eles -- <db_path> [start_sesno] [end_sesno]` - Test incremental element collection
- `cargo run --bin benchmark_increment_eles` - Benchmark incremental processing performance
- `cargo run --bin test_meilisearch` - Test Meilisearch integration
- `cargo run --bin test_search_integration` - Test search functionality

### Convenience Scripts
- `./test_refno.sh <db_path> [refno]` - Shell script wrapper for reference number testing
- `./test_increment.sh <db_path> [start_sesno] [end_sesno]` - Shell script wrapper for increment testing

### Features
The crate supports conditional compilation features:
- `debug_parse` - Enable parsing debug output
- `debug_btree_search` - Enable B+tree search debug output

## Architecture

### Core Components

#### Data Structures (`src/defines.rs`)
- `PdmsHeader` - PDMS database file header structure
- `ElePageData` - Element page data with 2KB page size
- `EleRawData` - Raw element data structures
- Binary data parsing using `deku` for big-endian format

#### I/O Layer (`src/io.rs`)
- `PdmsIO` - Main interface for PDMS database operations
- `ModifiedElement` - Tracks element modifications with detailed attribute changes
- B+tree index search algorithm with optimized path selection
- Support for incremental data collection and analysis

#### Search Module (`src/search.rs`)
- Meilisearch integration for fuzzy element searching
- `MeilisearchConfig` for server configuration
- Default configuration: localhost:7700, index name "pdms_elements"

#### File Synchronization (`src/sync/`)
- `sync.rs` - Main synchronization logic
- `files.rs` - File operations and management
- `compress.rs` - Data compression utilities
- `clone.rs` - Database cloning operations

#### Monitoring (`src/watch.rs`)
- `PdmsWatcher` - File system monitoring for PDMS database changes
- Real-time change detection and processing

### Key Dependencies
- `aios_core` - Core PDMS types and utilities (internal)
- `parse_pdms_db` - PDMS parsing library (internal)
- `surrealdb` - Database operations
- `meilisearch-sdk` - Search functionality
- `deku` - Binary data serialization/deserialization
- `tokio` - Async runtime
- `rayon` - Parallel processing

## Testing

The project includes comprehensive test suites:
- Unit tests in `src/test/` - Core functionality testing
- Integration tests in `src/tests/` - End-to-end testing
- Performance benchmarks for search algorithms

Test data is located in `pdms-test-data/` directory.

## B+Tree Search Algorithm

The project features an optimized B+tree index search algorithm with significant performance improvements (99%+ according to commit history). Debug output can be enabled with the `debug_btree_search` feature.

## Database Integration

The system supports multiple database backends:
- SurrealDB for structured data storage
- Meilisearch for full-text search capabilities
- Custom binary format for PDMS-specific data

## Configuration

Database options are configured via `DbOption.toml` file in the project root.