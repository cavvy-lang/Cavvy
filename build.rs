use std::env;
use std::fs;
use std::path::PathBuf;
use std::collections::HashMap;

fn parse_verinfo() -> Result<HashMap<String, HashMap<String, String>>, String> {
    let content = fs::read_to_string(".verinfo")
        .map_err(|e| format!("Failed to read .verinfo: {}", e))?;
    
    let mut map = HashMap::new();
    let mut current_section = None;
    
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        
        if line.starts_with('[') && line.ends_with(']') {
            current_section = Some(line[1..line.len()-1].to_string());
        } else if let Some(ref section) = current_section {
            if let Some(pos) = line.find('=') {
                let key = line[..pos].trim().to_string();
                let value = line[pos+1..].trim().to_string();
                
                // 移除引号
                let value = if value.starts_with('"') && value.ends_with('"') {
                    value[1..value.len()-1].to_string()
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
            // 设置各工具的版本环境变量
            if let Some(eolc_section) = verinfo.get("EOLC") {
                if let Some(version) = eolc_section.get("version") {
                    println!("cargo:rustc-env=EOLC_VERSION={}", version);
                }
            }
            if let Some(eolll_section) = verinfo.get("EOLLL") {
                if let Some(version) = eolll_section.get("version") {
                    println!("cargo:rustc-env=EOLLL_VERSION={}", version);
                }
            }
            if let Some(ir2exe_section) = verinfo.get("IR2EXE") {
                if let Some(version) = ir2exe_section.get("version") {
                    println!("cargo:rustc-env=IR2EXE_VERSION={}", version);
                }
            }
            if let Some(eol_check_section) = verinfo.get("EOL_CHECK") {
                if let Some(version) = eol_check_section.get("version") {
                    println!("cargo:rustc-env=EOL_CHECK_VERSION={}", version);
                }
            }
            
            // 设置通用版本（使用EOLC的版本）
            if let Some(eolc_section) = verinfo.get("EOLC") {
                if let Some(version) = eolc_section.get("version") {
                    println!("cargo:rustc-env=VERSION={}", version);
                }
            }
        }
        Err(e) => {
            eprintln!("Warning: Failed to parse .verinfo: {}", e);
            // 设置默认版本
            println!("cargo:rustc-env=EOLC_VERSION=0.3.2.0");
            println!("cargo:rustc-env=EOLLL_VERSION=0.3.2.0");
            println!("cargo:rustc-env=IR2EXE_VERSION=0.3.2.0");
            println!("cargo:rustc-env=EOL_CHECK_VERSION=0.3.2.0");
            println!("cargo:rustc-env=VERSION=0.3.2.0");
        }
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
    
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.verinfo");
    println!("cargo:rerun-if-changed=llvm-minimal/");
    println!("cargo:rerun-if-changed=lib/");
    println!("cargo:rerun-if-changed=mingw-minimal/");
    println!("cargo:rerun-if-changed=third-party/");
    
    // 复制 llvm-minimal 目录
    copy_dir_all("llvm-minimal", &target_dir.join("llvm-minimal"))
        .expect("Failed to copy llvm-minimal directory");
    
    // 复制 lib 目录
    copy_dir_all("lib", &target_dir.join("lib"))
        .expect("Failed to copy lib directory");
    
    // 复制 mingw-minimal 目录
    copy_dir_all("mingw-minimal", &target_dir.join("mingw-minimal"))
        .expect("Failed to copy mingw-minimal directory");
    
    // 复制 third-party 目录 (许可证文件)
    copy_dir_all("third-party", &target_dir.join("third-party"))
        .expect("Failed to copy third-party directory");
    
    println!("cargo:warning=Copied toolchain and license directories to {}", target_dir.display());
}

fn copy_dir_all(src: impl AsRef<std::path::Path>, dst: impl AsRef<std::path::Path>) -> std::io::Result<()> {
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
                src_meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH) > 
                    dst_meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH)
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
