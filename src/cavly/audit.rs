//! Cavly 审计日志模块
//!
//! 实现 ESSO-10400 第10章审计与透明性要求。
//! 所有安全验证事件必须记录日志，包括时间戳、操作类型、
//! 包指纹、版本、验证结果和用户决策。
//!
//! 复杂度标注：
//! - 写入日志: O(1) 时间（追加写入）, O(1) 空间
//! - 读取日志: O(n) 时间, O(n) 空间，n 为日志条目数

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{OpenOptions, create_dir_all};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// 安全事件类型 (ESSO-10400 10.1)
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecurityEventType {
    /// 验证通过
    VerificationPassed,
    /// 验证失败
    VerificationFailed,
    /// 验证被跳过（用户显式选择）
    VerificationSkipped,
    /// 降级模式（安全验证系统故障）
    VerificationDowngraded,
    /// 安装未验证源包
    UnverifiedSourceInstall,
    /// 安装官方安全源包
    SecureSourceInstall,
    /// 警告已显示
    WarningDisplayed,
    /// 用户确认继续
    UserConfirmed,
    /// 用户拒绝
    UserRejected,
    /// 缓存证书过期
    CachedCertificateExpired,
    /// 公钥轮换
    KeyRotated,
    /// 证书已获取
    CertificateFetched,
    /// 索引已更新
    IndexUpdated,
}

/// 审计日志条目 (ESSO-10400 10.1)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuditLogEntry {
    /// ISO 8601 时间戳
    pub timestamp: String,
    /// 事件类型
    pub event_type: SecurityEventType,
    /// 操作类型
    pub operation: String,
    /// 包指纹
    pub package_fingerprint: Option<String>,
    /// 包名
    pub package_name: Option<String>,
    /// 版本
    pub package_version: Option<String>,
    /// 验证结果
    pub verification_result: Option<String>,
    /// 用户决策
    pub user_decision: Option<String>,
    /// 额外信息
    pub details: Option<String>,
}

impl AuditLogEntry {
    /// 创建新的审计日志条目
    pub fn new(event_type: SecurityEventType, operation: &str) -> Self {
        Self {
            timestamp: iso_timestamp(),
            event_type,
            operation: operation.to_string(),
            package_fingerprint: None,
            package_name: None,
            package_version: None,
            verification_result: None,
            user_decision: None,
            details: None,
        }
    }

    pub fn with_package(mut self, fingerprint: &str, name: &str, version: &str) -> Self {
        self.package_fingerprint = Some(fingerprint.to_string());
        self.package_name = Some(name.to_string());
        self.package_version = Some(version.to_string());
        self
    }

    pub fn with_result(mut self, result: &str) -> Self {
        self.verification_result = Some(result.to_string());
        self
    }

    pub fn with_user_decision(mut self, decision: &str) -> Self {
        self.user_decision = Some(decision.to_string());
        self
    }

    pub fn with_details(mut self, details: &str) -> Self {
        self.details = Some(details.to_string());
        self
    }
}

/// 审计日志管理器
///
/// 线程安全：每个 log 调用独立打开文件并追加写入，
/// 操作系统保证追加写入的原子性（POSIX O_APPEND）。
#[derive(Debug, Clone)]
pub struct AuditLogger {
    log_path: PathBuf,
}

impl AuditLogger {
    /// 创建默认审计日志器
    ///
    /// 默认路径: ~/.cavvy/audit/security.log
    pub fn new() -> Result<Self> {
        let log_path = default_audit_log_path()?;
        Ok(Self { log_path })
    }

    /// 指定日志路径
    pub fn with_path(path: PathBuf) -> Self {
        Self { log_path: path }
    }

    /// 记录事件
    ///
    /// # 复杂度
    /// - 时间: O(1)（追加写入）
    /// - 空间: O(1)
    /// - 磁盘 IO: 1 次顺序写
    pub fn log(&self, entry: &AuditLogEntry) -> Result<()> {
        let log_dir = self
            .log_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("无效的日志路径"))?;
        create_dir_all(log_dir)
            .with_context(|| format!("创建日志目录失败: {}", log_dir.display()))?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
            .with_context(|| format!("打开日志文件失败: {}", self.log_path.display()))?;

        let line = serde_json::to_string(entry).context("序列化日志条目失败")?;

        writeln!(file, "{}", line)
            .with_context(|| format!("写入日志失败: {}", self.log_path.display()))?;

        Ok(())
    }

    /// 记录事件（忽略错误）
    ///
    /// 用于非关键路径：日志失败不应阻断主流程。
    pub fn log_silent(&self, entry: &AuditLogEntry) {
        let _ = self.log(entry);
    }

    /// 读取所有日志
    ///
    /// # 复杂度
    /// - 时间: O(n)，n 为日志条目数
    /// - 空间: O(n)
    /// - 磁盘 IO: 1 次顺序读
    pub fn read_all(&self) -> Result<Vec<AuditLogEntry>> {
        if !self.log_path.exists() {
            return Ok(Vec::new());
        }

        let content = std::fs::read_to_string(&self.log_path)
            .with_context(|| format!("读取日志文件失败: {}", self.log_path.display()))?;

        let mut entries = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let entry: AuditLogEntry = serde_json::from_str(line)
                .with_context(|| format!("解析日志条目失败: {}", line))?;
            entries.push(entry);
        }

        Ok(entries)
    }

    /// 按事件类型过滤日志
    ///
    /// # 复杂度
    /// - 时间: O(n)
    /// - 空间: O(k)，k 为匹配条目数
    pub fn filter_by_type(&self, event_type: SecurityEventType) -> Result<Vec<AuditLogEntry>> {
        let all = self.read_all()?;
        Ok(all
            .into_iter()
            .filter(|e| e.event_type == event_type)
            .collect())
    }

    /// 获取日志文件路径
    pub fn log_path(&self) -> &PathBuf {
        &self.log_path
    }
}

impl Default for AuditLogger {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self::with_path(PathBuf::from("cavly-security.log")))
    }
}

/// 默认审计日志路径: ~/.cavvy/audit/security.log
///
/// Windows: %USERPROFILE%\.cavvy\audit\security.log
/// Unix: $HOME/.cavvy/audit/security.log
pub fn default_audit_log_path() -> Result<PathBuf> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            // 尝试通过 dirs 风格的逻辑获取主目录
            #[cfg(target_os = "windows")]
            {
                std::env::var("HOMEDRIVE")
                    .ok()
                    .zip(std::env::var("HOMEPATH").ok())
                    .map(|(d, p)| PathBuf::from(format!("{}{}", d, p)))
            }
            #[cfg(not(target_os = "windows"))]
            {
                None
            }
        })
        .ok_or_else(|| anyhow::anyhow!("无法确定用户主目录"))?;

    Ok(home.join(".cavvy").join("audit").join("security.log"))
}

/// 生成 ISO 8601 格式时间戳 (UTC)
///
/// 实现不依赖 chrono 等外部库，基于 UNIX 时间戳进行格里高利历转换。
///
/// # 复杂度
/// - 时间: O(1)
/// - 空间: O(1)
fn iso_timestamp() -> String {
    let now = SystemTime::now();
    let duration = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();

    let (year, month, day, hour, min, sec) = unix_to_ymd_hms(secs);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, min, sec
    )
}

/// UNIX 时间戳转换为年月日时分秒
fn unix_to_ymd_hms(mut secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let seconds_in_day = 86400u64;
    let mut days = secs / seconds_in_day;
    let rem = secs % seconds_in_day;
    let hour = (rem / 3600) as u32;
    let min = ((rem % 3600) / 60) as u32;
    let sec = (rem % 60) as u32;

    let (year, month, day) = days_to_ymd(days);
    (year, month, day, hour, min, sec)
}

/// 从 1970-01-01 起的天数计算年月日
fn days_to_ymd(mut days: u64) -> (u32, u32, u32) {
    let mut year = 1970u32;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year as u64 {
            break;
        }
        days -= days_in_year as u64;
        year += 1;
    }

    let days_in_month = [
        31u32,
        if is_leap_year(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u32;
    for dim in &days_in_month {
        if days < *dim as u64 {
            break;
        }
        days -= *dim as u64;
        month += 1;
    }

    (year, month, (days + 1) as u32)
}

fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_audit_log_entry_builder() {
        let entry = AuditLogEntry::new(SecurityEventType::VerificationPassed, "test_op")
            .with_package("fp1", "pkg", "1.0.0")
            .with_result("ok")
            .with_user_decision("continue")
            .with_details("all checks passed");

        assert_eq!(entry.operation, "test_op");
        assert_eq!(entry.package_fingerprint, Some("fp1".to_string()));
        assert_eq!(entry.package_name, Some("pkg".to_string()));
        assert_eq!(entry.package_version, Some("1.0.0".to_string()));
        assert_eq!(entry.verification_result, Some("ok".to_string()));
        assert_eq!(entry.user_decision, Some("continue".to_string()));
        assert_eq!(entry.details, Some("all checks passed".to_string()));
        assert!(!entry.timestamp.is_empty());
    }

    #[test]
    fn test_logger_write_read() {
        let temp = TempDir::new().unwrap();
        let log_path = temp.path().join("test.log");
        let logger = AuditLogger::with_path(log_path.clone());

        let entry1 = AuditLogEntry::new(SecurityEventType::SecureSourceInstall, "install")
            .with_package("fp1", "pkg", "1.0.0")
            .with_result("passed");
        let entry2 = AuditLogEntry::new(SecurityEventType::WarningDisplayed, "warn");

        logger.log(&entry1).unwrap();
        logger.log(&entry2).unwrap();

        let entries = logger.read_all().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0].event_type,
            SecurityEventType::SecureSourceInstall
        );
        assert_eq!(entries[1].event_type, SecurityEventType::WarningDisplayed);
    }

    #[test]
    fn test_logger_filter_by_type() {
        let temp = TempDir::new().unwrap();
        let log_path = temp.path().join("test.log");
        let logger = AuditLogger::with_path(log_path);

        logger
            .log(&AuditLogEntry::new(
                SecurityEventType::SecureSourceInstall,
                "a",
            ))
            .unwrap();
        logger
            .log(&AuditLogEntry::new(
                SecurityEventType::WarningDisplayed,
                "b",
            ))
            .unwrap();
        logger
            .log(&AuditLogEntry::new(
                SecurityEventType::SecureSourceInstall,
                "c",
            ))
            .unwrap();

        let filtered = logger
            .filter_by_type(SecurityEventType::SecureSourceInstall)
            .unwrap();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_logger_read_empty() {
        let temp = TempDir::new().unwrap();
        let log_path = temp.path().join("nonexistent.log");
        let logger = AuditLogger::with_path(log_path);
        let entries = logger.read_all().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_iso_timestamp_format() {
        let ts = iso_timestamp();
        assert_eq!(ts.len(), 20); // "YYYY-MM-DDTHH:MM:SSZ"
        assert!(ts.ends_with('Z'));
        assert!(ts.contains('T'));
    }

    #[test]
    fn test_unix_to_ymd_hms_epoch() {
        let (y, m, d, h, min, s) = unix_to_ymd_hms(0);
        assert_eq!((y, m, d, h, min, s), (1970, 1, 1, 0, 0, 0));
    }

    #[test]
    fn test_unix_to_ymd_hms_known_date() {
        let (y, m, d, h, min, s) = unix_to_ymd_hms(1782691200);
        assert_eq!(y, 2026);
        assert_eq!(m, 6);
        assert_eq!(d, 29);
        assert_eq!((h, min, s), (0, 0, 0));
    }

    #[test]
    fn test_is_leap_year() {
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2023));
    }

    #[test]
    fn test_default_audit_log_path_format() {
        let path = default_audit_log_path().unwrap();
        let s = path.to_string_lossy();
        assert!(s.contains(".cavvy"));
        assert!(s.contains("audit"));
        assert!(s.ends_with("security.log"));
    }

    #[test]
    fn test_log_silent_no_panic() {
        let temp = TempDir::new().unwrap();
        let logger = AuditLogger::with_path(temp.path().join("test.log"));
        let entry = AuditLogEntry::new(SecurityEventType::VerificationPassed, "test");
        // 不应 panic，即使目录不存在（但这里目录存在）
        logger.log_silent(&entry);
    }
}
