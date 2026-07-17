pub mod cli;
pub mod install;
pub mod release;

use std::fmt;
use std::io;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Http(reqwest::Error),
    InvalidArgument(String),
    InvalidRelease(String),
    Verification(String),
    Unsupported(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O 错误: {error}"),
            Self::Http(error) => write!(f, "网络请求失败: {error}"),
            Self::InvalidArgument(message) => write!(f, "参数错误: {message}"),
            Self::InvalidRelease(message) => write!(f, "Release 不可用: {message}"),
            Self::Verification(message) => write!(f, "校验失败: {message}"),
            Self::Unsupported(message) => write!(f, "不支持: {message}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Http(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}
