use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Result, Context, bail};
use crate::cavly::config::{CavlyConfig, BinTarget, ProjectType};
use crate::cavly::workspace::{WorkspaceResolver, ResolvedDependency, topological_sort};
use crate::cavly::{ensure_dir, TARGET_DIR};

/// 构建器状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildState {
    Idle,
    /// 执行预构建脚本
    PreBuild,
    Compiling,
    Linking,
    Complete,
    Failed,
}

/// Cavly 构建器
/// 
/// 通过调用 cayc 编译器实现构建，确保与直接调用 cayc 的行为一致
pub struct Builder {
    /// 项目根目录
    project_root: PathBuf,
    /// 构建配置
    config: CavlyConfig,
    /// 当前状态
    state: BuildState,
    /// 是否 verbose 模式
    verbose: bool,
    /// 解析后的依赖列表
    dependencies: Vec<ResolvedDependency>,
}

impl Builder {
    /// 创建新的构建器
    /// 
    /// # 复杂度
    /// - 时间: O(1)
    /// - 空间: O(1)
    pub fn new(project_root: PathBuf, config: CavlyConfig) -> Self {
        Self {
            project_root,
            config,
            state: BuildState::Idle,
            verbose: false,
            dependencies: Vec::new(),
        }
    }
    
    /// 创建新的构建器并解析依赖
    /// 
    /// # 复杂度
    /// - 时间: O(n*m)，n 为依赖数量，m 为每个依赖的配置大小
    /// - 空间: O(n)
    pub fn with_dependencies(project_root: PathBuf, mut config: CavlyConfig) -> Result<Self> {
        let mut resolver = WorkspaceResolver::new(project_root.clone());
        
        // 解析所有依赖
        let dependencies = resolver.resolve_all(&config)?;
        
        // 拓扑排序依赖（确保被依赖的先构建）
        let sorted_deps = topological_sort(&dependencies)?;
        
        // 合并所有依赖的配置到主配置
        resolver.merge_dependencies_config(&mut config, &sorted_deps);
        
        Ok(Self {
            project_root,
            config,
            state: BuildState::Idle,
            verbose: false,
            dependencies: sorted_deps,
        })
    }
    
    /// 设置 verbose 模式
    pub fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }
    
    /// 获取当前状态
    pub fn state(&self) -> BuildState {
        self.state
    }
    
    /// 执行完整构建流程
    /// 
    /// # 流程
    /// 0. 执行构建脚本（如果配置了 build.cay）
    /// 1. 首先构建所有依赖库（如果是 lib 项目）
    /// 2. 验证源文件存在
    /// 3. 查找 cayc 编译器
    /// 4. 构建 cayc 命令行参数
    /// 5. 调用 cayc 执行编译
    /// 
    /// # 复杂度
    /// - 时间: O(n + m)，n 为源码大小，m 为链接复杂度
    /// - 空间: O(n) 临时文件
    pub fn build(&mut self) -> Result<PathBuf> {
        // 0. 执行构建脚本（如果配置了）
        self.execute_build_script()?;
        
        self.state = BuildState::Compiling;
        
        // 1. 先构建所有依赖库
        self.build_dependencies()?;
        
        // 2. 验证源文件
        let source_path = self.config.main_source_path(&self.project_root);
        if !source_path.exists() {
            bail!("主源文件不存在: {}", source_path.display());
        }
        
        // 3. 准备目标目录
        let target_dir = self.config.target_path(&self.project_root);
        ensure_dir(&target_dir)?;
        
        // 4. 查找 cayc 编译器
        let cayc_path = find_cayc()?;
        
        // 5. 确定输出文件路径
        let output_path = self.determine_output_path(&target_dir)?;
        
        // 6. 构建 cayc 命令行参数
        let args = self.build_cayc_args(&source_path, &output_path, None)?;
        
        if self.verbose {
            if self.config.is_lib() && self.config.lib.only_include {
                println!("Cavly: 项目: {} v{} (库 - 仅接口/only_include)", 
                    self.config.package.name, 
                    self.config.package.version
                );
            } else {
                println!("Cavly: 项目: {} v{} ({})", 
                    self.config.package.name, 
                    self.config.package.version,
                    if self.config.is_lib() { "库" } else { "可执行" }
                );
            }
            println!("Cavly: 调用: {} {}", 
                cayc_path.display(),
                args.join(" ")
            );
        }

        // 7. 执行 cayc 编译
        self.state = BuildState::Linking;
        
        self.invoke_cayc(&cayc_path, &args, &output_path)?;
        
        // 8. 如果是库项目且不是 only_include，安装到 lib 目录
        if self.config.is_lib() && !self.config.lib.only_include {
            self.install_library(&output_path)?;
        }
        
        self.state = BuildState::Complete;
        
        if self.verbose {
            println!("Cavly: 构建成功: {}", output_path.display());
        }
        
        Ok(output_path)
    }
    
    /// 构建所有默认的二进制目标
    /// 
    /// 这是 `cavly build` 的新默认行为：
    /// - 如果定义了 `[[bin]]`，构建所有 `default_build = true` 的 bin
    /// - 如果没有 `[[bin]]`，回退到 package.main（向后兼容）
    /// 
    /// # 返回
    /// 所有成功构建的输出文件路径列表
    pub fn build_all_bins(&mut self) -> Result<Vec<PathBuf>> {
        // 0. 执行构建脚本
        self.execute_build_script()?;
        
        // 1. 构建依赖
        self.build_dependencies()?;
        
        // 2. 获取有效的 bin 目标
        let bins = self.config.default_bins();
        
        if bins.is_empty() {
            // 无 bin 目标（可能是纯库项目），使用旧的 build() 路径
            return self.build().map(|p| vec![p]);
        }
        
        let target_dir = self.config.target_path(&self.project_root);
        ensure_dir(&target_dir)?;
        
        let cayc_path = find_cayc()?;
        
        let mut outputs = Vec::new();
        
        for bin in &bins {
            let source_path = self.project_root.join(&bin.path);
            if !source_path.exists() {
                if self.verbose {
                    println!("Cavly: 跳过不存在的 bin 源文件: {}", source_path.display());
                }
                continue;
            }
            
            let output_path = self.determine_bin_output_path(&target_dir, bin)?;
            
            if self.verbose {
                println!("Cavly: 构建 bin: {} ({})", bin.name, bin.path);
                println!("Cavly:   输出: {}", output_path.display());
            }
            
            let args = self.build_cayc_args(&source_path, &output_path, Some(bin))?;
            
            if self.verbose {
                println!("Cavly:   调用: {} {}", cayc_path.display(), args.join(" "));
            }
            
            self.invoke_cayc(&cayc_path, &args, &output_path)?;
            outputs.push(output_path);
        }
        
        self.state = BuildState::Complete;
        
        if self.verbose {
            println!("Cavly: 所有 bin 构建完成 ({} 个)", outputs.len());
        }
        
        Ok(outputs)
    }
    
    /// 按名称构建指定的二进制目标
    pub fn build_bin_by_name(&mut self, name: &str) -> Result<PathBuf> {
        // 0. 执行构建脚本
        self.execute_build_script()?;
        
        // 1. 构建依赖
        self.build_dependencies()?;
        
        // 2. 查找 bin
        let bin = self.config.effective_bins()
            .into_iter()
            .find(|b| b.name == name)
            .ok_or_else(|| anyhow::anyhow!("找不到二进制目标: '{}'", name))?;
        
        let target_dir = self.config.target_path(&self.project_root);
        ensure_dir(&target_dir)?;
        
        let cayc_path = find_cayc()?;
        
        let source_path = self.project_root.join(&bin.path);
        if !source_path.exists() {
            bail!("bin '{}' 的源文件不存在: {}", name, source_path.display());
        }
        
        let output_path = self.determine_bin_output_path(&target_dir, &bin)?;
        
        if self.verbose {
            println!("Cavly: 构建 bin: {} ({})", bin.name, bin.path);
        }
        
        let args = self.build_cayc_args(&source_path, &output_path, Some(&bin))?;
        self.invoke_cayc(&cayc_path, &args, &output_path)?;
        
        self.state = BuildState::Complete;
        
        Ok(output_path)
    }
    
    /// 执行构建脚本（build.cay）
    /// 
    /// # 流程
    /// 1. 检查是否配置了 build_script
    /// 2. 编译 build.cay → target/build-script/build.exe
    /// 3. 运行 build.exe，传入环境变量
    /// 4. 检查退出码
    fn execute_build_script(&mut self) -> Result<()> {
        let script_path = match self.config.build_script_path(&self.project_root) {
            Some(p) if p.exists() => p,
            Some(p) => {
                // 配置了但文件不存在
                bail!("构建脚本不存在: {}", p.display());
            }
            None => return Ok(()),  // 没有配置构建脚本，跳过
        };
        
        self.state = BuildState::PreBuild;
        
        let build_dir = self.config.build_script_dir(&self.project_root);
        ensure_dir(&build_dir)?;
        
        let cayc_path = find_cayc()?;
        
        // 确定 build.exe 输出路径
        let build_exe = if cfg!(target_os = "windows") {
            build_dir.join("build.exe")
        } else {
            build_dir.join("build")
        };
        
        if self.verbose {
            println!("Cavly: 执行构建脚本: {}", script_path.display());
            println!("Cavly:   编译构建脚本: {} {}", cayc_path.display(), script_path.display());
        }
        
        // 编译构建脚本为可执行文件
        // 构建脚本使用 -O0 以加快编译速度（脚本通常很小）
        let build_args = vec![
            "-O0".to_string(),
            script_path.to_string_lossy().to_string(),
            build_exe.to_string_lossy().to_string(),
        ];
        
        let output = Command::new(&cayc_path)
            .args(&build_args)
            .current_dir(&self.project_root)
            .output()
            .with_context(|| format!("编译构建脚本失败: {}", script_path.display()))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            bail!("构建脚本编译失败:\nstdout:\n{}\nstderr:\n{}", stdout, stderr);
        }
        
        if !build_exe.exists() {
            bail!("构建脚本编译未生成可执行文件: {}", build_exe.display());
        }
        
        if self.verbose {
            println!("Cavly:   运行构建脚本: {}", build_exe.display());
        }
        
        // 运行构建脚本，设置标准环境变量
        let target_dir = self.config.target_path(&self.project_root);
        let out_dir = target_dir.join("build-script-out");
        ensure_dir(&out_dir)?;
        
        let status = Command::new(&build_exe)
            .env("OUT_DIR", &out_dir)
            .env("PROJECT_ROOT", &self.project_root)
            .env("PROFILE", if self.config.build.debug { "debug" } else { "release" })
            .env("OPT_LEVEL", &self.config.build.opt_level)
            .env("TARGET", self.config.build.target.as_deref().unwrap_or("native"))
            .current_dir(&self.project_root)
            .status()
            .with_context(|| format!("运行构建脚本失败: {}", build_exe.display()))?;
        
        if !status.success() {
            bail!("构建脚本退出码: {:?}", status.code());
        }
        
        if self.verbose {
            println!("Cavly: 构建脚本执行成功");
        }
        
        Ok(())
    }
    
    /// 确定 bin 的输出文件路径
    fn determine_bin_output_path(&self, target_dir: &Path, bin: &BinTarget) -> Result<PathBuf> {
        if self.is_windows_target() {
            Ok(target_dir.join(format!("{}.exe", bin.output_basename())))
        } else {
            Ok(target_dir.join(bin.output_basename()))
        }
    }
    
    /// 调用 cayc 编译并检查结果（核心编译函数）
    fn invoke_cayc(&self, cayc_path: &Path, args: &[String], expected_output: &Path) -> Result<()> {
        let output = Command::new(cayc_path)
            .args(args)
            .current_dir(&self.project_root)
            .output()
            .with_context(|| format!("执行 cayc 失败: {}", cayc_path.display()))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            bail!("编译失败:\nstdout:\n{}\nstderr:\n{}", stdout, stderr);
        }
        
        if !expected_output.exists() {
            bail!("编译未生成输出文件: {}", expected_output.display());
        }
        
        Ok(())
    }
    
    /// 构建所有依赖库
    /// 
    /// # 复杂度
    /// - 时间: O(n*m)，n 为依赖数量，m 为每个依赖的构建时间
    /// - 空间: O(n)
    fn build_dependencies(&mut self) -> Result<()> {
        if self.dependencies.is_empty() {
            return Ok(());
        }

        if self.verbose {
            println!("Cavly: 开始构建 {} 个依赖...", self.dependencies.len());
        }

        for dep in &self.dependencies {
            // 跳过 only_include 依赖：它们只做接口检查，不产出 .lib
            if dep.config.lib.only_include {
                if self.verbose {
                    println!("Cavly: 跳过 only_include 依赖: {}", dep.name);
                }
                continue;
            }

            if self.verbose {
                println!("Cavly: 构建依赖: {} @ {}", dep.name, dep.path.display());
            }

            // 为每个依赖创建构建器
            let mut dep_builder = Builder::new(dep.path.clone(), dep.config.clone())
                .verbose(self.verbose);

            dep_builder.build()?;
        }

        if self.verbose {
            println!("Cavly: 依赖构建完成");
        }

        Ok(())
    }

    /// 确定输出文件路径
    /// 
    /// # 复杂度
    /// - 时间: O(1)
    /// - 空间: O(1)
    fn determine_output_path(&self, target_dir: &Path) -> Result<PathBuf> {
        match self.config.package.project_type {
            ProjectType::Bin => {
                let output_name = self.config.output_filename();
                if self.is_windows_target() {
                    Ok(target_dir.join(format!("{}.exe", output_name)))
                } else {
                    Ok(target_dir.join(&output_name))
                }
            }
            ProjectType::Lib => {
                if self.config.lib.only_include {
                    // only_include 模式：只生成 IR 文件，不链接成库
                    let ir_dir = target_dir.join("ir");
                    ensure_dir(&ir_dir)?;
                    let ir_name = format!("{}.ll", self.config.output_filename());
                    Ok(ir_dir.join(ir_name))
                } else {
                    // 库项目输出到 target/lib 目录
                    let lib_dir = self.config.lib_install_path(&self.project_root);
                    ensure_dir(&lib_dir)?;

                    let lib_filename = self.config.lib_output_filename();
                    Ok(lib_dir.join(lib_filename))
                }
            }
        }
    }
    
    /// 安装库文件
    /// 
    /// # 复杂度
    /// - 时间: O(1)
    /// - 空间: O(1)
    fn install_library(&self, output_path: &Path) -> Result<()> {
        // only_include 模式不产出库文件，无需安装
        if self.config.lib.only_include {
            return Ok(());
        }

        let lib_dir = self.config.lib_install_path(&self.project_root);
        ensure_dir(&lib_dir)?;
        
        // 复制库文件到安装目录
        let lib_filename = self.config.lib_output_filename();
        let install_path = lib_dir.join(&lib_filename);
        
        if output_path != install_path {
            std::fs::copy(output_path, &install_path)
                .with_context(|| format!("安装库文件失败: {} -> {}", 
                    output_path.display(), install_path.display()))?;
        }
        
        // TODO: 生成头文件（如果配置了）
        if self.config.lib.header.generate {
            self.generate_header(&lib_dir)?;
        }
        
        if self.verbose {
            println!("Cavly: 库已安装到: {}", lib_dir.display());
        }
        
        Ok(())
    }
    
    /// 生成 C 头文件
    /// 
    /// 解析库的公共接口并生成对应的 C 头文件声明。
    /// 支持函数声明、类型定义和常量定义。
    /// 
    /// # 复杂度
    /// - 时间: O(n)，n 为导出的符号数量
    /// - 空间: O(n)
    fn generate_header(&self, lib_dir: &Path) -> Result<()> {
        let header_name = self.config.lib.header.name.clone()
            .unwrap_or_else(|| format!("{}.h", self.config.package.name));
        
        let header_path = lib_dir.join(&header_name);
        let guard_name = self.config.package.name.to_uppercase().replace('-', "_");
        
        let mut header_content = format!(r#"/* Cavvy Library Header - Auto Generated */
/* Package: {} */
/* Version: {} */
#ifndef {}_H
#define {}_H

#ifdef __cplusplus
extern "C" {{
#endif

"#, 
            self.config.package.name,
            self.config.package.version,
            guard_name,
            guard_name
        );
        
        // 扫描源文件提取公共接口
        let src_dir = self.project_root.join("src");
        if src_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&src_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map_or(false, |e| e == "cay" || e == "eol") {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            self.extract_public_declarations(&content, &mut header_content);
                        }
                    }
                }
            }
        }
        
        header_content.push_str(&format!(r#"
#ifdef __cplusplus
}}
#endif

#endif /* {}_H */
"#, guard_name));
        
        std::fs::write(&header_path, header_content)
            .with_context(|| format!("写入头文件失败: {}", header_path.display()))?;
        
        if self.verbose {
            println!("Cavly: 头文件已生成: {}", header_path.display());
        }
        
        Ok(())
    }
    
    /// 从源文件中提取公共声明并生成 C 头文件内容
    fn extract_public_declarations(&self, source: &str, header: &mut String) {
        let mut in_public_block = false;
        
        for line in source.lines() {
            let trimmed = line.trim();
            
            // 跳过空行和注释
            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") {
                continue;
            }
            
            // 检测 extern 块开始
            if trimmed.starts_with("extern") && trimmed.contains('{') {
                in_public_block = true;
                continue;
            }
            
            // 检测 extern 块结束
            if in_public_block && trimmed == "}" {
                in_public_block = false;
                continue;
            }
            
            // 在 extern 块内，提取函数声明
            if in_public_block {
                if trimmed.starts_with("int") || trimmed.starts_with("void") || 
                   trimmed.starts_with("float") || trimmed.starts_with("double") ||
                   trimmed.starts_with("char") || trimmed.starts_with("long") ||
                   trimmed.starts_with("bool") || trimmed.starts_with("size_t") {
                    // 简化的函数声明提取
                    let decl = trimmed.trim_end_matches(';').trim();
                    header.push_str(&format!("{};\n\n", decl));
                }
            }
            
            // 检测 public class 或 public static 方法
            if trimmed.starts_with("public") && trimmed.contains("static") {
                // 提取静态方法声明
                if let Some(method_start) = trimmed.find("static") {
                    let method_decl = &trimmed[method_start..];
                    if let Some(semi_pos) = method_decl.find(';') {
                        let decl = method_decl[..semi_pos].trim();
                        header.push_str(&format!("/* static */ {};\n\n", decl));
                    }
                }
            }
        }
    }
    
    /// 构建 cayc 命令行参数
    /// 
    /// # 参数
    /// - `source_path`: 源文件路径
    /// - `output_path`: 输出文件路径
    /// - `bin`: 可选的二进制目标，用于 bin 级别的构建配置覆盖
    /// 
    /// # 复杂度
    /// - 时间: O(n)，n 为配置参数数量
    /// - 空间: O(n)
    fn build_cayc_args(&self, source_path: &Path, output_path: &Path, bin: Option<&BinTarget>) -> Result<Vec<String>> {
        let mut args = Vec::new();

        // 使用 bin 级别的构建配置（如果存在），否则使用全局配置
        let effective_build = bin
            .and_then(|b| b.build.as_ref())
            .unwrap_or(&self.config.build);

        // only_include 模式：只编译检查，不链接任何库
        let is_only_include = self.config.is_lib() && self.config.lib.only_include;

        // 优化级别
        args.push(format!("-O{}", effective_build.opt_level));
        
        // 调试信息
        if effective_build.debug {
            args.push("-g".to_string());
        }
        
        // 静态链接（only_include 模式不需要）
        if !is_only_include && effective_build.static_link {
            args.push("--static".to_string());
        }
        
        // LTO
        if effective_build.lto {
            if effective_build.opt_ir {
                // thin LTO
                args.push("--lto=thin".to_string());
            } else {
                args.push("--lto=full".to_string());
            }
        }
        
        // 目标平台
        if let Some(ref target) = effective_build.target {
            args.push("--target".to_string());
            args.push(target.clone());
        }
        
        // IR 优化
        if effective_build.opt_ir {
            args.push("--opt-ir".to_string());
        }
        
        // 保留 IR
        if effective_build.keep_ir {
            args.push("--keep-ir".to_string());
        }
        
        // only_include 模式不链接任何库（包括依赖库和 FFI 库）
        if !is_only_include {
            // 添加依赖库的搜索路径（跳过 only_include 依赖，它们不产 .lib）
            for dep in &self.dependencies {
                if dep.config.lib.only_include {
                    continue;
                }
                let lib_path = dep.config.lib_install_path(&dep.path);
                if lib_path.exists() {
                    args.push(format!("-L{}", lib_path.display()));
                }
            }

            // 库搜索路径（包括依赖的）
            for path in self.config.all_lib_paths() {
                args.push(format!("-L{}", path));
            }

            // 链接依赖库（跳过 only_include 的依赖，它们没有 .lib 产物）
            for dep in &self.dependencies {
                if dep.config.lib.only_include {
                    continue;
                }
                let lib_name = dep.config.output_filename();
                args.push(format!("-l{}", lib_name));
            }

            // 链接的库（包括 FFI 库）
            for lib in self.config.all_libs() {
                args.push(format!("-l{}", lib));
            }
        }

        // 添加依赖的源代码目录作为包含路径（供 #include 使用）
        for dep in &self.dependencies {
            let dep_src = dep.path.join(&dep.config.package.src_dir);
            if dep_src.exists() {
                args.push(format!("-I{}", dep_src.display()));
            }
        }

        // 额外的 cflags
        if !effective_build.cflags.is_empty() {
            args.push("--cflags".to_string());
            args.push(effective_build.cflags.join(" "));
        }
        
        // 额外的 ldflags
        if !effective_build.ldflags.is_empty() {
            args.push("--ldflags".to_string());
            args.push(effective_build.ldflags.join(" "));
        }
        
        // 输入文件（相对于项目根目录的路径）
        args.push(source_path.to_string_lossy().to_string());
        
        // 输出文件
        args.push(output_path.to_string_lossy().to_string());
        
        Ok(args)
    }
    
    /// 检查是否为 Windows 目标
    fn is_windows_target(&self) -> bool {
        if let Some(ref target) = self.config.build.target {
            target.contains("windows") || target.contains("mingw")
        } else {
            cfg!(target_os = "windows")
        }
    }
    
    /// 清理构建产物
    /// 
    /// # 复杂度
    /// - 时间: O(1)
    /// - 空间: O(1)
    pub fn clean(&self) -> Result<()> {
        let target_dir = self.config.target_path(&self.project_root);
        
        if target_dir.exists() {
            std::fs::remove_dir_all(&target_dir)
                .with_context(|| format!("清理目标目录失败: {}", target_dir.display()))?;
        }
        
        if self.verbose {
            println!("Cavly: 已清理: {}", target_dir.display());
        }
        
        Ok(())
    }
}

/// 查找 cayc 编译器
/// 
/// 搜索顺序:
/// 1. 系统 PATH 中的 cayc
/// 2. 当前可执行文件所在目录下的 cayc
pub fn find_cayc() -> Result<PathBuf> {
    // 1. 尝试系统 PATH
    if let Ok(output) = Command::new("cayc").arg("--version").output() {
        if output.status.success() {
            return Ok(PathBuf::from("cayc"));
        }
    }
    
    // 2. 尝试当前可执行文件所在目录
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let cayc_exe = if cfg!(target_os = "windows") {
                exe_dir.join("cayc.exe")
            } else {
                exe_dir.join("cayc")
            };
            
            if cayc_exe.exists() {
                return Ok(cayc_exe);
            }
        }
    }
    
    // 3. 尝试当前工作目录下的 target/debug 或 target/release
    if let Ok(cwd) = std::env::current_dir() {
        for profile in &["debug", "release"] {
            let cayc_exe = if cfg!(target_os = "windows") {
                cwd.join("target").join(profile).join("cayc.exe")
            } else {
                cwd.join("target").join(profile).join("cayc")
            };
            
            if cayc_exe.exists() {
                return Ok(cayc_exe);
            }
        }
    }
    
    bail!("找不到 cayc 编译器。请确保 cayc 已安装并在 PATH 中，或与 cavly 在同一目录下")
}

/// 快速构建入口（构建所有默认 bin）
/// 
/// # 复杂度
/// - 时间: O(n + m)
/// - 空间: O(n)
pub fn quick_build(project_root: &Path, verbose: bool) -> Result<Vec<PathBuf>> {
    let config_path = project_root.join("cavly.toml");
    let config = CavlyConfig::from_file(&config_path)?;
    
    let mut builder = Builder::new(project_root.to_path_buf(), config)
        .verbose(verbose);
    
    builder.build_all_bins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cavly::config::{PackageConfig, BuildConfig};
    use tempfile::TempDir;

    fn create_test_config() -> CavlyConfig {
        CavlyConfig {
            package: PackageConfig {
                name: "test".to_string(),
                version: "0.1.0".to_string(),
                main: "main.cay".to_string(),
                src_dir: "src".to_string(),
                target_dir: "target".to_string(),
                ..Default::default()
            },
            build: BuildConfig {
                opt_level: "0".to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn test_builder_state() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config();
        let builder = Builder::new(temp.path().to_path_buf(), config);
        
        assert_eq!(builder.state(), BuildState::Idle);
    }

    #[test]
    fn test_is_windows_target() {
        let temp = TempDir::new().unwrap();
        let mut config = create_test_config();
        
        // 显式 Windows 目标
        config.build.target = Some("x86_64-w64-mingw32".to_string());
        let builder = Builder::new(temp.path().to_path_buf(), config.clone());
        assert!(builder.is_windows_target());
        
        // Linux 目标
        config.build.target = Some("x86_64-unknown-linux-gnu".to_string());
        let builder = Builder::new(temp.path().to_path_buf(), config);
        assert!(!builder.is_windows_target());
    }

    #[test]
    fn test_builder_verbose() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config();
        let builder = Builder::new(temp.path().to_path_buf(), config)
            .verbose(true);
        
        assert!(builder.verbose);
    }

    #[test]
    fn test_build_cayc_args() {
        let temp = TempDir::new().unwrap();
        let mut config = create_test_config();
        
        // 设置一些构建选项
        config.build.debug = true;
        config.build.static_link = true;
        config.build.opt_level = "3".to_string();
        config.build.libs = vec!["m".to_string()];
        
        let builder = Builder::new(temp.path().to_path_buf(), config);
        
        let source = Path::new("src/main.cay");
        let output = Path::new("target/test.exe");
        
        let args = builder.build_cayc_args(source, output, None).unwrap();
        
        // 验证参数包含预期内容
        assert!(args.contains(&"-O3".to_string()));
        assert!(args.contains(&"-g".to_string()));
        assert!(args.contains(&"--static".to_string()));
        assert!(args.contains(&"-lm".to_string()));
        assert!(args.contains(&"src/main.cay".to_string()));
        assert!(args.contains(&"target/test.exe".to_string()));
    }
}
