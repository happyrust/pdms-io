use std::fs;
use std::path::Path;
// use dpcsync::{api, chunker, CompressionAlgorithm};
use anyhow::{Context, Result};
use tokio::fs::File;

/// 压缩单个输入文件为 .cba 归档
///
/// - 确保输入存在
/// - 自动创建输出父目录
/// 压缩单个输入文件为 .cba 归档
///
/// - 确保输入存在
/// - 自动创建输出父目录
/// 注意: 由于 dpcsync 依赖被注释掉，此函数暂时不可用
#[allow(dead_code)]
pub async fn compress_archive<T: AsRef<Path>>(
    input: T,
    output: T,
    _chunker_config: dpcsync::chunker::Config,
    _algorithm: Option<dpcsync::CompressionAlgorithm>,
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

    // TODO: 由于 dpcsync 依赖被注释掉，暂时无法实现压缩功能
    anyhow::bail!("compress_archive is not available: dpcsync dependency is commented out");

    Ok(())
}
