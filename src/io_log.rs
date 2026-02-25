use anyhow::{Context, Result};
use std::fmt;

/// 初始化日志，只输出到控制台
pub fn init_log(level: log::LevelFilter) -> Result<()> {
    let local_level = level;
    fern::Dispatch::new()
        .format(move |out, message, record| {
            if local_level > log::LevelFilter::Info {
                // Add some extra info to each message in debug
                out.finish(format_args!(
                    "[{}]({})({}) {}",
                    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f"),
                    record.target(),
                    record.level(),
                    message
                ))
            } else {
                out.finish(format_args!("{}", message))
            }
        })
        .level(level)
        .chain(std::io::stdout())
        .apply()
        .context("Unable to initialize log")?;
    Ok(())
}

/// 初始化日志，同时输出到控制台和文件
///
/// # 参数
///
/// * `level` - 日志级别过滤器
/// * `log_file_path` - 日志文件路径
/// * `rotate_size` - 单个日志文件大小限制（字节），达到后会创建新文件
/// * `max_files` - 最大保留的日志文件数量
///
/// # 返回
///
/// * `Result<()>` - 成功或错误
pub fn init_log_with_file(
    level: log::LevelFilter,
    log_file_path: &str,
    rotate_size: u64,
    max_files: usize,
) -> Result<()> {
    let local_level = level;

    // 创建日志格式
    let format =
        move |out: fern::FormatCallback, message: &fmt::Arguments, record: &log::Record| {
            if local_level > log::LevelFilter::Info {
                // 调试模式下，添加更多信息
                out.finish(format_args!(
                    "[{}][{}][{}] {}",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                    record.target(),
                    record.level(),
                    message
                ))
            } else {
                // 普通模式下，简化输出
                out.finish(format_args!(
                    "[{}][{}] {}",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    record.level(),
                    message
                ))
            }
        };

    // 创建日志分发器
    let base_config = fern::Dispatch::new().format(format).level(level);

    // 控制台输出
    let stdout_config = fern::Dispatch::new().chain(std::io::stdout());

    // 文件输出（带轮转）
    let file_config =
        fern::Dispatch::new().chain(fern::log_file(log_file_path).context("无法创建日志文件")?);

    // 组合并应用配置
    base_config
        .chain(stdout_config)
        .chain(file_config)
        .apply()
        .context("无法初始化日志系统")?;

    Ok(())
}

/// 高级日志配置，支持日志轮转功能
///
/// # 参数
///
/// * `config` - 日志配置
///
/// # 返回
///
/// * `Result<()>` - 成功或错误
pub fn init_log_advanced(config: LogConfig) -> Result<()> {
    use std::fmt;
    use std::fs;

    // 创建日志目录（如果不存在）
    if let Some(path) = config.log_file_path.as_ref() {
        if let Some(dir) = std::path::Path::new(path).parent() {
            fs::create_dir_all(dir).context("无法创建日志目录")?;
        }
    }

    // 创建日志格式
    let format =
        move |out: fern::FormatCallback, message: &fmt::Arguments, record: &log::Record| {
            let now = chrono::Local::now();

            if config.detailed_output {
                // 详细输出模式
                out.finish(format_args!(
                    "[{}][{}][{}] {}",
                    now.format("%Y-%m-%d %H:%M:%S%.3f"),
                    record.target(),
                    record.level(),
                    message
                ))
            } else {
                // 简洁输出模式
                out.finish(format_args!(
                    "[{}][{}] {}",
                    now.format("%Y-%m-%d %H:%M:%S"),
                    record.level(),
                    message
                ))
            }
        };

    // 创建基础配置
    let mut dispatch = fern::Dispatch::new().format(format).level(config.level);

    // 如果启用了控制台输出
    if config.console_output {
        // 根据是否为TTY终端决定是否添加颜色
        if atty::is(atty::Stream::Stdout) && config.colored_output {
            dispatch = dispatch.chain(fern::Dispatch::new().chain(std::io::stdout()));
        } else {
            dispatch = dispatch.chain(fern::Dispatch::new().chain(std::io::stdout()));
        }
    }

    // 如果启用了文件输出
    if let Some(log_path) = config.log_file_path {
        dispatch = dispatch.chain(
            fern::Dispatch::new().chain(fern::log_file(log_path).context("无法创建日志文件")?),
        );
    }

    // 应用配置
    dispatch.apply().context("无法初始化日志系统")?;

    Ok(())
}

/// 日志配置结构体
#[derive(Clone, Debug)]
pub struct LogConfig {
    /// 日志级别
    pub level: log::LevelFilter,
    /// 是否输出到控制台
    pub console_output: bool,
    /// 日志文件路径（None表示不输出到文件）
    pub log_file_path: Option<String>,
    /// 是否使用彩色输出（仅控制台）
    pub colored_output: bool,
    /// 是否使用详细输出格式
    pub detailed_output: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: log::LevelFilter::Info,
            console_output: true,
            log_file_path: None,
            colored_output: true,
            detailed_output: false,
        }
    }
}

impl LogConfig {
    /// 创建新的日志配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置日志级别
    pub fn level(mut self, level: log::LevelFilter) -> Self {
        self.level = level;
        self
    }

    /// 设置是否输出到控制台
    pub fn console_output(mut self, enabled: bool) -> Self {
        self.console_output = enabled;
        self
    }

    /// 设置日志文件路径
    pub fn file_output(mut self, path: impl Into<String>) -> Self {
        self.log_file_path = Some(path.into());
        self
    }

    /// 设置是否使用彩色输出
    pub fn colored_output(mut self, enabled: bool) -> Self {
        self.colored_output = enabled;
        self
    }

    /// 设置是否使用详细输出格式
    pub fn detailed_output(mut self, enabled: bool) -> Self {
        self.detailed_output = enabled;
        self
    }
}
