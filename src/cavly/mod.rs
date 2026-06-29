pub mod audit;
pub mod builder;
pub mod config;
pub mod ffi;
pub mod project;
pub mod registry;
pub mod security;
pub mod tester;
pub mod workspace;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Cavly 版本
pub const VERSION: &str = env!("CAVLY_VERSION");

/// 默认配置文件名
pub const CONFIG_FILE: &str = "cavly.toml";

/// 默认目标目录
pub const TARGET_DIR: &str = "target";

/// 查找项目根目录（向上搜索 cavly.toml）
pub fn find_project_root(start_dir: &Path) -> Option<PathBuf> {
    let mut current = start_dir.to_path_buf();

    loop {
        let config_path = current.join(CONFIG_FILE);
        if config_path.exists() {
            return Some(current);
        }

        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => return None,
        }
    }
}

/// 确保目录存在
pub fn ensure_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)
            .with_context(|| format!("创建目录失败: {}", path.display()))?;
    }
    Ok(())
}

/// 执行命令并捕获输出
pub fn run_command(cmd: &mut std::process::Command) -> Result<std::process::Output> {
    let output = cmd
        .output()
        .with_context(|| format!("执行命令失败: {:?}", cmd))?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_find_project_root_found() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::write(root.join(CONFIG_FILE), "").unwrap();

        let subdir = root.join("src").join("nested");
        fs::create_dir_all(&subdir).unwrap();

        let result = find_project_root(&subdir);
        assert_eq!(result, Some(root.to_path_buf()));
    }

    #[test]
    fn test_find_project_root_not_found() {
        let temp = TempDir::new().unwrap();
        let result = find_project_root(temp.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_ensure_dir_creates_nested() {
        let temp = TempDir::new().unwrap();
        let nested = temp.path().join("a").join("b").join("c");

        ensure_dir(&nested).unwrap();
        assert!(nested.exists());
    }

    #[test]
    fn test_ensure_dir_existing() {
        let temp = TempDir::new().unwrap();
        ensure_dir(temp.path()).unwrap();
        assert!(temp.path().exists());
    }
}
