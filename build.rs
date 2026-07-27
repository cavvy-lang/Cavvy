use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// 获取 git commit hash 和 dirty 状态
/// 返回格式: "abc1234" 或 "abc1234-dirty"
/// 自动忽略编译生成的二进制可执行文件（ELF 格式）
fn get_git_version() -> Option<String> {
    // 获取 short commit hash
    let output = Command::new("git")
        .args(&["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // 检查是否有未提交的修改（排除编译生成的二进制文件）
    let status_output = Command::new("git")
        .args(&["status", "--porcelain"])
        .output()
        .ok()?;

    let status_str = String::from_utf8_lossy(&status_output.stdout);

    // 过滤掉编译生成的二进制可执行文件（ELF 格式，无后缀）
    let is_dirty = status_str.lines().any(|line| {
        let line = line.trim();
        if line.is_empty() {
            return false;
        }

        // 提取文件名（git status --porcelain 格式: XY filename 或 ?? filename）
        let file_name = if line.starts_with("??") {
            line[2..].trim()
        } else if line.len() >= 3 {
            line[3..].trim()
        } else {
            return false;
        };

        // 跳过项目根目录下的 ELF 可执行文件（无后缀且是单个文件名）
        if !file_name.contains('/') && !file_name.contains('\\') && !file_name.contains('.') {
            // 检查是否是 ELF 文件
            let path = PathBuf::from(file_name);
            if is_elf_executable(&path) {
                return false; // 忽略 ELF 可执行文件
            }
        }

        true // 其他变更视为 dirty
    });

    if is_dirty {
        Some(format!("{}-dirty", commit))
    } else {
        Some(commit)
    }
}

/// 构建完整版本字符串: "version+commit" 或 "version+commit-dirty"
fn build_full_version(base_version: &str) -> String {
    match get_git_version() {
        Some(git_ver) => format!("{}+{}", base_version, git_ver),
        None => base_version.to_string(),
    }
}

fn is_full_system_llvm_prefix(path: &PathBuf) -> bool {
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("llvm-minimal"))
    {
        return false;
    }

    path.join("include").exists() && path.join("lib").exists()
}

fn discover_system_llvm_prefix() -> Option<String> {
    if let Ok(prefix) = env::var("LLVM_SYS_221_PREFIX") {
        let path = PathBuf::from(&prefix);
        if is_full_system_llvm_prefix(&path) {
            return Some(prefix);
        }

        println!(
            "cargo:warning=Ignoring LLVM_SYS_221_PREFIX={} because it is not a full system LLVM installation",
            prefix
        );
    }

    let output = Command::new("llvm-config").arg("--prefix").output().ok()?;
    if !output.status.success() {
        return None;
    }

    let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if prefix.is_empty() {
        return None;
    }

    let path = PathBuf::from(&prefix);
    if is_full_system_llvm_prefix(&path) {
        Some(prefix)
    } else {
        println!(
            "cargo:warning=llvm-config reported {}, but it is not a full system LLVM installation",
            prefix
        );
        None
    }
}

fn ensure_bundled_llvm_available(project_root: &str) {
    let llvm_path = PathBuf::from(project_root).join("llvm-minimal");
    if llvm_path.exists() {
        return;
    }

    println!(
        "cargo:warning=Bundled LLVM tools not found at {}, running setup-llvm.py",
        llvm_path.display()
    );
    let setup_result = Command::new("python")
        .args(&["setup-llvm.py"])
        .current_dir(project_root)
        .output();

    match setup_result {
        Ok(output) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("cargo:warning=setup-llvm.py failed: {}", stderr);
            } else {
                println!("cargo:warning=setup-llvm.py completed successfully");
            }
        }
        Err(e) => {
            println!("cargo:warning=Failed to run setup-llvm.py: {}", e);
        }
    }
}

fn parse_verinfo() -> Result<HashMap<String, HashMap<String, String>>, String> {
    let content =
        fs::read_to_string(".verinfo").map_err(|e| format!("Failed to read .verinfo: {}", e))?;

    let mut map = HashMap::new();
    let mut current_section = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            current_section = Some(line[1..line.len() - 1].to_string());
        } else if let Some(ref section) = current_section {
            if let Some(pos) = line.find('=') {
                let key = line[..pos].trim().to_string();
                let value = line[pos + 1..].trim().to_string();

                // 移除引号
                let value = if value.starts_with('"') && value.ends_with('"') {
                    value[1..value.len() - 1].to_string()
                } else {
                    value
                };

                map.entry(section.clone())
                    .or_insert_with(HashMap::new)
                    .insert(key, value);
            }
        }
    }

    // 空文件或缺少 CAYC 版本视为解析失败，走兜底分支，避免静默缺失版本宏
    if map.get("CAYC").and_then(|s| s.get("version")).is_none() {
        return Err(".verinfo 为空或缺少 [CAYC] version".to_string());
    }

    Ok(map)
}

/// .verinfo 中的工具版本段 → 编译期环境变量名
/// 注意：CAY-IR 的环境变量名历史上就带连字符（env!("CAY-IR_VERSION")），不可统一转换
const VERSION_SECTIONS: &[(&str, &str)] = &[
    ("CAYC", "CAYC_VERSION"),
    ("CAY-IR", "CAY-IR_VERSION"),
    ("IR2EXE", "IR2EXE_VERSION"),
    ("CAY-CHECK", "CAY_CHECK_VERSION"),
    ("CAY-RUN", "CAY_RUN_VERSION"),
    ("CAY-LSP", "CAY_LSP_VERSION"),
    ("CAY-DLL", "CAY_DLL_VERSION"),
    ("CAVLY", "CAVLY_VERSION"),
    ("CAY-PRE", "CAY_PRE_VERSION"),
    ("CAY-BCGEN", "CAY_BCGEN_VERSION"),
    ("CAY-DT", "CAY_DT_VERSION"),
    ("CAY-DP", "CAY_DP_VERSION"),
    ("CAY-RCPL", "CAY_RCPL_VERSION"),
    ("CAY-SETUP", "CAY_SETUP_VERSION"),
    ("CAY-AST", "CAY_AST_VERSION"),
    ("CAY-PL", "CAY_PL_VERSION"),
    ("CAY-SIR", "CAY_SIR_VERSION"),
];

fn main() {
    // 解析 .verinfo 文件
    match parse_verinfo() {
        Ok(verinfo) => {
            // 设置各工具的版本环境变量（带 commit hash）
            for (section, env_name) in VERSION_SECTIONS {
                if let Some(version) = verinfo.get(*section).and_then(|s| s.get("version")) {
                    println!(
                        "cargo:rustc-env={}={}",
                        env_name,
                        build_full_version(version)
                    );
                }
            }

            // 设置通用版本（使用CAYC的版本）
            if let Some(version) = verinfo.get("CAYC").and_then(|s| s.get("version")) {
                println!("cargo:rustc-env=VERSION={}", build_full_version(version));
            }
        }
        Err(e) => {
            eprintln!("Warning: Failed to parse .verinfo: {}", e);
            // 设置默认版本（带 commit hash），与 Cargo.toml 的 package.version 保持一致
            let default_version = env!("CARGO_PKG_VERSION");
            for (_, env_name) in VERSION_SECTIONS {
                println!(
                    "cargo:rustc-env={}={}",
                    env_name,
                    build_full_version(default_version)
                );
            }
            println!(
                "cargo:rustc-env=VERSION={}",
                build_full_version(default_version)
            );
        }
    }

    let project_root = env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    ensure_bundled_llvm_available(&project_root);

    // llvm-sys 需要完整的系统 LLVM；llvm-minimal 只用于运行时 clang/llc 工具。
    if let Some(llvm_prefix) = discover_system_llvm_prefix() {
        let llvm_path = PathBuf::from(&llvm_prefix);
        println!(
            "cargo:warning=Detected system LLVM for llvm-sys at {}",
            llvm_prefix
        );
        println!(
            "cargo:warning=LLVM include: {}",
            llvm_path.join("include").display()
        );
        println!(
            "cargo:warning=LLVM lib: {}",
            llvm_path.join("lib").display()
        );
        println!("cargo:rustc-env=LLVM_SYS_221_PREFIX={}", llvm_prefix);

        // 供 build.rs 后续逻辑或 cargo run/cargo test 子进程继承；不指向 llvm-minimal。
        unsafe {
            env::set_var("LLVM_SYS_221_PREFIX", &llvm_prefix);
        }
    } else {
        println!(
            "cargo:warning=System LLVM 22.1.x not found; llvm-sys must be built from a full system LLVM installation"
        );
        println!(
            "cargo:warning=Install LLVM or set LLVM_SYS_221_PREFIX to the full system LLVM prefix"
        );
    }

    // 获取输出目录
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = PathBuf::from(&out_dir);

    // 获取 profile (debug/release)
    let profile = env::var("PROFILE").unwrap();

    // 计算目标目录 (target/debug 或 target/release)
    let target_dir = out_path
        .ancestors()
        .find(|p| p.ends_with(&profile))
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| out_path.clone());

    // 检测目标平台
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let is_windows = target_os == "windows";
    let is_linux = target_os == "linux";

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.verinfo");
    // 嵌入 commit hash 需要感知提交切换；不监听 .git/index（任何 stage 操作都会触发全量重编）
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=llvm-minimal/");
    println!("cargo:rerun-if-changed=lib/");
    println!("cargo:rerun-if-changed=mingw-minimal/");
    println!("cargo:rerun-if-changed=third-party/");
    println!("cargo:rerun-if-changed=examples/");
    println!("cargo:rerun-if-changed=caylibs/");

    if is_windows {
        // Windows平台：复制Windows版LLVM工具
        if PathBuf::from("llvm-minimal").exists() {
            copy_dir_all("llvm-minimal", &target_dir.join("llvm-minimal"))
                .expect("Failed to copy llvm-minimal directory");
        }

        // 复制 MinGW 库（若项目包含捆绑库）
        if PathBuf::from("lib").exists() {
            copy_dir_all("lib", &target_dir.join("lib")).expect("Failed to copy lib directory");
        }

        // 复制 MinGW 运行时DLL（若项目包含）
        if PathBuf::from("mingw-minimal").exists() {
            copy_dir_all("mingw-minimal", &target_dir.join("mingw-minimal"))
                .expect("Failed to copy mingw-minimal directory");
        }

        println!(
            "cargo:warning=Copied Windows toolchain (LLVM + MinGW) to {}",
            target_dir.display()
        );
    } else if is_linux {
        // Linux平台：只复制Linux版LLVM工具
        // 创建 llvm-minimal 目录结构，但只复制 bin-linux
        let llvm_dst = target_dir.join("llvm-minimal");
        fs::create_dir_all(&llvm_dst).expect("Failed to create llvm-minimal directory");

        // 复制 bin-linux 到目标目录
        copy_dir_all("llvm-minimal/bin-linux", &llvm_dst.join("bin-linux"))
            .expect("Failed to copy llvm-minimal/bin-linux directory");

        // 复制 lib 目录（LLVM库是跨平台的）
        copy_dir_all("llvm-minimal/lib", &llvm_dst.join("lib"))
            .expect("Failed to copy llvm-minimal/lib directory");

        println!(
            "cargo:warning=Copied Linux toolchain (LLVM Linux binaries) to {}",
            target_dir.display()
        );
    } else {
        // 其他平台：复制完整的llvm-minimal
        copy_dir_all("llvm-minimal", &target_dir.join("llvm-minimal"))
            .expect("Failed to copy llvm-minimal directory");

        println!(
            "cargo:warning=Copied generic toolchain to {}",
            target_dir.display()
        );
    }

    // 复制 third-party 目录 (许可证文件) - 所有平台都需要
    copy_dir_all("third-party", &target_dir.join("third-party"))
        .expect("Failed to copy third-party directory");

    // 复制 examples 目录 - 所有平台都需要
    copy_dir_all("examples", &target_dir.join("examples"))
        .expect("Failed to copy examples directory");

    // 复制 caylibs 目录 - 所有平台都需要（系统库目录）
    copy_dir_all("caylibs", &target_dir.join("caylibs")).expect("Failed to copy caylibs directory");

    println!(
        "cargo:warning=Copied examples and caylibs directories to {}",
        target_dir.display()
    );
}

/// 检查文件是否是 ELF 可执行文件
/// 时间复杂度: O(1) - 只读取文件头
/// 空间复杂度: O(1)
fn is_elf_executable(path: &std::path::Path) -> bool {
    use std::io::Read;

    let mut file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };

    // ELF 文件头魔数: 0x7F 'E' 'L' 'F'
    let mut header = [0u8; 4];
    match file.read_exact(&mut header) {
        Ok(_) => header == [0x7F, 0x45, 0x4C, 0x46], // \x7F ELF
        Err(_) => false,
    }
}

fn copy_dir_all(
    src: impl AsRef<std::path::Path>,
    dst: impl AsRef<std::path::Path>,
) -> std::io::Result<()> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    if !src.exists() {
        return Ok(());
    }

    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let dest_path = dst.join(&file_name);

        if path.is_dir() {
            copy_dir_all(&path, &dest_path)?;
        } else {
            // 只在文件不存在或源文件更新时才复制
            let should_copy = if dest_path.exists() {
                let src_meta = fs::metadata(&path)?;
                let dst_meta = fs::metadata(&dest_path)?;
                src_meta
                    .modified()
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                    > dst_meta
                        .modified()
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            } else {
                true
            };

            if should_copy {
                fs::copy(&path, &dest_path)?;
            }
        }
    }

    Ok(())
}
