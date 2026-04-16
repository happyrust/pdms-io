use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use crate::core::EngineError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileOpenMode {
    ReadOnly,
    ReadWrite,
    Shared,
}

impl FileOpenMode {
    pub fn fioxst_code(&self, db_no: i32) -> i32 {
        let mode = match self {
            FileOpenMode::ReadOnly => 7,
            FileOpenMode::ReadWrite => 2,
            FileOpenMode::Shared => 6,
        };
        db_no * 100 + mode
    }
}

pub struct FileToken {
    path: PathBuf,
    file: Option<File>,
    mode: FileOpenMode,
    db_no: i32,
}

impl FileToken {
    pub fn open(path: &Path, db_no: i32, mode: FileOpenMode) -> Result<Self, EngineError> {
        let file = match mode {
            FileOpenMode::ReadOnly | FileOpenMode::Shared => {
                OpenOptions::new().read(true).open(path)?
            }
            FileOpenMode::ReadWrite => OpenOptions::new().read(true).write(true).open(path)?,
        };

        Ok(Self {
            path: path.to_path_buf(),
            file: Some(file),
            mode,
            db_no,
        })
    }

    pub fn create_new(path: &Path, db_no: i32) -> Result<Self, EngineError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        Ok(Self {
            path: path.to_path_buf(),
            file: Some(file),
            mode: FileOpenMode::ReadWrite,
            db_no,
        })
    }

    pub fn close(&mut self) {
        self.file = None;
    }

    pub fn switch_mode(&mut self, new_mode: FileOpenMode) -> Result<(), EngineError> {
        self.close();
        let file = match new_mode {
            FileOpenMode::ReadOnly | FileOpenMode::Shared => {
                OpenOptions::new().read(true).open(&self.path)?
            }
            FileOpenMode::ReadWrite => {
                OpenOptions::new().read(true).write(true).open(&self.path)?
            }
        };
        self.file = Some(file);
        self.mode = new_mode;
        Ok(())
    }

    pub fn file(&self) -> Result<&File, EngineError> {
        self.file
            .as_ref()
            .ok_or_else(|| EngineError::InvalidState("文件已关闭".into()))
    }

    pub fn file_mut(&mut self) -> Result<&mut File, EngineError> {
        self.file
            .as_mut()
            .ok_or_else(|| EngineError::InvalidState("文件已关闭".into()))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn mode(&self) -> FileOpenMode {
        self.mode
    }

    pub fn db_no(&self) -> i32 {
        self.db_no
    }

    pub fn is_open(&self) -> bool {
        self.file.is_some()
    }

    pub fn delete(mut self) -> Result<(), EngineError> {
        self.close();
        std::fs::remove_file(&self.path)?;
        Ok(())
    }
}

pub fn build_db_filename(project_dir: &Path, project_name: &str, db_no: i32) -> PathBuf {
    let suffix = format!("{:03}", db_no);
    project_dir.join(format!("{}{}", project_name, suffix))
}
