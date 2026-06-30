//! Cavly 安全验证模块
//!
//! 实现 ESSO-10410（数字签名与哈希标准）和 ESSO-10430（包管理器证书规范）
//! 的安全验证功能。
//!
//! 复杂度标注：
//! - SHA-256 计算: O(n) 时间, O(1) 额外空间
//! - Ed25519 验证: O(1) 时间（固定长度输入）, O(1) 空间
//! - JCS 规范化: O(n log n) 时间（键排序）, O(n) 空间

use anyhow::{Context, Result, bail};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// 安全等级 (ESSO-10400 第6章)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SecurityLevel {
    /// Level 1 — 基础级: 允许未验证源，但需警告
    General,
    /// Level 2 — 标准级: 强制官方安全源验证，未验证源需二次确认
    Standard,
    /// Level 3 — 核心级: 仅允许官方安全源，不可覆盖
    Critical,
}

impl Default for SecurityLevel {
    fn default() -> Self {
        SecurityLevel::Standard
    }
}

impl std::fmt::Display for SecurityLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecurityLevel::General => write!(f, "general"),
            SecurityLevel::Standard => write!(f, "standard"),
            SecurityLevel::Critical => write!(f, "critical"),
        }
    }
}

/// Ed25519 公钥封装
#[derive(Debug, Clone)]
pub struct Ed25519PublicKey {
    pub key_id: String,
    pub bytes: [u8; 32],
}

impl Ed25519PublicKey {
    /// 从 Base64 编码的字符串创建公钥
    ///
    /// # 复杂度
    /// - 时间: O(1)
    /// - 空间: O(1)
    pub fn from_base64(key_id: &str, b64: &str) -> Result<Self> {
        let bytes = base64_decode(b64)?;
        if bytes.len() != 32 {
            bail!(
                "Ed25519 公钥长度必须为 32 字节，实际 {} 字节",
                bytes.len()
            );
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self {
            key_id: key_id.to_string(),
            bytes: arr,
        })
    }

    /// 从原始字节创建公钥
    pub fn from_bytes(key_id: &str, bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 32 {
            bail!(
                "Ed25519 公钥长度必须为 32 字节，实际 {} 字节",
                bytes.len()
            );
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(bytes);
        Ok(Self {
            key_id: key_id.to_string(),
            bytes: arr,
        })
    }

    /// 转换为 VerifyingKey
    pub fn to_verifying_key(&self) -> Result<VerifyingKey> {
        VerifyingKey::from_bytes(&self.bytes)
            .map_err(|e| anyhow::anyhow!("无效的 Ed25519 公钥: {}", e))
    }
}

/// 签名结构 (ESSO-10430 5.3)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SignatureData {
    pub key_id: String,
    pub algorithm: String,
    pub signature: String,
}

/// 双重签名结构 (ESSO-10430 5.2)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DualSignatures {
    pub publisher: SignatureData,
    pub authority: SignatureData,
}

/// 包指纹元信息 (ESSO-10430 5.1)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FingerprintMetadata {
    pub esso_version: String,
    pub fingerprint: String,
    pub current_name: String,
    pub current_publisher: String,
    pub current_repository: String,
    pub created_at: String,
    pub history: Option<History>,
    pub public_keys: Vec<PublicKeyEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct History {
    pub names: Option<Vec<HistoryEntry>>,
    pub publishers: Option<Vec<HistoryEntry>>,
    pub repositories: Option<Vec<RepositoryHistoryEntry>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HistoryEntry {
    pub name: Option<String>,
    pub publisher: Option<String>,
    pub changed_at: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RepositoryHistoryEntry {
    pub repository: String,
    pub changed_at: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PublicKeyEntry {
    pub key_id: String,
    pub algorithm: String,
    pub public_key: String,
    pub activated_at: String,
    pub status: String,
}

/// 版本级证书 (ESSO-10430 5.2)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VersionCertificate {
    pub esso_version: String,
    pub fingerprint: String,
    pub version: String,
    pub name: String,
    pub publisher: String,
    pub repository: String,
    pub commit_hash: String,
    pub package_sha256: String,
    pub certified_at: String,
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Vec<CertDependency>>,
    pub signatures: DualSignatures,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CertDependency {
    pub fingerprint: String,
    pub version_constraint: String,
}

/// 安全索引条目 (ESSO-10430 7.1)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IndexPackage {
    pub fingerprint: String,
    pub name: String,
    pub latest_version: String,
    pub repository: String,
    pub latest_commit: String,
    pub latest_sha256: String,
    pub cert_url: String,
    pub meta_url: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecureIndex {
    pub esso_version: String,
    pub generated_at: String,
    pub packages: Vec<IndexPackage>,
}

/// 将字节数组编码为小写十六进制字符串
///
/// # 复杂度
/// - 时间: O(n)
/// - 空间: O(n)
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(&mut s, "{:02x}", b);
    }
    s
}

/// 计算 SHA-256 哈希 (ESSO-10410 4.1)
///
/// # 复杂度
/// - 时间: O(n)，n 为输入字节数
/// - 空间: O(1) 额外空间
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    bytes_to_hex(&hasher.finalize())
}

/// 计算 SHA-256 原始字节
pub fn sha256_raw(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&result);
    arr
}

/// 验证 SHA-256 哈希
///
/// # 复杂度
/// - 时间: O(n)
/// - 空间: O(1)
pub fn verify_sha256(data: &[u8], expected: &str) -> Result<()> {
    let actual = sha256_hex(data);
    let expected_lower = expected.to_lowercase();
    if actual != expected_lower {
        bail!(
            "SHA-256 校验失败\n预期: {}\n实际: {}",
            expected_lower,
            actual
        );
    }
    Ok(())
}

/// Base64 解码
///
/// # 复杂度
/// - 时间: O(n)
/// - 空间: O(n)
pub fn base64_decode(s: &str) -> Result<Vec<u8>> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    STANDARD.decode(s).context("Base64 解码失败")
}

/// Base64 编码
///
/// # 复杂度
/// - 时间: O(n)
/// - 空间: O(n)
pub fn base64_encode(data: &[u8]) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    STANDARD.encode(data)
}

/// RFC 8785 JSON Canonicalization Scheme (JCS)
///
/// 使用 serde_jcs 实现符合规范的 JSON 规范化：
/// 对象键按 Unicode 码点排序，数字按 IEEE 754 规范序列化，
/// 字符串转义规范化，无多余空白。
///
/// # 复杂度
/// - 时间: O(n log n)，n 为对象键数量（排序开销）
/// - 空间: O(n)
pub fn canonicalize_jcs(value: &Value) -> Result<Vec<u8>> {
    serde_jcs::to_vec(value).context("JCS 规范化失败")
}

/// 构造签名载荷：移除 signatures 字段，JCS 规范化
///
/// # 复杂度
/// - 时间: O(n log n)
/// - 空间: O(n)
pub fn build_signing_payload(cert: &Value) -> Result<Vec<u8>> {
    let mut cert_without_sigs = cert.clone();
    if let Value::Object(ref mut map) = cert_without_sigs {
        map.remove("signatures");
    }
    canonicalize_jcs(&cert_without_sigs)
}

/// 验证 Ed25519 签名 (ESSO-10410 8.1)
///
/// # 复杂度
/// - 时间: O(1)（Ed25519 验证为常数时间）
/// - 空间: O(1)
pub fn verify_ed25519(
    payload: &[u8],
    signature_b64: &str,
    public_key: &Ed25519PublicKey,
) -> Result<()> {
    if public_key.bytes == [0u8; 32] {
        bail!("公钥未初始化");
    }

    let signature_bytes = base64_decode(signature_b64)?;
    if signature_bytes.len() != 64 {
        bail!(
            "Ed25519 签名长度必须为 64 字节，实际 {} 字节",
            signature_bytes.len()
        );
    }

    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|e| anyhow::anyhow!("无效的 Ed25519 签名: {}", e))?;

    let verifying_key = public_key.to_verifying_key()?;

    verifying_key
        .verify(payload, &signature)
        .map_err(|e| anyhow::anyhow!("Ed25519 签名验证失败: {}", e))?;

    Ok(())
}

/// 验证双重签名 (ESSO-10430 5.3)
///
/// # 复杂度
/// - 时间: O(1)
/// - 空间: O(1)
pub fn verify_dual_signatures(
    cert: &VersionCertificate,
    meta: &FingerprintMetadata,
    root_public_key: Option<&Ed25519PublicKey>,
) -> Result<()> {
    // 1. 验证算法
    if cert.signatures.publisher.algorithm != "Ed25519" {
        bail!(
            "不支持的发布者签名算法: {}",
            cert.signatures.publisher.algorithm
        );
    }
    if cert.signatures.authority.algorithm != "Ed25519" {
        bail!(
            "不支持的官方签名算法: {}",
            cert.signatures.authority.algorithm
        );
    }

    // 2. 获取发布者公钥
    let publisher_pk = meta
        .public_keys
        .iter()
        .find(|pk| {
            pk.key_id == cert.signatures.publisher.key_id && pk.status == "active"
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "找不到活跃的发布者公钥: {}",
                cert.signatures.publisher.key_id
            )
        })?;

    let publisher_ed_pk =
        Ed25519PublicKey::from_base64(&publisher_pk.key_id, &publisher_pk.public_key)?;

    // 3. 重构签名载荷
    let cert_json = serde_json::to_value(cert).context("序列化证书失败")?;
    let payload = build_signing_payload(&cert_json)?;

    // 4. 验证发布者签名
    verify_ed25519(&payload, &cert.signatures.publisher.signature, &publisher_ed_pk)
        .context("发布者签名验证失败")?;

    // 5. 验证官方签名（如果提供了根公钥）
    if let Some(root_pk) = root_public_key {
        verify_ed25519(&payload, &cert.signatures.authority.signature, root_pk)
            .context("Ethernos 官方签名验证失败")?;
    }

    Ok(())
}

/// 验证证书完整性链
///
/// # 复杂度
/// - 时间: O(1)（网络获取不计入）
/// - 空间: O(1)
pub fn verify_certificate_chain(
    cert: &VersionCertificate,
    meta: &FingerprintMetadata,
    package_data: &[u8],
    root_public_key: Option<&Ed25519PublicKey>,
) -> Result<()> {
    // 1. 验证包指纹格式 (UUID v4)
    verify_uuid_v4(&cert.fingerprint).context("包指纹格式无效")?;

    // 2. 验证元信息一致性
    if cert.name != meta.current_name {
        bail!(
            "证书名称与元信息不一致: {} vs {}",
            cert.name,
            meta.current_name
        );
    }
    if cert.publisher != meta.current_publisher {
        bail!(
            "证书发布者与元信息不一致: {} vs {}",
            cert.publisher,
            meta.current_publisher
        );
    }
    if cert.repository != meta.current_repository {
        bail!(
            "证书仓库与元信息不一致: {} vs {}",
            cert.repository,
            meta.current_repository
        );
    }

    // 3. 验证 SHA-256 完整性
    verify_sha256(package_data, &cert.package_sha256).context("包完整性校验失败")?;

    // 4. 验证双重签名
    verify_dual_signatures(cert, meta, root_public_key).context("双重签名验证失败")?;

    Ok(())
}

/// 验证 UUID v4 格式
fn verify_uuid_v4(s: &str) -> Result<()> {
    // 使用简单字符检查而非正则，避免引入 regex 开销到核心路径
    if s.len() != 36 {
        bail!("UUID 长度必须为 36 字符");
    }
    let expected_dashes = [8, 13, 18, 23];
    for &pos in &expected_dashes {
        if s.as_bytes()[pos] != b'-' {
            bail!("UUID 格式错误: 第 {} 位应为 '-', 实际是 '{}'", pos, s.as_bytes()[pos] as char);
        }
    }
    let version_char = s.as_bytes()[14];
    if version_char != b'4' {
        bail!("UUID 版本位必须为 '4', 实际是 '{}'", version_char as char);
    }
    let variant_char = s.as_bytes()[19];
    if !matches!(variant_char, b'8' | b'9' | b'a' | b'b') {
        bail!("UUID 变体位无效: '{}'", variant_char as char);
    }
    for (i, c) in s.bytes().enumerate() {
        if expected_dashes.contains(&i) {
            continue;
        }
        if !c.is_ascii_hexdigit() {
            bail!("UUID 包含非十六进制字符: '{}' 在位置 {}", c as char, i);
        }
    }
    Ok(())
}

/// 计算公钥指纹 (SHA-256 前 16 字节，32 字符十六进制)
///
/// # 复杂度
/// - 时间: O(1)
/// - 空间: O(1)
pub fn compute_key_fingerprint(public_key_bytes: &[u8]) -> String {
    let hash = sha256_hex(public_key_bytes);
    hash[..32].to_string()
}

/// 官方根公钥（硬编码占位，实际应从可信通道获取）
///
/// 注意：在生产环境中，此公钥应通过独立可信渠道分发，
/// 或编译时通过环境变量/构建脚本注入。
pub fn official_root_public_key() -> Option<Ed25519PublicKey> {
    // 返回 None 表示未配置根公钥，此时跳过官方签名验证
    // 实际部署时应替换为硬编码的根公钥
    None
}

/// 从配置文件加载根公钥
pub fn load_root_public_key_from_config(config_pk: Option<&str>) -> Option<Ed25519PublicKey> {
    if let Some(b64) = config_pk {
        if let Ok(pk) = Ed25519PublicKey::from_base64("root", b64) {
            return Some(pk);
        }
    }
    official_root_public_key()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_hex() {
        let data = b"hello";
        let hash = sha256_hex(data);
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        // 验证输出为小写十六进制
        for c in hash.chars() {
            assert!(
                c.is_ascii_digit() || ('a'..='f').contains(&c),
                "字符 '{}' 不是小写十六进制",
                c
            );
        }
    }

    #[test]
    fn test_sha256_known_vector() {
        // "abc" 的 SHA-256 已知值
        let data = b"abc";
        let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(sha256_hex(data), expected);
    }

    #[test]
    fn test_verify_sha256_success() {
        let data = b"hello";
        let hash = sha256_hex(data);
        assert!(verify_sha256(data, &hash).is_ok());
    }

    #[test]
    fn test_verify_sha256_failure() {
        let data = b"hello";
        let result = verify_sha256(
            data,
            "0000000000000000000000000000000000000000000000000000000000000000",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_base64_roundtrip() {
        let data = b"hello world";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(data.to_vec(), decoded);
    }

    #[test]
    fn test_base64_decode_empty() {
        let decoded = base64_decode("").unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_canonicalize_jcs_simple() {
        let val = serde_json::json!({"b": 1, "a": 2});
        let canonical = canonicalize_jcs(&val).unwrap();
        let s = String::from_utf8(canonical).unwrap();
        assert_eq!(s, r#"{"a":2,"b":1}"#);
    }

    #[test]
    fn test_canonicalize_jcs_nested() {
        let val = serde_json::json!({"z": {"b": 2, "a": 1}, "a": 3});
        let canonical = canonicalize_jcs(&val).unwrap();
        let s = String::from_utf8(canonical).unwrap();
        assert_eq!(s, r#"{"a":3,"z":{"a":1,"b":2}}"#);
    }

    #[test]
    fn test_verify_uuid_v4_valid() {
        assert!(verify_uuid_v4("a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d").is_ok());
        assert!(verify_uuid_v4("00000000-0000-4000-8000-000000000000").is_ok());
        assert!(verify_uuid_v4("ffffffff-ffff-4fff-9fff-ffffffffffff").is_ok());
    }

    #[test]
    fn test_verify_uuid_v4_invalid() {
        assert!(verify_uuid_v4("not-a-uuid").is_err());
        assert!(verify_uuid_v4("a1b2c3d4-e5f6-1a7b-8c9d-0e1f2a3b4c5d").is_err());
        assert!(verify_uuid_v4("a1b2c3d4-e5f6-4a7b-1c9d-0e1f2a3b4c5d").is_err());
        assert!(verify_uuid_v4("a1b2c3d4e5f6-4a7b-8c9d-0e1f2a3b4c5d").is_err());
        assert!(verify_uuid_v4("a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5g").is_err());
    }

    #[test]
    fn test_compute_key_fingerprint_length() {
        let pk = [0u8; 32];
        let fp = compute_key_fingerprint(&pk);
        assert_eq!(fp.len(), 32);
    }

    #[test]
    fn test_ed25519_public_key_from_base64() {
        // 32 个零字节的 base64
        let b64 = base64_encode(&[0u8; 32]);
        let pk = Ed25519PublicKey::from_base64("test", &b64).unwrap();
        assert_eq!(pk.key_id, "test");
        assert_eq!(pk.bytes, [0u8; 32]);
    }

    #[test]
    fn test_ed25519_public_key_invalid_length() {
        let b64 = base64_encode(&[0u8; 31]);
        assert!(Ed25519PublicKey::from_base64("test", &b64).is_err());
    }

    #[test]
    fn test_security_level_display() {
        assert_eq!(SecurityLevel::General.to_string(), "general");
        assert_eq!(SecurityLevel::Standard.to_string(), "standard");
        assert_eq!(SecurityLevel::Critical.to_string(), "critical");
    }

    #[test]
    fn test_security_level_default() {
        assert_eq!(SecurityLevel::default(), SecurityLevel::Standard);
    }

    #[test]
    fn test_certificate_parse() {
        let json = serde_json::json!({
            "esso_version": "1.0.0",
            "fingerprint": "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d",
            "version": "1.0.0",
            "name": "test-pkg",
            "publisher": "ethernos",
            "repository": "https://github.com/ethernos/test",
            "commit_hash": "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0",
            "package_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
            "certified_at": "2026-06-17T12:00:00Z",
            "signatures": {
                "publisher": {
                    "key_id": "pub123",
                    "algorithm": "Ed25519",
                    "signature": "SGVsbG8="
                },
                "authority": {
                    "key_id": "root123",
                    "algorithm": "Ed25519",
                    "signature": "V29ybGQ="
                }
            }
        });
        let cert: VersionCertificate = serde_json::from_value(json).unwrap();
        assert_eq!(cert.name, "test-pkg");
        assert_eq!(cert.signatures.publisher.algorithm, "Ed25519");
    }

    #[test]
    fn test_meta_parse() {
        let json = serde_json::json!({
            "esso_version": "1.0.0",
            "fingerprint": "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d",
            "current_name": "test-pkg",
            "current_publisher": "ethernos",
            "current_repository": "https://github.com/ethernos/test",
            "created_at": "2026-06-17T12:00:00Z",
            "public_keys": []
        });
        let meta: FingerprintMetadata = serde_json::from_value(json).unwrap();
        assert_eq!(meta.current_name, "test-pkg");
    }

    #[test]
    fn test_build_signing_payload() {
        let cert = serde_json::json!({
            "name": "test",
            "version": "1.0.0",
            "signatures": {"publisher": {"key_id": "k", "algorithm": "Ed25519", "signature": "s"}}
        });
        let payload = build_signing_payload(&cert).unwrap();
        let s = String::from_utf8(payload).unwrap();
        assert!(!s.contains("signatures"));
        assert!(s.contains("name"));
        assert!(s.contains("version"));
    }

    #[test]
    fn test_verify_dual_signatures_missing_key() {
        let cert = VersionCertificate {
            esso_version: "1.0.0".to_string(),
            fingerprint: "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d".to_string(),
            version: "1.0.0".to_string(),
            name: "test".to_string(),
            publisher: "pub".to_string(),
            repository: "https://example.com".to_string(),
            commit_hash: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0".to_string(),
            package_sha256: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            certified_at: "2026-06-17T12:00:00Z".to_string(),
            expires_at: None,
            dependencies: None,
            signatures: DualSignatures {
                publisher: SignatureData {
                    key_id: "missing".to_string(),
                    algorithm: "Ed25519".to_string(),
                    signature: "SGVsbG8=".to_string(),
                },
                authority: SignatureData {
                    key_id: "root".to_string(),
                    algorithm: "Ed25519".to_string(),
                    signature: "V29ybGQ=".to_string(),
                },
            },
        };
        let meta = FingerprintMetadata {
            esso_version: "1.0.0".to_string(),
            fingerprint: "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d".to_string(),
            current_name: "test".to_string(),
            current_publisher: "pub".to_string(),
            current_repository: "https://example.com".to_string(),
            created_at: "2026-06-17T12:00:00Z".to_string(),
            history: None,
            public_keys: vec![],
        };
        let result = verify_dual_signatures(&cert, &meta, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_dual_signatures_wrong_algorithm() {
        let cert = VersionCertificate {
            esso_version: "1.0.0".to_string(),
            fingerprint: "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d".to_string(),
            version: "1.0.0".to_string(),
            name: "test".to_string(),
            publisher: "pub".to_string(),
            repository: "https://example.com".to_string(),
            commit_hash: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0".to_string(),
            package_sha256: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            certified_at: "2026-06-17T12:00:00Z".to_string(),
            expires_at: None,
            dependencies: None,
            signatures: DualSignatures {
                publisher: SignatureData {
                    key_id: "k".to_string(),
                    algorithm: "RSA".to_string(),
                    signature: "s".to_string(),
                },
                authority: SignatureData {
                    key_id: "root".to_string(),
                    algorithm: "Ed25519".to_string(),
                    signature: "s".to_string(),
                },
            },
        };
        let meta = FingerprintMetadata {
            esso_version: "1.0.0".to_string(),
            fingerprint: "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d".to_string(),
            current_name: "test".to_string(),
            current_publisher: "pub".to_string(),
            current_repository: "https://example.com".to_string(),
            created_at: "2026-06-17T12:00:00Z".to_string(),
            history: None,
            public_keys: vec![PublicKeyEntry {
                key_id: "k".to_string(),
                algorithm: "Ed25519".to_string(),
                public_key: base64_encode(&[0u8; 32]),
                activated_at: "2026-06-17T12:00:00Z".to_string(),
                status: "active".to_string(),
            }],
        };
        let result = verify_dual_signatures(&cert, &meta, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_certificate_chain_name_mismatch() {
        let cert = VersionCertificate {
            esso_version: "1.0.0".to_string(),
            fingerprint: "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d".to_string(),
            version: "1.0.0".to_string(),
            name: "wrong-name".to_string(),
            publisher: "pub".to_string(),
            repository: "https://example.com".to_string(),
            commit_hash: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0".to_string(),
            package_sha256: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            certified_at: "2026-06-17T12:00:00Z".to_string(),
            expires_at: None,
            dependencies: None,
            signatures: DualSignatures {
                publisher: SignatureData {
                    key_id: "k".to_string(),
                    algorithm: "Ed25519".to_string(),
                    signature: "s".to_string(),
                },
                authority: SignatureData {
                    key_id: "root".to_string(),
                    algorithm: "Ed25519".to_string(),
                    signature: "s".to_string(),
                },
            },
        };
        let meta = FingerprintMetadata {
            esso_version: "1.0.0".to_string(),
            fingerprint: "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d".to_string(),
            current_name: "right-name".to_string(),
            current_publisher: "pub".to_string(),
            current_repository: "https://example.com".to_string(),
            created_at: "2026-06-17T12:00:00Z".to_string(),
            history: None,
            public_keys: vec![],
        };
        let result = verify_certificate_chain(&cert, &meta, b"test", None);
        assert!(result.is_err());
    }

    /// 使用真实 caysdlib 证书验证签名计算路径（回归测试）
    #[test]
    fn test_real_caysdlib_signature() {
        let cert_json = r#"{"esso_version":"1.0.0","fingerprint":"54274058-c8bf-485b-a71e-d7bdee8b3b0f","version":"0.1.0","name":"caysdlib","publisher":"Ethernos Studio","repository":"https://github.com/cavvy-lang/caysdlib","commit_hash":"a7ea8dcc48de6649f8101b64efa4403c6e04239d","package_sha256":"ced1e7182690910be5a18c30f3d96f3cb3719004980ac9ee50cfff43f9ca4979","certified_at":"2026-06-29T13:12:30Z","expires_at":"2031-06-29T13:12:30Z","signatures":{"publisher":{"key_id":"7a1a00b49d394f65d0a7f6031a1b2692","algorithm":"Ed25519","signature":"9eBxjlvd6mS2xutnI1Y5o2JzRRgHVBxQ/LBeds814gzumo6Ytj2J51LUR7D6Ht89ZdgVI2nxf8p91+xC/FfJBQ=="},"authority":{"key_id":"a3d59261417db6d1b8c3398465274d5e","algorithm":"Ed25519","signature":"NsriHQWLDERowOLqt5p7c8ao+123MHJLI01IlXmnqQXAclNNNwJFL8RUhPR6mBNvMuRXeRLNUuwolsbwAOYdDA=="}}}"#;
        let cert: VersionCertificate = serde_json::from_str(cert_json).unwrap();
        let cert_value = serde_json::to_value(&cert).unwrap();
        let payload = build_signing_payload(&cert_value).unwrap();

        let pk = Ed25519PublicKey::from_base64("pub", "RTfAQ4kW+D+2LpOQlrglBE0ZsPjIhLlR1+cAymRbx2U=").unwrap();
        let result = verify_ed25519(&payload, "9eBxjlvd6mS2xutnI1Y5o2JzRRgHVBxQ/LBeds814gzumo6Ytj2J51LUR7D6Ht89ZdgVI2nxf8p91+xC/FfJBQ==", &pk);
        assert!(result.is_ok());
    }
}
