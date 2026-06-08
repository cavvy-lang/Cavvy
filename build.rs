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

    Ok(map)
}

fn main() {
    // 解析 .verinfo 文件
    match parse_verinfo() {
        Ok(verinfo) => {
            // 设置各工具的版本环境变量（带 commit hash）
            if let Some(cayc_section) = verinfo.get("CAYC") {
                if let Some(version) = cayc_section.get("version") {
                    println!(
                        "cargo:rustc-env=CAYC_VERSION={}",
                        build_full_version(version)
                    );
                }
            }
            if let Some(cay_ir_section) = verinfo.get("CAY-IR") {
                if let Some(version) = cay_ir_section.get("version") {
                    println!(
                        "cargo:rustc-env=CAY-IR_VERSION={}",
                        build_full_version(version)
                    );
                }
            }
            if let Some(ir2exe_section) = verinfo.get("IR2EXE") {
                if let Some(version) = ir2exe_section.get("version") {
                    println!(
                        "cargo:rustc-env=IR2EXE_VERSION={}",
                        build_full_version(version)
                    );
                }
            }
            if let Some(cay_check_section) = verinfo.get("CAY-CHECK") {
                if let Some(version) = cay_check_section.get("version") {
                    println!(
                        "cargo:rustc-env=CAY_CHECK_VERSION={}",
                        build_full_version(version)
                    );
                }
            }
            if let Some(cay_run_section) = verinfo.get("CAY-RUN") {
                if let Some(version) = cay_run_section.get("version") {
                    println!(
                        "cargo:rustc-env=CAY_RUN_VERSION={}",
                        build_full_version(version)
                    );
                }
            }
            if let Some(cay_lsp_section) = verinfo.get("CAY-LSP") {
                if let Some(version) = cay_lsp_section.get("version") {
                    println!(
                        "cargo:rustc-env=CAY_LSP_VERSION={}",
                        build_full_version(version)
                    );
                }
            }
            if let Some(cay_dll_section) = verinfo.get("CAY-DLL") {
                if let Some(version) = cay_dll_section.get("version") {
                    println!(
                        "cargo:rustc-env=CAY_DLL_VERSION={}",
                        build_full_version(version)
                    );
                }
            }

            // 设置 Cavly 版本
            if let Some(cavly_section) = verinfo.get("CAVLY") {
                if let Some(version) = cavly_section.get("version") {
                    println!(
                        "cargo:rustc-env=CAVLY_VERSION={}",
                        build_full_version(version)
                    );
                }
            }

            // 设置 CAY-PRE 版本
            if let Some(cay_pre_section) = verinfo.get("CAY-PRE") {
                if let Some(version) = cay_pre_section.get("version") {
                    println!(
                        "cargo:rustc-env=CAY_PRE_VERSION={}",
                        build_full_version(version)
                    );
                }
            }

            // 设置 CAY-BCGEN 版本
            if let Some(cay_bcgen_section) = verinfo.get("CAY-BCGEN") {
                if let Some(version) = cay_bcgen_section.get("version") {
                    println!(
                        "cargo:rustc-env=CAY_BCGEN_VERSION={}",
                        build_full_version(version)
                    );
                }
            }

            // 设置 CAY-DT 版本
            if let Some(cay_dt_section) = verinfo.get("CAY-DT") {
                if let Some(version) = cay_dt_section.get("version") {
                    println!(
                        "cargo:rustc-env=CAY_DT_VERSION={}",
                        build_full_version(version)
                    );
                }
            }

            // 设置 CAY-DP 版本
            if let Some(cay_dp_section) = verinfo.get("CAY-DP") {
                if let Some(version) = cay_dp_section.get("version") {
                    println!(
                        "cargo:rustc-env=CAY_DP_VERSION={}",
                        build_full_version(version)
                    );
                }
            }

            // 设置 CAY-RCPL 版本
            if let Some(cay_rcpl_section) = verinfo.get("CAY-RCPL") {
                if let Some(version) = cay_rcpl_section.get("version") {
                    println!(
                        "cargo:rustc-env=CAY_RCPL_VERSION={}",
                        build_full_version(version)
                    );
                }
            }

            // 设置通用版本（使用CAYC的版本）
            if let Some(cayc_section) = verinfo.get("CAYC") {
                if let Some(version) = cayc_section.get("version") {
                    println!("cargo:rustc-env=VERSION={}", build_full_version(version));
                }
            }
        }
        Err(e) => {
            eprintln!("Warning: Failed to parse .verinfo: {}", e);
            // 设置默认版本（带 commit hash）
            let default_version = "5.1.0-Alpha.3";
            println!(
                "cargo:rustc-env=CAYC_VERSION={}",
                build_full_version(default_version)
            );
            println!(
                "cargo:rustc-env=CAY-IR_VERSION={}",
                build_full_version(default_version)
            );
            println!(
                "cargo:rustc-env=IR2EXE_VERSION={}",
                build_full_version(default_version)
            );
            println!(
                "cargo:rustc-env=CAY_CHECK_VERSION={}",
                build_full_version(default_version)
            );
            println!(
                "cargo:rustc-env=CAY_RUN_VERSION={}",
                build_full_version(default_version)
            );
            println!(
                "cargo:rustc-env=CAY_LSP_VERSION={}",
                build_full_version(default_version)
            );
            println!(
                "cargo:rustc-env=CAY_DLL_VERSION={}",
                build_full_version(default_version)
            );
            println!(
                "cargo:rustc-env=CAVLY_VERSION={}",
                build_full_version(default_version)
            );
            println!(
                "cargo:rustc-env=CAY_PRE_VERSION={}",
                build_full_version(default_version)
            );
            println!(
                "cargo:rustc-env=CAY_BCGEN_VERSION={}",
                build_full_version(default_version)
            );
            println!(
                "cargo:rustc-env=CAY_DT_VERSION={}",
                build_full_version(default_version)
            );
            println!(
                "cargo:rustc-env=CAY_DP_VERSION={}",
                build_full_version(default_version)
            );
            println!(
                "cargo:rustc-env=CAY_RCPL_VERSION={}",
                build_full_version(default_version)
            );
            println!(
                "cargo:rustc-env=VERSION={}",
                build_full_version(default_version)
            );
        }
    }

    // 设置 LLVM_SYS_221_PREFIX 环境变量（供 llvm-sys crate 使用）
    // 优先使用已存在的环境变量，否则使用项目目录下的 llvm-minimal
    let llvm_prefix = env::var("LLVM_SYS_221_PREFIX")
        .unwrap_or_else(|_| {
            let project_root = env::var("CARGO_MANIFEST_DIR")
                .unwrap_or_else(|_| ".".to_string());
            let llvm_path = PathBuf::from(&project_root).join("llvm-minimal");
            llvm_path.to_string_lossy().to_string()
        });

    // 检查 LLVM 目录是否存在，如果不存在则运行 setup-llvm.py
    let llvm_path = PathBuf::from(&llvm_prefix);
    if !llvm_path.exists() {
        println!("cargo:warning=LLVM not found at {}, running setup-llvm.py", llvm_prefix);
        let setup_result = Command::new("python")
            .args(&["setup-llvm.py"])
            .current_dir(&env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()))
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

    // 检查是否是完整LLVM安装（包含include和lib目录）
    let llvm_include = llvm_path.join("include");
    let llvm_lib = llvm_path.join("lib");
    if llvm_include.exists() && llvm_lib.exists() {
        println!("cargo:warning=Detected full LLVM installation at {}", llvm_prefix);
        println!("cargo:warning=LLVM include: {}", llvm_include.display());
        println!("cargo:warning=LLVM lib: {}", llvm_lib.display());
    } else {
        println!("cargo:warning=Minimal LLVM installation detected (no include/lib dirs)");
        println!("cargo:warning=For llvm-sys support, set CAVVY_USE_FULL_LLVM=1 to download full LLVM dev package");
    }

    // 设置环境变量供 llvm-sys 使用
    // 注意：必须在编译任何依赖llvm-sys的crate之前设置
    println!("cargo:rustc-env=LLVM_SYS_221_PREFIX={}", llvm_prefix);
    // 使用unsafe块设置环境变量（Rust 2024 edition要求）
    // 这是安全的，因为我们在build.rs主线程中执行，没有并发问题
    unsafe {
        env::set_var("LLVM_SYS_221_PREFIX", &llvm_prefix);
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
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");
    println!("cargo:rerun-if-changed=llvm-minimal/");
    println!("cargo:rerun-if-changed=lib/");
    println!("cargo:rerun-if-changed=mingw-minimal/");
    println!("cargo:rerun-if-changed=third-party/");
    println!("cargo:rerun-if-changed=examples/");
    println!("cargo:rerun-if-changed=caylibs/");

    if is_windows {
        // Windows平台：复制Windows版LLVM工具
        copy_dir_all("llvm-minimal", &target_dir.join("llvm-minimal"))
            .expect("Failed to copy llvm-minimal directory");

        // 复制 MinGW 库
        copy_dir_all("lib", &target_dir.join("lib")).expect("Failed to copy lib directory");

        // 复制 MinGW 运行时DLL
        copy_dir_all("mingw-minimal", &target_dir.join("mingw-minimal"))
            .expect("Failed to copy mingw-minimal directory");

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
