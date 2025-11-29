use std::path::Path;
use std::fs;
use dpcsync::{api, chunker, CompressionAlgorithm};
use tokio::fs::File;
use anyhow::{Context, Result};

/// 压缩单个输入文件为 .cba 归档
///
/// - 确保输入存在
/// - 自动创建输出父目录
pub async fn compress_archive<T: AsRef<Path>>(
    input: T,
    output: T,
    chunker_config: chunker::Config,
    algorithm: Option<CompressionAlgorithm>,
) -> Result<()> {
    let input_path = input.as_ref();
    let output_path = output.as_ref();

    if !input_path.exists() {
        anyhow::bail!("input file not found: {}", input_path.display());
    }
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create archive parent dir {}", parent.display()))?;
    }

    let compression = algorithm
        .map(|compression_algorithm| {
            dpcsync::Compression::try_new(
                compression_algorithm,
                compression_algorithm.max_level(),
            )
        })
        .transpose()
        .context("create compression config")?;

    let options = dpcsync::api::compress::CreateArchiveOptions {
        chunker_config,
        compression,
        ..Default::default()
    };
    let mut input_file = File::from_std(
        std::fs::File::open(input_path)
            .with_context(|| format!("open input file {}", input_path.display()))?,
    );
    let force_create = true;
    let mut output_file = File::from_std(
        std::fs::OpenOptions::new()
            .write(true)
            .read(true)
            .create(force_create)
            .truncate(force_create)
            .create_new(!force_create)
            .open(output_path)
            .with_context(|| format!("open output archive {}", output_path.display()))?,
    );
    api::compress::create_archive(&mut input_file, &mut output_file, &options)
        .await
        .context("create archive")?;

    Ok(())
}
