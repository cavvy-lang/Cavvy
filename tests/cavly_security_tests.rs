//! Cavly 安全模块集成测试
//!
//! 测试 ESSO-10410（哈希与签名）和 ESSO-10430（证书与注册表）的集成功能。

use std::collections::HashMap;

use cavvy::cavly::audit::{AuditLogEntry, AuditLogger, SecurityEventType};
use cavvy::cavly::config::{CavlyConfig, SecurityConfig, SecurityWarningPreference};
use cavvy::cavly::registry::{RegistryConfig, SecureRegistry};
use cavvy::cavly::security::{
    FingerprintMetadata, PublicKeyEntry, SecurityLevel, SignatureData, VersionCertificate,
    base64_decode, base64_encode, build_signing_payload, bytes_to_hex, canonicalize_jcs,
    compute_key_fingerprint, sha256_hex, sha256_raw, verify_dual_signatures, verify_ed25519,
    verify_sha256,
};
use tempfile::TempDir;

// ============================================================
// SHA-256 测试
// ============================================================

#[test]
fn test_sha256_known_vectors() {
    let vectors = vec![
        (
            b"" as &[u8],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            b"abc",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
    ];

    for (input, expected) in vectors {
        assert_eq!(sha256_hex(input), expected, "SHA-256 已知向量测试失败");
    }
}

#[test]
fn test_sha256_raw_length() {
    let hash = sha256_raw(b"test");
    assert_eq!(hash.len(), 32);
}

#[test]
fn test_verify_sha256_success() {
    let data = b"hello world";
    let hash = sha256_hex(data);
    assert!(verify_sha256(data, &hash).is_ok());
}

#[test]
fn test_verify_sha256_failure() {
    let data = b"hello world";
    let result = verify_sha256(
        data,
        "0000000000000000000000000000000000000000000000000000000000000000",
    );
    assert!(result.is_err());
}

// ============================================================
// Base64 测试
// ============================================================

#[test]
fn test_base64_roundtrip() {
    let data = b"The quick brown fox jumps over the lazy dog";
    let encoded = base64_encode(data);
    let decoded = base64_decode(&encoded).unwrap();
    assert_eq!(data.to_vec(), decoded);
}

#[test]
fn test_base64_empty() {
    let encoded = base64_encode(b"");
    assert_eq!(encoded, "");
    assert_eq!(base64_decode(&encoded).unwrap(), Vec::<u8>::new());
}

// ============================================================
// JCS 规范化测试
// ============================================================

#[test]
fn test_jcs_sorts_object_keys() {
    let val = serde_json::json!({"z": 1, "a": 2, "m": 3});
    let canonical = canonicalize_jcs(&val).unwrap();
    let s = String::from_utf8(canonical).unwrap();
    assert_eq!(s, r#"{"a":2,"m":3,"z":1}"#);
}

#[test]
fn test_jcs_nested_objects() {
    let val = serde_json::json!({
        "b": {"z": 1, "a": 2},
        "a": {"y": 3, "x": 4}
    });
    let canonical = canonicalize_jcs(&val).unwrap();
    let s = String::from_utf8(canonical).unwrap();
    assert_eq!(s, r#"{"a":{"x":4,"y":3},"b":{"a":2,"z":1}}"#);
}

#[test]
fn test_build_signing_payload_removes_signatures() {
    let cert = serde_json::json!({
        "name": "pkg",
        "version": "1.0.0",
        "signatures": {"publisher": {"key_id": "k", "algorithm": "Ed25519", "signature": "s"}}
    });
    let payload = build_signing_payload(&cert).unwrap();
    let s = String::from_utf8(payload).unwrap();
    assert!(!s.contains("signatures"));
    assert!(s.contains("name"));
    assert!(s.contains("version"));
}

// ============================================================
// 公钥与指纹测试
// ============================================================

#[test]
fn test_ed25519_public_key_from_base64_valid() {
    let bytes = [0u8; 32];
    let b64 = base64_encode(&bytes);
    let pk = cavvy::cavly::security::Ed25519PublicKey::from_base64("test", &b64).unwrap();
    assert_eq!(pk.key_id, "test");
    assert_eq!(pk.bytes, bytes);
}

#[test]
fn test_ed25519_public_key_invalid_length() {
    let b64 = base64_encode(&[0u8; 31]);
    let result = cavvy::cavly::security::Ed25519PublicKey::from_base64("test", &b64);
    assert!(result.is_err());
}

#[test]
fn test_compute_key_fingerprint_length() {
    let fp = compute_key_fingerprint(&[0u8; 32]);
    assert_eq!(fp.len(), 32);
}

#[test]
fn test_bytes_to_hex() {
    assert_eq!(bytes_to_hex(&[0xAB, 0xCD, 0xEF]), "abcdef");
    assert_eq!(bytes_to_hex(&[0x00, 0xFF]), "00ff");
}

// ============================================================
// 证书解析与验证测试
// ============================================================

#[test]
fn test_version_certificate_parse() {
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
    assert_eq!(cert.version, "1.0.0");
    assert_eq!(cert.signatures.publisher.algorithm, "Ed25519");
    assert_eq!(cert.signatures.authority.algorithm, "Ed25519");
}

#[test]
fn test_fingerprint_metadata_parse() {
    let json = serde_json::json!({
        "esso_version": "1.0.0",
        "fingerprint": "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d",
        "current_name": "test-pkg",
        "current_publisher": "ethernos",
        "current_repository": "https://github.com/ethernos/test",
        "created_at": "2026-06-17T12:00:00Z",
        "public_keys": [
            {
                "key_id": "pub1",
                "algorithm": "Ed25519",
                "public_key": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
                "activated_at": "2026-06-17T12:00:00Z",
                "status": "active"
            }
        ]
    });

    let meta: FingerprintMetadata = serde_json::from_value(json).unwrap();
    assert_eq!(meta.current_name, "test-pkg");
    assert_eq!(meta.public_keys.len(), 1);
    assert_eq!(meta.public_keys[0].status, "active");
}

#[test]
fn test_verify_dual_signatures_missing_key_fails() {
    let cert = VersionCertificate {
        esso_version: "1.0.0".to_string(),
        fingerprint: "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d".to_string(),
        version: "1.0.0".to_string(),
        name: "test".to_string(),
        publisher: "pub".to_string(),
        repository: "https://example.com".to_string(),
        commit_hash: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0".to_string(),
        package_sha256: "0000000000000000000000000000000000000000000000000000000000000000"
            .to_string(),
        certified_at: "2026-06-17T12:00:00Z".to_string(),
        expires_at: None,
        dependencies: None,
        signatures: cavvy::cavly::security::DualSignatures {
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
fn test_verify_dual_signatures_wrong_algorithm_fails() {
    let cert = VersionCertificate {
        esso_version: "1.0.0".to_string(),
        fingerprint: "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d".to_string(),
        version: "1.0.0".to_string(),
        name: "test".to_string(),
        publisher: "pub".to_string(),
        repository: "https://example.com".to_string(),
        commit_hash: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0".to_string(),
        package_sha256: "0000000000000000000000000000000000000000000000000000000000000000"
            .to_string(),
        certified_at: "2026-06-17T12:00:00Z".to_string(),
        expires_at: None,
        dependencies: None,
        signatures: cavvy::cavly::security::DualSignatures {
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

// ============================================================
// 审计日志集成测试
// ============================================================

#[test]
fn test_audit_logger_write_read() {
    let temp = TempDir::new().unwrap();
    let log_path = temp.path().join("audit.log");
    let logger = AuditLogger::with_path(log_path.clone());

    let entry = AuditLogEntry::new(SecurityEventType::VerificationPassed, "test_op")
        .with_package("fp1", "pkg", "1.0.0")
        .with_result("ok")
        .with_details("all checks passed");

    logger.log(&entry).unwrap();

    let entries = logger.read_all().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].event_type, SecurityEventType::VerificationPassed);
    assert_eq!(entries[0].package_name, Some("pkg".to_string()));
}

#[test]
fn test_audit_logger_filter_by_type() {
    let temp = TempDir::new().unwrap();
    let logger = AuditLogger::with_path(temp.path().join("audit.log"));

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
fn test_audit_logger_read_empty_file() {
    let temp = TempDir::new().unwrap();
    let logger = AuditLogger::with_path(temp.path().join("empty.log"));
    let entries = logger.read_all().unwrap();
    assert!(entries.is_empty());
}

// ============================================================
// 安全注册表客户端集成测试
// ============================================================

#[test]
fn test_registry_config_default() {
    let config = RegistryConfig::default();
    assert_eq!(config.index_url, "https://caypak.ethernos.net/index.json");
    assert_eq!(config.cert_base_url, "https://caycert.ethernos.net");
    assert!(config.cache_enabled);
    assert_eq!(config.timeout_secs, 30);
}

#[test]
fn test_secure_registry_offline_mode() {
    let reg = SecureRegistry::new().unwrap().offline(true);
    assert!(reg.offline);
}

#[test]
fn test_secure_registry_cache_roundtrip() {
    let temp = TempDir::new().unwrap();
    let mut config = RegistryConfig::default();
    config.cache_dir = temp.path().to_path_buf();
    config.cache_enabled = true;

    let reg = SecureRegistry::with_config(config).unwrap();

    let fp = "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d";
    let meta = FingerprintMetadata {
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
fn test_secure_registry_clear_cache() {
    let temp = TempDir::new().unwrap();
    let mut config = RegistryConfig::default();
    config.cache_dir = temp.path().join("cache");
    config.cache_enabled = true;

    let reg = SecureRegistry::with_config(config).unwrap();
    reg.cache_metadata("fp", b"data").unwrap();
    reg.clear_cache().unwrap();
}

// ============================================================
// 安全配置集成测试
// ============================================================

#[test]
fn test_security_config_default() {
    let config = SecurityConfig::default();
    assert_eq!(config.level, SecurityLevel::Standard);
    assert!(!config.allow_downgrade);
    assert!(config.cache_enabled);
    assert!(config.audit_log);
    assert_eq!(config.warning_preference, SecurityWarningPreference::Warn);
    assert!(config.trusted_keys.is_empty());
}

#[test]
fn test_cavly_config_with_security_roundtrip() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("cavly.toml");

    let mut config = CavlyConfig::default();
    config.package.name = "test-sec".to_string();
    config.security.level = SecurityLevel::Critical;
    config.security.allow_downgrade = false;
    config.security.audit_log = true;
    config.security.trusted_keys = vec!["AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=".to_string()];

    config.to_file(&config_path).unwrap();
    let loaded = CavlyConfig::from_file(&config_path).unwrap();

    assert_eq!(loaded.security.level, SecurityLevel::Critical);
    assert!(!loaded.security.allow_downgrade);
    assert_eq!(loaded.security.trusted_keys.len(), 1);
}

#[test]
fn test_workspace_resolver_from_config() {
    let temp = TempDir::new().unwrap();
    let mut config = CavlyConfig::default();
    config.package.name = "test".to_string();
    config.package.version = "1.0.0".to_string();
    config.security.level = SecurityLevel::Critical;
    config.security.allow_downgrade = true;

    let mut resolver =
        cavvy::cavly::workspace::WorkspaceResolver::from_config(temp.path().to_path_buf(), &config);

    // 创建测试项目以便 resolver 可以工作
    config.to_file(&temp.path().join("cavly.toml")).unwrap();

    // 解析器应根据配置初始化安全参数
    let resolved = resolver.resolve_all(&config);
    // 空项目应该成功解析
    assert!(resolved.is_ok());
}

// ============================================================
// 安全等级序列化测试
// ============================================================

#[test]
fn test_security_level_serialization() {
    assert_eq!(
        serde_json::to_string(&SecurityLevel::General).unwrap(),
        "\"general\""
    );
    assert_eq!(
        serde_json::to_string(&SecurityLevel::Standard).unwrap(),
        "\"standard\""
    );
    assert_eq!(
        serde_json::to_string(&SecurityLevel::Critical).unwrap(),
        "\"critical\""
    );
}

#[test]
fn test_security_level_deserialization() {
    assert_eq!(
        serde_json::from_str::<SecurityLevel>("\"general\"").unwrap(),
        SecurityLevel::General
    );
    assert_eq!(
        serde_json::from_str::<SecurityLevel>("\"standard\"").unwrap(),
        SecurityLevel::Standard
    );
    assert_eq!(
        serde_json::from_str::<SecurityLevel>("\"critical\"").unwrap(),
        SecurityLevel::Critical
    );
}

// ============================================================
// 安全事件类型测试
// ============================================================

#[test]
fn test_security_event_type_serialization() {
    let types = vec![
        SecurityEventType::VerificationPassed,
        SecurityEventType::VerificationFailed,
        SecurityEventType::VerificationSkipped,
        SecurityEventType::WarningDisplayed,
        SecurityEventType::SecureSourceInstall,
    ];

    for t in types {
        let serialized = serde_json::to_string(&t).unwrap();
        let deserialized: SecurityEventType = serde_json::from_str(&serialized).unwrap();
        assert_eq!(t, deserialized);
    }
}

// ============================================================
// 边界和压力测试
// ============================================================

#[test]
fn test_sha256_large_input() {
    let data = vec![0xABu8; 1024 * 1024]; // 1MB
    let hash = sha256_hex(&data);
    assert_eq!(hash.len(), 64);
}

#[test]
fn test_audit_logger_many_entries() {
    let temp = TempDir::new().unwrap();
    let logger = AuditLogger::with_path(temp.path().join("audit.log"));

    for i in 0..100 {
        let entry = AuditLogEntry::new(SecurityEventType::VerificationPassed, "stress")
            .with_details(&format!("entry {}", i));
        logger.log(&entry).unwrap();
    }

    let entries = logger.read_all().unwrap();
    assert_eq!(entries.len(), 100);
}

#[test]
fn test_canonicalize_jcs_array_of_objects() {
    let val = serde_json::json!([
        {"b": 2, "a": 1},
        {"d": 4, "c": 3}
    ]);
    let canonical = canonicalize_jcs(&val).unwrap();
    let s = String::from_utf8(canonical).unwrap();
    assert_eq!(s, r#"[{"a":1,"b":2},{"c":3,"d":4}]"#);
}

// ============================================================
// 依赖添加功能测试
// ============================================================

#[test]
fn test_add_system_lib_to_config() {
    let temp = TempDir::new().unwrap();
    let mut config = CavlyConfig::default();
    config.package.name = "test".to_string();
    config.package.version = "1.0.0".to_string();
    config.to_file(&temp.path().join("cavly.toml")).unwrap();

    cavvy::cavly::project::Project::add_system_lib(temp.path(), "ws2_32").unwrap();

    let loaded = CavlyConfig::from_file(&temp.path().join("cavly.toml")).unwrap();
    assert!(loaded.ffi.system_libs.contains(&"ws2_32".to_string()));
}

/// 创建本地测试用 Git 仓库（无需网络）
///
/// # 复杂度
/// - 时间: O(1)
/// - 空间: O(1)
fn create_local_git_repo(path: &std::path::Path, project_type: &str) -> anyhow::Result<()> {
    use std::process::Command;

    std::fs::create_dir_all(path)?;

    // git init
    let status = Command::new("git")
        .args(&["init", &path.to_string_lossy()])
        .status()?;
    if !status.success() {
        anyhow::bail!("git init 失败");
    }

    // 配置 git 用户（commit 需要）
    Command::new("git")
        .args(&[
            "-C",
            &path.to_string_lossy(),
            "config",
            "user.email",
            "test@test.com",
        ])
        .status()?;
    Command::new("git")
        .args(&[
            "-C",
            &path.to_string_lossy(),
            "config",
            "user.name",
            "Test User",
        ])
        .status()?;

    // 创建 cavly.toml
    let cay_config = format!(
        r#"[package]
name = "my-lib"
version = "0.1.0"
project_type = "{}"
main = "lib.cay"
src_dir = "src"
target_dir = "target"
"#,
        project_type
    );
    std::fs::write(path.join("cavly.toml"), cay_config)?;

    // 创建 src 目录和空 lib.cay
    std::fs::create_dir_all(path.join("src"))?;
    std::fs::write(path.join("src").join("lib.cay"), "// lib\n")?;

    // git add + commit
    Command::new("git")
        .args(&["-C", &path.to_string_lossy(), "add", "."])
        .status()?;
    let status = Command::new("git")
        .args(&["-C", &path.to_string_lossy(), "commit", "-m", "init"])
        .status()?;
    if !status.success() {
        anyhow::bail!("git commit 失败");
    }

    Ok(())
}

#[test]
fn test_add_git_dependency_to_config() {
    // 跳过测试如果 git 不可用
    if std::process::Command::new("git")
        .arg("--version")
        .status()
        .is_err()
    {
        eprintln!("警告: git 不可用，跳过 test_add_git_dependency_to_config");
        return;
    }

    let temp = TempDir::new().unwrap();
    let mut config = CavlyConfig::default();
    config.package.name = "test".to_string();
    config.package.version = "1.0.0".to_string();
    config.to_file(&temp.path().join("cavly.toml")).unwrap();

    // 创建本地 Git 仓库
    let repo_path = temp.path().join("remote-my-lib");
    create_local_git_repo(&repo_path, "lib").unwrap();

    let repo_url = repo_path.to_string_lossy().to_string();

    cavvy::cavly::project::Project::add_git_dependency(
        temp.path(),
        "my-lib",
        &repo_url,
        Some("master"),
        None,
    )
    .unwrap();

    let loaded = CavlyConfig::from_file(&temp.path().join("cavly.toml")).unwrap();
    assert!(loaded.dependencies.contains_key("my-lib"));

    // 验证本地路径已设置
    let dep = loaded.dependencies.get("my-lib").unwrap();
    match dep {
        cavvy::cavly::config::Dependency::Detailed(detailed) => {
            assert!(detailed.path.is_some());
            assert_eq!(detailed.git.as_deref(), Some(repo_url.as_str()));
        }
        _ => panic!("期望 Detailed 依赖"),
    }

    // 验证仓库已克隆到本地
    assert!(
        temp.path()
            .join(".cavvy")
            .join("git")
            .join("my-lib")
            .join("cavly.toml")
            .exists()
    );
}

#[test]
fn test_add_path_dependency_to_config() {
    let temp = TempDir::new().unwrap();
    let mut config = CavlyConfig::default();
    config.package.name = "test".to_string();
    config.package.version = "1.0.0".to_string();
    config.to_file(&temp.path().join("cavly.toml")).unwrap();

    cavvy::cavly::project::Project::add_path_dependency(
        temp.path(),
        "local-helper",
        "../local-helper",
    )
    .unwrap();

    let loaded = CavlyConfig::from_file(&temp.path().join("cavly.toml")).unwrap();
    assert!(loaded.dependencies.contains_key("local-helper"));
}

#[test]
fn test_add_duplicate_dependency_fails() {
    let temp = TempDir::new().unwrap();
    let mut config = CavlyConfig::default();
    config.package.name = "test".to_string();
    config.package.version = "1.0.0".to_string();
    config.to_file(&temp.path().join("cavly.toml")).unwrap();

    cavvy::cavly::project::Project::add_system_lib(temp.path(), "m").unwrap();
    let result = cavvy::cavly::project::Project::add_system_lib(temp.path(), "m");
    // 第二次添加应提示已存在，但不会报错
    assert!(result.is_ok());

    // Git 依赖重复应报错（使用本地 Git 仓库测试）
    if std::process::Command::new("git")
        .arg("--version")
        .status()
        .is_ok()
    {
        let repo_path = temp.path().join("remote-dup");
        create_local_git_repo(&repo_path, "lib").unwrap();
        let repo_url = repo_path.to_string_lossy().to_string();

        cavvy::cavly::project::Project::add_git_dependency(
            temp.path(),
            "dup",
            &repo_url,
            Some("master"),
            None,
        )
        .unwrap();
        let result = cavvy::cavly::project::Project::add_git_dependency(
            temp.path(),
            "dup",
            &repo_url,
            Some("master"),
            None,
        );
        assert!(result.is_err());
    }
}
