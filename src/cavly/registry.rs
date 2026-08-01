//! Cavly 安全注册表客户端
//!
//! 实现 ESSO-10430 第7章官方服务器端点定义，
//! 支持从安全源索引获取包信息并下载验证。
//!
//! 复杂度标注：
//! - 索引获取: O(1) 网络 + O(n) JSON 解析
//! - 包查找: O(n)，n 为索引中的包数量
//! - 下载验证: O(m) 网络 + O(m) 哈希计算，m 为包大小

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::audit::{AuditLogEntry, AuditLogger, SecurityEventType};
use super::security::{
    FingerprintMetadata, SecureIndex, VersionCertificate, load_root_public_key_from_config,
    verify_certificate_chain,
};

/// 官方安全源索引服务器地址
const CAYPAK_INDEX_URL: &str = "https://caypak.ethernos.net/index.json";
/// 官方安全证书服务器基础地址
const CAYCERT_BASE_URL: &str = "https://caycert.ethernos.net";

/// 官方根公钥分发地址（按优先级排序）
///
/// 1. 首选官方域名
/// 2. GitHub Pages 镜像
/// 3. GitHub raw 直链备用
pub const ROOT_KEY_URL_PRIMARY: &str = "https://cavvy-root-public-key.ethernos.net/public.pub";
pub const ROOT_KEY_URL_SECONDARY: &str = "https://cavvy-lang.github.io/EthernosRootPublicKey/public.pub";
pub const ROOT_KEY_URL_BACKUP: &str = "https://raw.githubusercontent.com/cavvy-lang/EthernosRootPublicKey/refs/heads/main/public.pub";

/// 安全注册表客户端配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegistryConfig {
    /// 索引 URL
    pub index_url: String,
    /// 证书服务器基础 URL
    pub cert_base_url: String,
    /// 是否启用证书缓存
    pub cache_enabled: bool,
    /// 证书缓存目录
    pub cache_dir: PathBuf,
    /// 根公钥 Base64（可选）
    pub root_public_key: Option<String>,
    /// 网络超时秒数
    pub timeout_secs: u64,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            index_url: CAYPAK_INDEX_URL.to_string(),
            cert_base_url: CAYCERT_BASE_URL.to_string(),
            cache_enabled: true,
            cache_dir: default_cache_dir().unwrap_or_else(|_| PathBuf::from(".cavvy/cache")),
            root_public_key: None,
            timeout_secs: 30,
        }
    }
}

/// 安全注册表客户端
///
/// 负责与 Ethernos 官方安全源服务器交互，获取索引、元信息、证书，
/// 并执行下载后的完整性验证。
pub struct SecureRegistry {
    config: RegistryConfig,
    logger: AuditLogger,
    /// 离线模式标志（pub 以便测试和高级使用）
    pub offline: bool,
}

impl SecureRegistry {
    /// 创建默认配置的安全注册表客户端
    pub fn new() -> Result<Self> {
        Ok(Self {
            config: RegistryConfig::default(),
            logger: AuditLogger::new()?,
            offline: false,
        })
    }

    /// 使用自定义配置创建
    pub fn with_config(config: RegistryConfig) -> Result<Self> {
        Ok(Self {
            config,
            logger: AuditLogger::new()?,
            offline: false,
        })
    }

    /// 设置离线模式
    pub fn offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }

    /// 设置审计日志器
    pub fn with_logger(mut self, logger: AuditLogger) -> Self {
        self.logger = logger;
        self
    }

    /// 获取安全源索引
    ///
    /// # 复杂度
    /// - 时间: O(1) 网络 + O(n) JSON 解析，n 为索引大小
    /// - 空间: O(n)
    /// - 磁盘 IO: 若启用缓存则 1 次写
    pub fn fetch_index(&self) -> Result<SecureIndex> {
        if self.offline {
            // 尝试从缓存读取
            return self.read_cached_index();
        }

        let data = http_get(&self.config.index_url, self.config.timeout_secs)
            .with_context(|| format!("获取索引失败: {}", self.config.index_url))?;

        let index: SecureIndex =
            serde_json::from_slice(&data).with_context(|| "解析索引 JSON 失败")?;

        // 缓存索引
        if self.config.cache_enabled {
            let _ = self.cache_index(&data);
        }

        self.logger.log_silent(
            &AuditLogEntry::new(SecurityEventType::IndexUpdated, "fetch_index")
                .with_details(&format!("packages_count={}", index.packages.len())),
        );

        Ok(index)
    }

    /// 获取包指纹元信息
    ///
    /// # 复杂度
    /// - 时间: O(1) 网络 + O(n) JSON 解析
    /// - 空间: O(n)
    pub fn fetch_fingerprint_metadata(&self, fingerprint: &str) -> Result<FingerprintMetadata> {
        validate_fingerprint(fingerprint)?;

        if self.offline {
            return self.read_cached_metadata(fingerprint);
        }

        let url = format!("{}/{}.json", self.config.cert_base_url, fingerprint);
        let data = http_get(&url, self.config.timeout_secs)
            .with_context(|| format!("获取指纹元信息失败: {}", url))?;

        let meta: FingerprintMetadata =
            serde_json::from_slice(&data).with_context(|| "解析指纹元信息失败")?;

        // 验证指纹格式
        if meta.fingerprint != fingerprint {
            bail!(
                "服务器返回的指纹不匹配: 请求 {}, 响应 {}",
                fingerprint,
                meta.fingerprint
            );
        }

        // 缓存元信息
        if self.config.cache_enabled {
            let _ = self.cache_metadata(fingerprint, &data);
        }

        Ok(meta)
    }

    /// 获取版本证书
    ///
    /// # 复杂度
    /// - 时间: O(1) 网络 + O(n) JSON 解析
    /// - 空间: O(n)
    pub fn fetch_certificate(&self, fingerprint: &str) -> Result<VersionCertificate> {
        validate_fingerprint(fingerprint)?;

        if self.offline {
            return self.read_cached_certificate(fingerprint);
        }

        let url = format!("{}/{}.cert", self.config.cert_base_url, fingerprint);
        let data = http_get(&url, self.config.timeout_secs)
            .with_context(|| format!("获取证书失败: {}", url))?;

        let cert: VersionCertificate =
            serde_json::from_slice(&data).with_context(|| "解析证书失败")?;

        if cert.fingerprint != fingerprint {
            bail!(
                "服务器返回的证书指纹不匹配: 请求 {}, 响应 {}",
                fingerprint,
                cert.fingerprint
            );
        }

        // 缓存证书
        if self.config.cache_enabled {
            let _ = self.cache_certificate(fingerprint, &data);
        }

        self.logger.log_silent(
            &AuditLogEntry::new(SecurityEventType::CertificateFetched, "fetch_certificate")
                .with_package(&cert.fingerprint, &cert.name, &cert.version),
        );

        Ok(cert)
    }

    /// 在索引中查找包
    ///
    /// # 复杂度
    /// - 时间: O(n)，n 为索引中的包数量
    /// - 空间: O(1)
    pub fn find_package(&self, name: &str) -> Result<super::security::IndexPackage> {
        let index = self.fetch_index()?;
        index
            .packages
            .into_iter()
            .find(|p| p.name == name)
            .ok_or_else(|| anyhow::anyhow!("在官方索引中找不到包: {}", name))
    }

    /// 按指纹查找包
    pub fn find_package_by_fingerprint(
        &self,
        fingerprint: &str,
    ) -> Result<super::security::IndexPackage> {
        let index = self.fetch_index()?;
        index
            .packages
            .into_iter()
            .find(|p| p.fingerprint == fingerprint)
            .ok_or_else(|| anyhow::anyhow!("在官方索引中找不到指纹对应的包: {}", fingerprint))
    }

    /// 下载并验证包
    ///
    /// # 流程
    /// 1. 获取指纹元信息
    /// 2. 获取版本证书
    /// 3. 下载包数据
    /// 4. 验证证书链（SHA-256 + 双重签名）
    /// 5. 记录审计日志
    ///
    /// # 复杂度
    /// - 时间: O(m) 网络 + O(m) 哈希，m 为包大小
    /// - 空间: O(m)
    pub fn download_and_verify(
        &self,
        pkg: &super::security::IndexPackage,
        dest_dir: &Path,
    ) -> Result<PathBuf> {
        // 1. 获取元信息和证书
        let meta = self.fetch_fingerprint_metadata(&pkg.fingerprint)?;
        let cert = self.fetch_certificate(&pkg.fingerprint)?;

        // 2. 下载包数据
        let package_path = self.download_package_data(pkg, dest_dir)?;
        let package_data = std::fs::read(&package_path)
            .with_context(|| format!("读取包数据失败: {}", package_path.display()))?;

        // 3. 验证证书链
        let root_pk = load_root_public_key_from_config(self.config.root_public_key.as_deref());
        verify_certificate_chain(&cert, &meta, &package_data, root_pk.as_ref())
            .with_context(|| format!("包 {}@{} 的安全验证失败", pkg.name, pkg.latest_version))?;

        // 4. 记录审计日志
        self.logger.log_silent(
            &AuditLogEntry::new(
                SecurityEventType::SecureSourceInstall,
                "download_and_verify",
            )
            .with_package(&pkg.fingerprint, &pkg.name, &pkg.latest_version)
            .with_result("passed"),
        );

        Ok(package_path)
    }

    /// 验证本地已下载的包
    ///
    /// 用于对已缓存的包重新执行验证（如根公钥更新后）。
    pub fn verify_local_package(&self, package_path: &Path, fingerprint: &str) -> Result<()> {
        let meta = self.fetch_fingerprint_metadata(fingerprint)?;
        let cert = self.fetch_certificate(fingerprint)?;
        let package_data = std::fs::read(package_path)
            .with_context(|| format!("读取包数据失败: {}", package_path.display()))?;

        let root_pk = load_root_public_key_from_config(self.config.root_public_key.as_deref());
        verify_certificate_chain(&cert, &meta, &package_data, root_pk.as_ref())
            .with_context(|| format!("本地包验证失败: {}", package_path.display()))?;

        self.logger.log_silent(
            &AuditLogEntry::new(
                SecurityEventType::VerificationPassed,
                "verify_local_package",
            )
            .with_package(fingerprint, &cert.name, &cert.version)
            .with_result("passed"),
        );

        Ok(())
    }

    /// 下载包数据
    ///
    /// 从 GitHub release 自动生成的源码 tar.gz 下载包。
    /// 统一使用 tar.gz 格式（zip 的 SHA-256 与证书中记录的不一致）。
    ///
    /// # 复杂度
    /// - 时间: O(n) 网络 + O(n) 磁盘，n 为包大小
    /// - 空间: O(n)
    fn download_package_data(
        &self,
        pkg: &super::security::IndexPackage,
        dest_dir: &Path,
    ) -> Result<PathBuf> {
        std::fs::create_dir_all(dest_dir)
            .with_context(|| format!("创建目录失败: {}", dest_dir.display()))?;

        let repo = pkg.repository.trim_end_matches(".git");

        // GitHub 自动生成 release 源码 tar.gz 的标准 URL
        // 策略1: 带 refs/tags/ 前缀
        let url = format!("{}/archive/refs/tags/v{}.tar.gz", repo, pkg.latest_version);
        let dest = dest_dir.join(format!("{}-{}.tar.gz", pkg.name, pkg.latest_version));

        println!("  开始下载包文件...");
        match http_get_with_progress(&url, self.config.timeout_secs) {
            Ok(data) => {
                std::fs::write(&dest, &data)
                    .with_context(|| format!("写入包文件失败: {}", dest.display()))?;
                println!("  下载完成: {} ({} 字节)", dest.display(), data.len());
                return Ok(dest);
            }
            Err(e1) => {
                // 策略2: 不带 refs/tags/ 前缀（兼容旧格式）
                let url_alt = format!("{}/archive/v{}.tar.gz", repo, pkg.latest_version);
                println!("  策略1失败，尝试策略2...");
                match http_get_with_progress(&url_alt, self.config.timeout_secs) {
                    Ok(data) => {
                        std::fs::write(&dest, &data)
                            .with_context(|| format!("写入包文件失败: {}", dest.display()))?;
                        println!("  下载完成: {} ({} 字节)", dest.display(), data.len());
                        return Ok(dest);
                    }
                    Err(e2) => {
                        bail!(
                            "包 {}@{} 下载失败，无法从 release 页面获取 (tar.gz)\n  策略1 ({}): {}\n  策略2 ({}): {}",
                            pkg.name,
                            pkg.latest_version,
                            url,
                            e1,
                            url_alt,
                            e2
                        );
                    }
                }
            }
        }
    }

    // ---- 缓存管理 ----

    fn cache_index(&self, data: &[u8]) -> Result<PathBuf> {
        let path = self.config.cache_dir.join("index.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, data)?;
        Ok(path)
    }

    fn read_cached_index(&self) -> Result<SecureIndex> {
        let path = self.config.cache_dir.join("index.json");
        let data = std::fs::read(&path)
            .with_context(|| format!("读取缓存索引失败: {}", path.display()))?;
        let index: SecureIndex =
            serde_json::from_slice(&data).with_context(|| "解析缓存索引失败")?;
        Ok(index)
    }

    /// 缓存元信息（pub 以便测试使用）
    pub fn cache_metadata(&self, fingerprint: &str, data: &[u8]) -> Result<PathBuf> {
        validate_fingerprint(fingerprint)?;
        let path = self.config.cache_dir.join(format!("{}.json", fingerprint));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, data)?;
        Ok(path)
    }

    /// 读取缓存的元信息（pub 以便测试使用）
    pub fn read_cached_metadata(&self, fingerprint: &str) -> Result<FingerprintMetadata> {
        validate_fingerprint(fingerprint)?;
        let path = self.config.cache_dir.join(format!("{}.json", fingerprint));
        let data = std::fs::read(&path)
            .with_context(|| format!("读取缓存元信息失败: {}", path.display()))?;
        let meta: FingerprintMetadata =
            serde_json::from_slice(&data).with_context(|| "解析缓存元信息失败")?;
        Ok(meta)
    }

    fn cache_certificate(&self, fingerprint: &str, data: &[u8]) -> Result<PathBuf> {
        validate_fingerprint(fingerprint)?;
        let path = self.config.cache_dir.join(format!("{}.cert", fingerprint));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, data)?;
        Ok(path)
    }

    fn read_cached_certificate(&self, fingerprint: &str) -> Result<VersionCertificate> {
        validate_fingerprint(fingerprint)?;
        let path = self.config.cache_dir.join(format!("{}.cert", fingerprint));
        let data = std::fs::read(&path)
            .with_context(|| format!("读取缓存证书失败: {}", path.display()))?;
        let cert: VersionCertificate =
            serde_json::from_slice(&data).with_context(|| "解析缓存证书失败")?;

        // 缓存证书过期检查：过期证书不得继续使用，记录审计事件后拒绝
        if let Err(e) = super::security::check_certificate_expiry(&cert) {
            self.logger.log_silent(
                &AuditLogEntry::new(
                    SecurityEventType::CachedCertificateExpired,
                    "read_cached_certificate",
                )
                .with_package(fingerprint, &cert.name, &cert.version)
                .with_details(&format!("{}", e)),
            );
            bail!("缓存证书已过期: {}。请清除缓存后重试", fingerprint);
        }

        Ok(cert)
    }

    /// 清除所有缓存
    pub fn clear_cache(&self) -> Result<()> {
        if self.config.cache_dir.exists() {
            std::fs::remove_dir_all(&self.config.cache_dir)
                .with_context(|| format!("清除缓存失败: {}", self.config.cache_dir.display()))?;
        }
        Ok(())
    }
}

/// 校验包指纹字符串，防止路径遍历与 URL 注入
///
/// 服务器下发的指纹会被拼入缓存文件路径（`{fp}.json` / `{fp}.cert`）
/// 和请求 URL，必须限制为安全字符集：仅允许 [A-Za-z0-9_-]，
/// 拒绝路径分隔符、`..` 及其他特殊字符。
fn validate_fingerprint(fingerprint: &str) -> Result<()> {
    if fingerprint.is_empty()
        || !fingerprint
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        bail!(
            "无效的包指纹（含非法字符，拒绝使用以防路径遍历）: {:?}",
            fingerprint
        );
    }
    Ok(())
}

/// 校验 URL 合法性，防止命令注入
///
/// URL 会被拼入 curl/wget 参数和 PowerShell 脚本，仅允许 http/https
/// 协议，并拒绝控制字符与空白字符。
fn validate_url(url: &str) -> Result<()> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        bail!("URL 协议不允许（仅支持 http/https）: {}", url);
    }
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        bail!("URL 包含非法字符（控制字符或空白）: {:?}", url);
    }
    Ok(())
}

/// HTTP GET 请求（最小依赖实现）
///
/// 优先使用系统 curl，Windows 回退到 PowerShell。
///
/// # 复杂度
/// - 时间: O(n) 网络 + O(n) IO
/// - 空间: O(n)
pub(crate) fn http_get(url: &str, timeout_secs: u64) -> Result<Vec<u8>> {
    validate_url(url)?;

    // 尝试使用 curl
    // --fail: HTTP 错误状态码（如 404）视为失败，避免错误页被当作数据
    let curl_result = std::process::Command::new("curl")
        .args(&[
            "-sL",
            "--fail",
            "--max-time",
            &timeout_secs.to_string(),
            "--connect-timeout",
            "10",
            url,
        ])
        .output();

    if let Ok(output) = curl_result {
        if output.status.success() && !output.stdout.is_empty() {
            return Ok(output.stdout);
        }
    }

    // 回退到 PowerShell (Windows)
    // 使用 -OutFile 保存到临时文件，避免二进制数据在 stdout 传输中被损坏
    if cfg!(target_os = "windows") {
        // 使用 tempfile 生成不可预测的临时文件名，
        // 避免固定文件名（cavvy_http_{pid}.tmp）被预判/劫持
        let temp_file = tempfile::Builder::new()
            .prefix("cavvy_http_")
            .suffix(".tmp")
            .tempfile()
            .context("创建临时文件失败")?;
        let temp_path = temp_file.path().to_path_buf();

        // PowerShell 单引号字符串转义: ' -> ''，防止命令注入
        let ps_escape = |s: &str| s.replace('\'', "''");
        let ps_cmd = format!(
            "try {{ Invoke-WebRequest -Uri '{}' -UseBasicParsing -MaximumRedirection 5 -TimeoutSec {} -OutFile '{}'; exit 0 }} catch {{ exit 1 }}",
            ps_escape(url),
            timeout_secs,
            ps_escape(&temp_path.to_string_lossy())
        );
        let output = std::process::Command::new("powershell")
            .args(&["-NoProfile", "-Command", &ps_cmd])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                if let Ok(data) = std::fs::read(&temp_path) {
                    if !data.is_empty() {
                        return Ok(data);
                    }
                }
            }
        }
        // temp_file 离开作用域时自动删除临时文件
    }

    // 回退到 wget (Linux)
    // wget 默认对 HTTP 错误状态码返回非零退出码，下方检查退出码即可
    let wget_result = std::process::Command::new("wget")
        .args(&["-qO-", "--timeout", &timeout_secs.to_string(), url])
        .output();

    if let Ok(output) = wget_result {
        if output.status.success() && !output.stdout.is_empty() {
            return Ok(output.stdout);
        }
    }

    bail!(
        "HTTP 请求失败，请确保系统已安装 curl、wget 或 PowerShell。URL: {}",
        url
    )
}

/// HTTP GET 请求并显示下载进度条
///
/// 优先使用系统 curl 的 `--progress-bar`，将进度实时输出到 stderr。
/// 下载内容写入临时文件后读入内存，避免进度信息与二进制数据混杂。
/// curl 不可用时静默回退到 `http_get`。
///
/// # 复杂度
/// - 时间: O(n) 网络 + O(n) IO
/// - 空间: O(n)
pub(crate) fn http_get_with_progress(url: &str, timeout_secs: u64) -> Result<Vec<u8>> {
    validate_url(url)?;

    // 使用临时文件接收下载内容，进度条由 curl 直接输出到 stderr
    let temp_file = tempfile::Builder::new()
        .prefix("cavvy_http_progress_")
        .suffix(".tmp")
        .tempfile()
        .context("创建临时文件失败")?;
    let temp_path = temp_file.path().to_path_buf();

    // curl: --progress-bar 提供原生进度条，-L 跟随重定向，--fail 对 HTTP 错误码返回非零
    let status = std::process::Command::new("curl")
        .args(&[
            "-L",
            "--fail",
            "--progress-bar",
            "--max-time",
            &timeout_secs.to_string(),
            "--connect-timeout",
            "10",
            "-o",
            &temp_path.to_string_lossy(),
            url,
        ])
        .status();

    if let Ok(status) = status {
        if status.success() {
            let data = std::fs::read(&temp_path)
                .with_context(|| format!("读取临时下载文件失败: {}", temp_path.display()))?;
            if !data.is_empty() {
                return Ok(data);
            }
        }
    }

    // curl 不可用或失败时回退到无进度版本
    http_get(url, timeout_secs)
}

/// 从官方分发点同步根公钥
///
/// 按优先级依次尝试：首选域名 → GitHub Pages → GitHub raw 备用。
/// 返回经过去空白处理的 Base64 编码公钥字符串，并验证其为 32 字节 Ed25519 公钥。
///
/// # 复杂度
/// - 时间: O(1) 网络（最多 3 次请求）
/// - 空间: O(1)
pub fn sync_root_public_key(timeout_secs: u64) -> Result<String> {
    sync_root_public_key_from_urls(
        &[
            ROOT_KEY_URL_PRIMARY,
            ROOT_KEY_URL_SECONDARY,
            ROOT_KEY_URL_BACKUP,
        ],
        timeout_secs,
    )
}

/// 从给定 URL 列表同步根公钥（可测试）
///
/// 按数组顺序尝试每个 URL，返回第一个验证通过的 Ed25519 公钥。
/// 成功时会打印来源信息到 stdout。
/// 从响应内容中提取公钥字符串
///
/// 支持两种格式：
/// 1. 纯文本 Base64 编码的 32 字节 Ed25519 公钥
/// 2. JSON 对象，包含 `public_key` 字段（如 EthernosRootPublicKey 仓库当前格式）
fn extract_root_public_key(text: &str) -> Result<String> {
    use super::security::Ed25519PublicKey;

    let trimmed = text.trim();
    if trimmed.is_empty() {
        bail!("响应内容为空");
    }

    // 尝试作为 JSON 解析，提取 public_key 字段
    if trimmed.starts_with('{') {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(pk) = value.get("public_key").and_then(|v| v.as_str()) {
                let pk = pk.trim();
                Ed25519PublicKey::from_base64("sync-root", pk)
                    .with_context(|| "JSON 中的 public_key 不是有效 Ed25519 公钥")?;
                return Ok(pk.to_string());
            }
        }
    }

    // 否则按纯 Base64 处理
    Ed25519PublicKey::from_base64("sync-root", trimmed)
        .with_context(|| "内容不是有效 Ed25519 公钥")?;
    Ok(trimmed.to_string())
}

pub fn sync_root_public_key_from_urls(urls: &[&str], timeout_secs: u64) -> Result<String> {
    use super::security::compute_key_fingerprint;

    let mut last_error = None;

    for url in urls {
        match http_get(url, timeout_secs) {
            Ok(data) => {
                let text = String::from_utf8_lossy(&data).to_string();
                match extract_root_public_key(&text) {
                    Ok(public_key_b64) => {
                        let pk = super::security::Ed25519PublicKey::from_base64("sync-root", &public_key_b64)
                            .expect("extract_root_public_key 已验证公钥格式");
                        let fingerprint = compute_key_fingerprint(&pk.bytes);
                        if *url == ROOT_KEY_URL_PRIMARY {
                            println!("已同步官方根公钥（首选源）");
                        } else {
                            println!("已同步官方根公钥（回退源: {}）", url);
                        }
                        println!("  指纹: {}", fingerprint);
                        return Ok(public_key_b64);
                    }
                    Err(e) => {
                        last_error = Some(format!("{} 内容解析失败: {}", url, e));
                        continue;
                    }
                }
            }
            Err(e) => {
                last_error = Some(format!("{} 获取失败: {}", url, e));
                continue;
            }
        }
    }

    bail!(
        "无法从任何源同步根公钥。最后错误: {}",
        last_error.unwrap_or_else(|| "未知".to_string())
    )
}

/// 默认缓存目录: ~/.cavvy/cache/registry
pub(crate) fn default_cache_dir() -> Result<PathBuf> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
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

    Ok(home.join(".cavvy").join("cache").join("registry"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use tempfile::TempDir;

    #[test]
    fn test_registry_config_default() {
        let config = RegistryConfig::default();
        assert_eq!(config.index_url, CAYPAK_INDEX_URL);
        assert_eq!(config.cert_base_url, CAYCERT_BASE_URL);
        assert!(config.cache_enabled);
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn test_secure_registry_new() {
        let reg = SecureRegistry::new().unwrap();
        assert!(!reg.offline);
    }

    #[test]
    fn test_secure_registry_offline() {
        let reg = SecureRegistry::new().unwrap().offline(true);
        assert!(reg.offline);
    }

    #[test]
    fn test_cache_roundtrip() {
        let temp = TempDir::new().unwrap();
        let mut config = RegistryConfig::default();
        config.cache_dir = temp.path().to_path_buf();
        config.cache_enabled = true;

        let reg = SecureRegistry::with_config(config).unwrap();

        let fp = "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d";
        let meta = crate::cavly::security::FingerprintMetadata {
            esso_version: "1.0.0".to_string(),
            fingerprint: fp.to_string(),
            current_name: "test".to_string(),
            current_publisher: "pub".to_string(),
            current_repository: "https://example.com".to_string(),
            created_at: "2026-06-29T00:00:00Z".to_string(),
            history: None,
            public_keys: vec![],
        };
        let json = serde_json::to_vec(&meta).unwrap();

        reg.cache_metadata(fp, &json).unwrap();
        let read = reg.read_cached_metadata(fp).unwrap();
        assert_eq!(read.fingerprint, fp);
    }

    #[test]
    fn test_clear_cache() {
        let temp = TempDir::new().unwrap();
        let mut config = RegistryConfig::default();
        config.cache_dir = temp.path().join("cache");
        config.cache_enabled = true;

        let reg = SecureRegistry::with_config(config).unwrap();
        reg.cache_metadata("fp", b"data").unwrap();
        assert!(reg.config.cache_dir.exists());

        reg.clear_cache().unwrap();
        assert!(!reg.config.cache_dir.exists());
    }

    #[test]
    fn test_default_cache_dir_format() {
        let path = default_cache_dir().unwrap();
        let s = path.to_string_lossy();
        assert!(s.contains(".cavvy"));
        assert!(s.contains("cache"));
        assert!(s.contains("registry"));
    }

    #[test]
    fn test_http_get_invalid_url_should_fail() {
        // 使用一个不可能成功的 URL 测试错误处理
        let result = http_get("http://localhost:59999/nonexistent", 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_fingerprint() {
        // 合法指纹
        assert!(validate_fingerprint("a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d").is_ok());
        assert!(validate_fingerprint("pkg_name-123").is_ok());
        // 路径遍历与非法字符一律拒绝
        assert!(validate_fingerprint("../../x").is_err());
        assert!(validate_fingerprint("..").is_err());
        assert!(validate_fingerprint("a/b").is_err());
        assert!(validate_fingerprint("a\\b").is_err());
        assert!(validate_fingerprint("").is_err());
        assert!(validate_fingerprint("a b").is_err());
    }

    #[test]
    fn test_validate_url() {
        assert!(validate_url("https://example.com/x.json").is_ok());
        assert!(validate_url("http://example.com").is_ok());
        // 非 http/https 协议拒绝
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("ftp://example.com").is_err());
        // 控制字符/空白拒绝（防止命令注入）
        assert!(validate_url("https://example.com/x\ny").is_err());
        assert!(validate_url("https://example.com/x y").is_err());
    }

    #[test]
    fn test_root_key_url_constants() {
        assert_eq!(
            ROOT_KEY_URL_PRIMARY,
            "https://cavvy-root-public-key.ethernos.net/public.pub"
        );
        assert_eq!(
            ROOT_KEY_URL_SECONDARY,
            "https://cavvy-lang.github.io/EthernosRootPublicKey/public.pub"
        );
        assert_eq!(
            ROOT_KEY_URL_BACKUP,
            "https://raw.githubusercontent.com/cavvy-lang/EthernosRootPublicKey/refs/heads/main/public.pub"
        );
    }

    #[test]
    fn test_sync_root_public_key_from_urls_all_fail() {
        // 使用不可能成功的地址，验证会按顺序尝试后失败
        let result = sync_root_public_key_from_urls(
            &["http://localhost:59998/public.pub", "http://localhost:59997/public.pub"],
            1,
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("无法从任何源同步根公钥"));
    }

    #[test]
    fn test_sync_root_public_key_from_urls_success_plain() {
        // 32 个零字节的 base64
        let valid_key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        let body = format!("{}\n\n", valid_key);

        // 启动一个最小 HTTP 服务器，只响应一次请求
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        });

        let url = format!("http://127.0.0.1:{}/public.pub", port);
        let result = sync_root_public_key_from_urls(&[&url], 5);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), valid_key);
    }

    #[test]
    fn test_sync_root_public_key_from_urls_success_json() {
        // 32 个零字节的 base64
        let valid_key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        let body = format!(
            r#"{{"version":1,"date":"2026-08-01","key_id":"test","algorithm":"Ed25519","public_key":"{}"}}"#,
            valid_key
        );

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        });

        let url = format!("http://127.0.0.1:{}/public.pub", port);
        let result = sync_root_public_key_from_urls(&[&url], 5);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), valid_key);
    }

    #[test]
    fn test_sync_root_public_key_from_urls_invalid_content() {
        // 启动一个最小 HTTP 服务器返回无效内容
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let body = "not-a-valid-key";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        });

        let url = format!("http://127.0.0.1:{}/public.pub", port);
        let result = sync_root_public_key_from_urls(&[&url], 5);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("无法从任何源同步根公钥"));
    }
}
