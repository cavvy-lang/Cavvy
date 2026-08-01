use crate::cavly::config::{
    CavlyConfig, ProjectType, default_config_template, default_lib_config_template,
};
use crate::cavly::{CONFIG_FILE, ensure_dir};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// Cavly 项目管理器
pub struct Project;

impl Project {
    /// 初始化新项目
    ///
    /// # 参数
    /// - `path`: 项目目录路径
    /// - `name`: 项目名称（可选，默认使用目录名）
    /// - `project_type`: 项目类型（bin 或 lib）
    ///
    /// # 复杂度
    /// - 时间: O(1)
    /// - 空间: O(1)
    pub fn init(path: &Path, name: Option<&str>, project_type: ProjectType) -> Result<()> {
        let project_name = name
            .map(String::from)
            .or_else(|| path.file_name().and_then(|n| n.to_str()).map(String::from))
            .unwrap_or_else(|| "my-project".to_string());

        // 验证项目名称
        Self::validate_name(&project_name)?;

        // 创建项目目录
        ensure_dir(path)?;

        // 创建 src 目录
        let src_dir = path.join("src");
        ensure_dir(&src_dir)?;

        // 创建 cavly.toml
        let config_path = path.join(CONFIG_FILE);
        if config_path.exists() {
            bail!("配置文件已存在: {}", config_path.display());
        }

        // 根据项目类型选择模板
        let config_content = match project_type {
            ProjectType::Bin => default_config_template(&project_name),
            ProjectType::Lib => default_lib_config_template(&project_name),
        };

        std::fs::write(&config_path, config_content)
            .with_context(|| format!("写入配置文件失败: {}", config_path.display()))?;

        // 创建默认源文件
        match project_type {
            ProjectType::Bin => {
                let main_path = src_dir.join("main.cay");
                if !main_path.exists() {
                    let main_content = r#"// Cavvy 主程序入口

public class main {
    public static void main() {
        println("Hello, Cavvy!");
    }
}
"#;
                    std::fs::write(&main_path, main_content)
                        .with_context(|| format!("写入主文件失败: {}", main_path.display()))?;
                }
            }
            ProjectType::Lib => {
                let lib_path = src_dir.join("lib.cay");
                if !lib_path.exists() {
                    let lib_content = format!(
                        r#"// Cavvy 库项目: {}

// 导出模块示例
public class {} {{
    // 公共函数会被导出到库中
    public static int add(int a, int b) {{
        return a + b;
    }}
    
    public static void greet() {{
        println("Hello from {} library!");
    }}
}}
"#,
                        project_name,
                        Self::to_class_name(&project_name),
                        project_name
                    );
                    std::fs::write(&lib_path, lib_content)
                        .with_context(|| format!("写入库文件失败: {}", lib_path.display()))?;
                }
            }
        };

        // 创建 .gitignore
        let gitignore_path = path.join(".gitignore");
        if !gitignore_path.exists() {
            let gitignore_content = match project_type {
                ProjectType::Bin => {
                    r#"# Cavvy 构建产物
target/
*.exe
*.ll
*.o

# IDE
.vscode/
.idea/
*.swp
*.swo
*~

# 操作系统
.DS_Store
Thumbs.db
"#
                }
                ProjectType::Lib => {
                    r#"# Cavvy 构建产物
target/
*.exe
*.ll
*.o
*.lib
*.a
*.dll
*.so
*.dylib

# IDE
.vscode/
.idea/
*.swp
*.swo
*~

# 操作系统
.DS_Store
Thumbs.db
"#
                }
            };
            std::fs::write(&gitignore_path, gitignore_content)
                .with_context(|| format!("写入 .gitignore 失败: {}", gitignore_path.display()))?;
        }

        // 创建 tests 目录和示例测试文件
        let tests_dir = path.join("tests");
        if !tests_dir.exists() {
            ensure_dir(&tests_dir)?;

            match project_type {
                ProjectType::Bin => {
                    let test_path = tests_dir.join("test_basic.cay");
                    if !test_path.exists() {
                        let test_content = r#"// Cavvy 测试文件示例
// 使用 @Test 注解标记测试方法（需要编译器 --test 模式）

public class BasicTests {
    @Test
    public static void testAddition() {
        int result = 1 + 1;
        // 断言：如果条件为 false，测试失败
        // 在正式支持 assert 前，用 if + println 模拟
        if (result != 2) {
            println("FAILED: testAddition expected 2, got " + result);
            return;
        }
        println("  testAddition passed");
    }
    
    @Test
    public static void testStringConcat() {
        String hello = "Hello, ";
        String world = "Cavvy!";
        String result = hello + world;
        if (result != "Hello, Cavvy!") {
            println("FAILED: testStringConcat");
            return;
        }
        println("  testStringConcat passed");
    }
    
    public static void main() {
        // 测试入口：手动调用 test 方法
        // cavly test 在 --test 模式下会自动调用 @Test 方法
        testAddition();
        testStringConcat();
        println("All tests passed!");
    }
}
"#;
                        std::fs::write(&test_path, test_content).with_context(|| {
                            format!("写入测试文件失败: {}", test_path.display())
                        })?;
                    }
                }
                ProjectType::Lib => {
                    let test_path = tests_dir.join("test_lib.cay");
                    if !test_path.exists() {
                        let test_content = format!(
                            r#"// {} 库测试

public class LibTests {{
    @Test
    public static void testAdd() {{
        // 测试库的 add 函数
        int result = {}::add(1, 2);
        if (result != 3) {{
            println("FAILED: testAdd expected 3, got " + result);
        }} else {{
            println("  testAdd passed");
        }}
    }}
    
    public static void main() {{
        testAdd();
        println("All library tests passed!");
    }}
}}
"#,
                            project_name,
                            Self::to_class_name(&project_name)
                        );
                        std::fs::write(&test_path, test_content).with_context(|| {
                            format!("写入库测试文件失败: {}", test_path.display())
                        })?;
                    }
                }
            }
        }

        // 创建 build.cay 模板（可选）
        let build_script_path = path.join("build.cay");
        if !build_script_path.exists() {
            let build_content = r#"// Cavvy 构建脚本 (build.cay)
// 在编译主项目之前自动编译并运行此脚本。
//
// 环境变量：
//   OUT_DIR       - 构建产物输出目录
//   PROJECT_ROOT  - 项目根目录
//   PROFILE       - 构建配置 (debug/release)
//   OPT_LEVEL     - 优化级别 (0/1/2/3/s/z)
//   TARGET        - 目标平台
//
// 用途示例：
//   - 代码生成
//   - 下载外部依赖
//   - 编译 C/C++ 代码
//   - 生成版本头文件

public class BuildScript {
    public static void main() {
        // TODO: 在此添加构建前置逻辑
        println("Build script executed successfully!");
    }
}
"#;
            std::fs::write(&build_script_path, build_content).with_context(|| {
                format!("写入构建脚本模板失败: {}", build_script_path.display())
            })?;
        }

        let type_str = match project_type {
            ProjectType::Bin => "可执行项目",
            ProjectType::Lib => "库项目",
        };

        let main_file_name = match project_type {
            ProjectType::Bin => "main.cay",
            ProjectType::Lib => "lib.cay",
        };

        println!(
            "已在 {} 创建{} '{}'",
            path.display(),
            type_str,
            project_name
        );
        println!("  配置文件: {}", config_path.display());
        println!("  主文件: {}", src_dir.join(main_file_name).display());
        println!("  测试目录: {}", tests_dir.display());
        if build_script_path.exists() {
            println!("  构建脚本: {}", build_script_path.display());
        }
        println!();
        println!("下一步:");
        match project_type {
            ProjectType::Bin => {
                println!("  cavly build       # 构建项目");
                println!("  cavly run         # 构建并运行");
                println!("  cavly test        # 运行测试");
            }
            ProjectType::Lib => {
                println!("  cavly build       # 构建库");
                println!("  cavly test        # 运行测试");
            }
        }

        Ok(())
    }

    /// 将项目名称转换为类名（首字母大写）
    fn to_class_name(name: &str) -> String {
        let mut result = String::new();
        let mut capitalize = true;

        for c in name.chars() {
            if c == '_' || c == '-' {
                capitalize = true;
            } else if capitalize {
                result.push(c.to_ascii_uppercase());
                capitalize = false;
            } else {
                result.push(c);
            }
        }

        if result.is_empty() {
            result = "Lib".to_string();
        }

        result
    }

    /// 验证项目名称有效性
    ///
    /// 规则:
    /// - 只能包含字母、数字、下划线和连字符
    /// - 不能以数字开头
    /// - 不能为空
    fn validate_name(name: &str) -> Result<()> {
        if name.is_empty() {
            bail!("项目名称不能为空");
        }

        let first_char = name.chars().next().unwrap();
        if first_char.is_ascii_digit() {
            bail!("项目名称不能以数字开头");
        }

        for c in name.chars() {
            if !c.is_ascii_alphanumeric() && c != '_' && c != '-' {
                bail!("项目名称只能包含字母、数字、下划线和连字符");
            }
        }

        Ok(())
    }

    /// 检查目录是否为 Cavly 项目
    pub fn is_project(path: &Path) -> bool {
        path.join(CONFIG_FILE).exists()
    }

    /// 获取项目信息
    pub fn info(path: &Path) -> Result<ProjectInfo> {
        let config_path = path.join(CONFIG_FILE);
        if !config_path.exists() {
            bail!("当前目录不是 Cavly 项目（找不到 cavly.toml）");
        }

        let config = CavlyConfig::from_file(&config_path)?;

        // 检查源文件
        let src_dir = path.join(&config.package.src_dir);
        let main_file = src_dir.join(&config.package.main);
        let source_exists = main_file.exists();

        // 检查目标目录
        let target_dir = path.join(&config.package.target_dir);
        let has_build = target_dir.exists();

        Ok(ProjectInfo {
            name: config.package.name.clone(),
            version: config.package.version.clone(),
            description: config.package.description.clone(),
            authors: config.package.authors.clone(),
            license: config.package.license.clone(),
            main_file,
            source_exists,
            target_dir,
            has_build,
            config,
        })
    }

    /// 添加 FFI 库配置
    pub fn add_ffi_lib(path: &Path, name: &str, lib: &str) -> Result<()> {
        let config_path = path.join(CONFIG_FILE);
        let mut config = CavlyConfig::from_file(&config_path)?;

        use crate::cavly::config::ExternalLibrary;

        let ext_lib = ExternalLibrary {
            name: name.to_string(),
            lib: lib.to_string(),
            static_lib: false,
            deps: Vec::new(),
            path: None,
            platform: std::collections::HashMap::new(),
        };

        config.ffi.libraries.insert(name.to_string(), ext_lib);
        config.to_file(&config_path)?;

        println!("已添加 FFI 库: {} ({})", name, lib);
        Ok(())
    }

    /// 添加系统库
    pub fn add_system_lib(path: &Path, lib: &str) -> Result<()> {
        let config_path = path.join(CONFIG_FILE);
        let mut config = CavlyConfig::from_file(&config_path)?;

        if !config.ffi.system_libs.contains(&lib.to_string()) {
            config.ffi.system_libs.push(lib.to_string());
            config.to_file(&config_path)?;
            println!("已添加系统库: {}", lib);
        } else {
            println!("系统库已存在: {}", lib);
        }

        Ok(())
    }

    /// 添加注册表依赖并从安全源下载包
    ///
    /// # 流程
    /// 1. 在安全源索引中查找包
    /// 2. 下载并验证包
    /// 3. 将包安装到 .cavvy/registry/<name>/<version>/
    /// 4. 更新 cavly.toml 的 [dependencies]
    ///
    /// # 复杂度
    /// - 时间: O(n) 网络 + O(m) 哈希，m 为包大小
    /// - 空间: O(m)
    pub fn add_registry_dependency(path: &Path, name: &str, version: &str) -> Result<()> {
        use crate::cavly::config::Dependency;

        let config_path = path.join(CONFIG_FILE);
        let mut config = CavlyConfig::from_file(&config_path)?;

        // 检查是否已存在同名依赖
        if config.dependencies.contains_key(name) {
            bail!("依赖 '{}' 已存在于 cavly.toml 中", name);
        }

        println!("正在从安全源查找包 '{}'...", name);

        let mut registry_config = crate::cavly::registry::RegistryConfig::default();
        registry_config.root_public_key = config.security.root_public_key.clone();
        let registry = crate::cavly::registry::SecureRegistry::with_config(registry_config)
            .with_context(|| "创建安全注册表客户端失败")?;

        let pkg = registry
            .find_package(name)
            .with_context(|| format!("在官方索引中找不到包: {}", name))?;

        println!(
            "  找到包: {} v{} (指纹: {})",
            pkg.name, pkg.latest_version, pkg.fingerprint
        );
        println!("  仓库: {}", pkg.repository);

        // 确定安装目录
        let registry_dir = path.join(".cavvy").join("registry").join(name);
        let install_dir = registry_dir.join(&pkg.latest_version);
        std::fs::create_dir_all(&install_dir)
            .with_context(|| format!("创建安装目录失败: {}", install_dir.display()))?;

        // 下载并验证包
        println!("  正在下载并验证安全证书...");
        let package_path = registry
            .download_and_verify(&pkg, &install_dir)
            .with_context(|| format!("下载并验证包 '{}' 失败", name))?;

        println!("  包已下载到: {}", package_path.display());

        // 解压 tar.gz 到安装目录（去掉顶层目录如 caysdlib-0.1.0/）
        println!("  正在解压包到安装目录...");
        let tar_status = std::process::Command::new("tar")
            .args(&[
                "-xzf",
                &package_path.to_string_lossy(),
                "-C",
                &install_dir.to_string_lossy(),
                "--strip-components=1",
            ])
            .status()
            .with_context(|| "无法执行 tar 命令，请确保系统支持 tar")?;

        if !tar_status.success() {
            bail!("解压包 '{}' 失败", package_path.display());
        }

        // 验证解压后的目录包含 cavly.toml
        let dep_cay_config = install_dir.join(CONFIG_FILE);
        if !dep_cay_config.exists() {
            std::fs::remove_dir_all(&install_dir).ok();
            bail!("下载的包 '{}' 缺少 cavly.toml，不是有效的 Cavvy 项目", name);
        }

        println!("  包已安装到: {}", install_dir.display());

        // 添加到依赖配置（同时记录版本和本地 path，构建时可直接使用）
        let actual_version = if version == "latest" {
            pkg.latest_version.clone()
        } else {
            version.to_string()
        };

        let rel_path = PathBuf::from(".cavvy")
            .join("registry")
            .join(name)
            .join(&actual_version);

        let dep = Dependency::Detailed(crate::cavly::config::DetailedDependency {
            version: Some(actual_version),
            path: Some(rel_path),
            ..Default::default()
        });

        config.add_dependency(name, dep);
        config
            .to_file(&config_path)
            .with_context(|| "写入 cavly.toml 失败")?;

        println!("已将 '{}' 添加到 [dependencies] 并安装到本地注册表", name);
        Ok(())
    }

    /// 添加 Git 依赖并立即克隆仓库
    ///
    /// # 流程
    /// 1. 检查 cavly.toml 中是否已存在同名依赖
    /// 2. 克隆 Git 仓库到 .cavvy/git/<name>/
    /// 3. 如果指定分支/标签，执行 checkout
    /// 4. 验证克隆的仓库为有效 Cavvy 库项目
    /// 5. 更新 cavly.toml，同时记录 git URL 和本地 path
    /// 6. 记录审计日志
    ///
    /// # 复杂度
    /// - 时间: O(n) 网络 + O(m) 磁盘，m 为仓库大小
    /// - 空间: O(m)
    pub fn add_git_dependency(
        path: &Path,
        name: &str,
        git_url: &str,
        branch: Option<&str>,
        tag: Option<&str>,
    ) -> Result<()> {
        use crate::cavly::audit::{AuditLogEntry, AuditLogger, SecurityEventType};
        use crate::cavly::config::{Dependency, DetailedDependency};

        let config_path = path.join(CONFIG_FILE);
        let mut config = CavlyConfig::from_file(&config_path)?;

        if config.dependencies.contains_key(name) {
            bail!("依赖 '{}' 已存在于 cavly.toml 中", name);
        }

        // 确定克隆目录
        let git_dir = path.join(".cavvy").join("git").join(name);
        if git_dir.exists() {
            std::fs::remove_dir_all(&git_dir)
                .with_context(|| format!("删除已存在的 Git 目录失败: {}", git_dir.display()))?;
        }
        std::fs::create_dir_all(&git_dir.parent().unwrap())
            .with_context(|| "创建 .cavvy/git 目录失败")?;

        println!("正在克隆 Git 仓库 '{}'...", git_url);

        // 执行 git clone
        let mut cmd = std::process::Command::new("git");
        cmd.arg("clone");

        // 如果指定了分支，使用浅克隆加速
        if let Some(b) = branch {
            cmd.args(&["--branch", b, "--single-branch"]);
        }

        cmd.args(&["--depth", "1"]);
        cmd.arg(git_url);
        cmd.arg(&git_dir);

        let status = cmd
            .status()
            .with_context(|| "无法执行 git 命令，请确保 git 已安装并加入 PATH")?;

        if !status.success() {
            bail!("Git 克隆失败 (退出码: {:?}): {}", status.code(), git_url);
        }

        // 如果指定了标签，需要拉取完整历史并 checkout 标签
        // （浅克隆默认不包含标签指向的 commit，除非标签在分支上）
        if let Some(t) = tag {
            println!("  正在检出标签 '{}'...", t);
            let fetch_status = std::process::Command::new("git")
                .args(&["-C", &git_dir.to_string_lossy(), "fetch", "--tags"])
                .status()
                .with_context(|| "执行 git fetch --tags 失败")?;

            if !fetch_status.success() {
                bail!("获取标签失败: {}", t);
            }

            let checkout_status = std::process::Command::new("git")
                .args(&["-C", &git_dir.to_string_lossy(), "checkout", t])
                .status()
                .with_context(|| format!("检出标签 '{}' 失败", t))?;

            if !checkout_status.success() {
                bail!("检出标签 '{}' 失败", t);
            }
        }

        // 验证克隆的仓库是否为有效 Cavvy 库项目
        let dep_config_path = git_dir.join(CONFIG_FILE);
        if !dep_config_path.exists() {
            std::fs::remove_dir_all(&git_dir).ok();
            bail!(
                "克隆的仓库 '{}' 缺少 cavly.toml，不是有效的 Cavvy 项目",
                name
            );
        }

        let dep_config = CavlyConfig::from_file(&dep_config_path)
            .with_context(|| format!("解析依赖 '{}' 的配置文件失败", name))?;

        if dep_config.package.project_type != ProjectType::Lib {
            std::fs::remove_dir_all(&git_dir).ok();
            bail!(
                "Git 依赖 '{}' 不是库项目 (project_type = {:?})",
                name,
                dep_config.package.project_type
            );
        }

        // 计算相对路径写入 cavly.toml（使项目可移植）
        let rel_path = PathBuf::from(".cavvy").join("git").join(name);

        let dep = Dependency::Detailed(DetailedDependency {
            git: Some(git_url.to_string()),
            branch: branch.map(String::from),
            tag: tag.map(String::from),
            path: Some(rel_path),
            ..Default::default()
        });

        config.add_dependency(name, dep);
        config
            .to_file(&config_path)
            .with_context(|| "写入 cavly.toml 失败")?;

        // 审计日志
        if let Ok(logger) = AuditLogger::new() {
            logger.log_silent(
                &AuditLogEntry::new(SecurityEventType::SecureSourceInstall, "add_git_dependency")
                    .with_package("git", name, &dep_config.package.version)
                    .with_details(&format!("从 {} 克隆 Git 依赖", git_url)),
            );
        }

        println!("已添加 Git 依赖: {} ({})", name, git_url);
        println!("  本地路径: {}", git_dir.display());
        println!("  库版本: {}", dep_config.package.version);
        if let Some(b) = branch {
            println!("  分支: {}", b);
        }
        if let Some(t) = tag {
            println!("  标签: {}", t);
        }

        Ok(())
    }

    /// 添加本地路径依赖
    ///
    /// # 复杂度
    /// - 时间: O(1)
    /// - 空间: O(1)
    pub fn add_path_dependency(path: &Path, name: &str, dep_path: &str) -> Result<()> {
        use crate::cavly::config::{Dependency, DetailedDependency};

        let config_path = path.join(CONFIG_FILE);
        let mut config = CavlyConfig::from_file(&config_path)?;

        if config.dependencies.contains_key(name) {
            bail!("依赖 '{}' 已存在于 cavly.toml 中", name);
        }

        let dep = Dependency::Detailed(DetailedDependency {
            path: Some(std::path::PathBuf::from(dep_path)),
            ..Default::default()
        });

        config.add_dependency(name, dep);
        config.to_file(&config_path)?;

        println!("已添加本地路径依赖: {} (路径: {})", name, dep_path);
        Ok(())
    }

    /// 安装所有缺失的依赖
    ///
    /// 遍历 cavly.toml 中的 [dependencies]，检查每个依赖的本地 path 是否存在。
    /// 若缺失，根据依赖类型自动下载并安装：
    /// - A 类（纯包名，有 version 无 git/source）：从官方安全源下载
    /// - B 类（git URL）：克隆 Git 仓库
    /// - C 类（自定义 source）：从自定义源服务器下载（当前按未验证来源处理）
    ///
    /// # 复杂度
    /// - 时间: O(n * (网络 + 磁盘))，n 为依赖数量
    /// - 空间: O(m)，m 为最大包大小
    pub fn install_dependencies(path: &Path, verbose: bool) -> Result<()> {
        use crate::cavly::audit::{AuditLogEntry, AuditLogger, SecurityEventType};
        use crate::cavly::config::{Dependency, DetailedDependency};

        let config_path = path.join(CONFIG_FILE);
        let config = CavlyConfig::from_file(&config_path)?;

        if config.dependencies.is_empty() {
            if verbose {
                println!("没有需要安装的依赖。");
            }
            return Ok(());
        }

        let mut installed = 0;
        let mut already_exist = 0;
        let mut skipped = 0;

        for (name, dep) in &config.dependencies {
            let detailed = match dep {
                Dependency::Simple(version) => DetailedDependency {
                    version: Some(version.clone()),
                    ..Default::default()
                },
                Dependency::Detailed(d) => d.clone(),
            };

            // 检查本地路径是否已存在
            let is_installed = if let Some(ref p) = detailed.path {
                path.join(p).join(CONFIG_FILE).exists()
            } else {
                false
            };

            if is_installed {
                if verbose {
                    println!("  [已安装] {}", name);
                }
                already_exist += 1;
                continue;
            }

            if detailed.optional {
                if verbose {
                    println!("  [跳过可选] {}", name);
                }
                skipped += 1;
                continue;
            }

            println!("  正在安装 {}...", name);

            // B 类: Git 依赖
            if let Some(ref git_url) = detailed.git {
                Self::install_git_dependency(
                    path,
                    name,
                    git_url,
                    detailed.branch.as_deref(),
                    detailed.tag.as_deref(),
                    verbose,
                )?;
                installed += 1;
                continue;
            }

            // C 类: 自定义源依赖
            if let Some(ref source_url) = detailed.source {
                Self::install_source_dependency(
                    path,
                    name,
                    source_url,
                    detailed.version.as_deref(),
                    verbose,
                )?;
                installed += 1;
                continue;
            }

            // A 类: 注册表依赖（纯包名 + 版本）
            let version = detailed.version.as_deref().unwrap_or("latest");
            Self::install_registry_dependency(path, name, version, verbose)?;
            installed += 1;
        }

        println!();
        println!(
            "依赖安装完成: {} 个新安装, {} 个已存在, {} 个跳过",
            installed, already_exist, skipped
        );
        Ok(())
    }

    /// 内部: 从官方安全源下载并安装单个包（不修改 cavly.toml）
    fn install_registry_dependency(
        path: &Path,
        name: &str,
        version: &str,
        verbose: bool,
    ) -> Result<()> {
        let config_path = path.join(CONFIG_FILE);
        let config = CavlyConfig::from_file(&config_path)?;

        let mut registry_config = crate::cavly::registry::RegistryConfig::default();
        registry_config.root_public_key = config.security.root_public_key.clone();
        let registry = crate::cavly::registry::SecureRegistry::with_config(registry_config)
            .with_context(|| "创建安全注册表客户端失败")?;

        let pkg = registry
            .find_package(name)
            .with_context(|| format!("在官方索引中找不到包: {}", name))?;

        if verbose {
            println!(
                "    找到包: {} v{} (指纹: {})",
                pkg.name, pkg.latest_version, pkg.fingerprint
            );
            println!("    仓库: {}", pkg.repository);
        }

        let registry_dir = path.join(".cavvy").join("registry").join(name);
        let install_dir = registry_dir.join(&pkg.latest_version);
        std::fs::create_dir_all(&install_dir)
            .with_context(|| format!("创建安装目录失败: {}", install_dir.display()))?;

        let package_path = registry
            .download_and_verify(&pkg, &install_dir)
            .with_context(|| format!("下载并验证包 '{}' 失败", name))?;

        // 解压 tar.gz
        let tar_status = std::process::Command::new("tar")
            .args(&[
                "-xzf",
                &package_path.to_string_lossy(),
                "-C",
                &install_dir.to_string_lossy(),
                "--strip-components=1",
            ])
            .status()
            .with_context(|| "无法执行 tar 命令，请确保系统支持 tar")?;

        if !tar_status.success() {
            bail!("解压包 '{}' 失败", package_path.display());
        }

        let dep_cay_config = install_dir.join(CONFIG_FILE);
        if !dep_cay_config.exists() {
            std::fs::remove_dir_all(&install_dir).ok();
            bail!("下载的包 '{}' 缺少 cavly.toml，不是有效的 Cavvy 项目", name);
        }

        if verbose {
            println!("    包已安装到: {}", install_dir.display());
        }
        Ok(())
    }

    /// 内部: 克隆 Git 仓库（不修改 cavly.toml）
    fn install_git_dependency(
        path: &Path,
        name: &str,
        git_url: &str,
        branch: Option<&str>,
        tag: Option<&str>,
        verbose: bool,
    ) -> Result<()> {
        let git_dir = path.join(".cavvy").join("git").join(name);
        if git_dir.exists() {
            std::fs::remove_dir_all(&git_dir)
                .with_context(|| format!("删除已存在的 Git 目录失败: {}", git_dir.display()))?;
        }
        std::fs::create_dir_all(&git_dir.parent().unwrap())
            .with_context(|| "创建 .cavvy/git 目录失败")?;

        if verbose {
            println!("    正在克隆 {} ...", git_url);
        }

        let mut cmd = std::process::Command::new("git");
        cmd.arg("clone");
        if let Some(b) = branch {
            cmd.args(&["--branch", b, "--single-branch"]);
        }
        cmd.args(&["--depth", "1"]);
        cmd.arg(git_url);
        cmd.arg(&git_dir);

        let status = cmd
            .status()
            .with_context(|| "无法执行 git 命令，请确保 git 已安装并加入 PATH")?;

        if !status.success() {
            bail!("Git 克隆失败 (退出码: {:?}): {}", status.code(), git_url);
        }

        if let Some(t) = tag {
            let fetch_status = std::process::Command::new("git")
                .args(&["-C", &git_dir.to_string_lossy(), "fetch", "--tags"])
                .status()?;
            if fetch_status.success() {
                let checkout_status = std::process::Command::new("git")
                    .args(&["-C", &git_dir.to_string_lossy(), "checkout", t])
                    .status()?;
                if !checkout_status.success() {
                    bail!("检出标签 '{}' 失败", t);
                }
            }
        }

        let dep_config_path = git_dir.join(CONFIG_FILE);
        if !dep_config_path.exists() {
            std::fs::remove_dir_all(&git_dir).ok();
            bail!("克隆的仓库 '{}' 缺少 cavly.toml", name);
        }

        if verbose {
            println!("    Git 依赖已克隆到: {}", git_dir.display());
        }
        Ok(())
    }

    /// 内部: 从自定义源下载并安装（ESSO-11420 C 类，当前按未验证来源处理）
    fn install_source_dependency(
        path: &Path,
        name: &str,
        source_url: &str,
        version: Option<&str>,
        verbose: bool,
    ) -> Result<()> {
        use crate::cavly::audit::{AuditLogEntry, AuditLogger, SecurityEventType};
        use crate::cavly::security::{blocking_warning_custom_source, is_interactive};

        // 阻塞警告（ESSO-11420 第 8 节）
        if is_interactive() {
            blocking_warning_custom_source(source_url, name)?;
        } else {
            // 非交互环境：若未关闭阻塞，则失败
            if std::env::var("ESSO_UNVERIFIED_SOURCE_NO_BLOCK").unwrap_or_default() != "1" {
                bail!(
                    "非交互环境中安装自定义源依赖需要显式确认，请设置环境变量 ESSO_UNVERIFIED_SOURCE_NO_BLOCK=1"
                );
            }
            eprintln!(
                "[SECURITY NOTICE] Installing from custom source server: {}. Package: {}. Official secure source verification not applicable. This installation is not covered by Ethernos secure source guarantees.",
                source_url, name
            );
        }

        // 审计日志
        if let Ok(logger) = AuditLogger::new() {
            logger.log_silent(
                &AuditLogEntry::new(
                    SecurityEventType::UnverifiedSourceInstall,
                    "install_source_dependency",
                )
                .with_package(source_url, name, version.unwrap_or("latest"))
                .with_details(&format!("从自定义源 {} 安装包 {}", source_url, name)),
            );
        }

        // 构造下载 URL: {source_url}/{name}/{version}/package.tar.gz
        let ver = version.unwrap_or("latest");
        let download_url = format!(
            "{}/{}/{}/package.tar.gz",
            source_url.trim_end_matches('/'),
            name,
            ver
        );

        let install_dir = path.join(".cavvy").join("registry").join(name).join(ver);
        std::fs::create_dir_all(&install_dir)?;

        let package_path = install_dir.join("package.tar.gz");

        if verbose {
            println!("    正在从自定义源下载: {}", download_url);
        }

        // 使用 http_get 下载
        let data = crate::cavly::registry::http_get(&download_url, 60)
            .with_context(|| format!("从自定义源下载失败: {}", download_url))?;

        std::fs::write(&package_path, &data)
            .with_context(|| format!("保存下载包失败: {}", package_path.display()))?;

        // 解压
        let tar_status = std::process::Command::new("tar")
            .args(&[
                "-xzf",
                &package_path.to_string_lossy(),
                "-C",
                &install_dir.to_string_lossy(),
                "--strip-components=1",
            ])
            .status()
            .with_context(|| "无法执行 tar 命令")?;

        if !tar_status.success() {
            bail!("解压包 '{}' 失败", package_path.display());
        }

        let dep_cay_config = install_dir.join(CONFIG_FILE);
        if !dep_cay_config.exists() {
            std::fs::remove_dir_all(&install_dir).ok();
            bail!("下载的包 '{}' 缺少 cavly.toml，不是有效的 Cavvy 项目", name);
        }

        if verbose {
            println!("    包已从自定义源安装到: {}", install_dir.display());
        }
        Ok(())
    }

    /// 添加自定义源依赖（C 类）
    pub fn add_source_dependency(
        path: &Path,
        name: &str,
        source_url: &str,
        version: Option<&str>,
    ) -> Result<()> {
        use crate::cavly::config::{Dependency, DetailedDependency};

        let config_path = path.join(CONFIG_FILE);
        let mut config = CavlyConfig::from_file(&config_path)?;

        if config.dependencies.contains_key(name) {
            bail!("依赖 '{}' 已存在于 cavly.toml 中", name);
        }

        // 先执行安装（触发阻塞警告和审计日志）
        Self::install_source_dependency(path, name, source_url, version, true)?;

        // 写入配置
        let ver = version.unwrap_or("latest").to_string();
        let rel_path = PathBuf::from(".cavvy")
            .join("registry")
            .join(name)
            .join(&ver);

        let dep = Dependency::Detailed(DetailedDependency {
            version: Some(ver),
            source: Some(source_url.to_string()),
            path: Some(rel_path),
            ..Default::default()
        });

        config.add_dependency(name, dep);
        config
            .to_file(&config_path)
            .with_context(|| "写入 cavly.toml 失败")?;

        println!(
            "已将 '{}' 添加到 [dependencies]（自定义源: {}）",
            name, source_url
        );
        Ok(())
    }
}

/// 项目信息
#[derive(Debug, Clone)]
pub struct ProjectInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    pub license: String,
    pub main_file: PathBuf,
    pub source_exists: bool,
    pub target_dir: PathBuf,
    pub has_build: bool,
    pub config: CavlyConfig,
}

impl ProjectInfo {
    /// 格式化输出项目信息
    pub fn print(&self) {
        println!("项目: {} ({})", self.name, self.version);

        if !self.description.is_empty() {
            println!("描述: {}", self.description);
        }

        if !self.authors.is_empty() {
            println!("作者: {}", self.authors.join(", "));
        }

        if !self.license.is_empty() {
            println!("许可证: {}", self.license);
        }

        println!(
            "主文件: {} {}",
            self.main_file.display(),
            if self.source_exists {
                "[存在]"
            } else {
                "[缺失]"
            }
        );

        println!(
            "目标目录: {} {}",
            self.target_dir.display(),
            if self.has_build {
                "[有构建产物]"
            } else {
                "[空]"
            }
        );

        // FFI 库信息
        if !self.config.ffi.system_libs.is_empty() {
            println!("系统库: {}", self.config.ffi.system_libs.join(", "));
        }

        if !self.config.ffi.libraries.is_empty() {
            println!("第三方库:");
            for (name, lib) in &self.config.ffi.libraries {
                println!("  - {} ({})", name, lib.lib);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_validate_name_valid() {
        assert!(Project::validate_name("my-project").is_ok());
        assert!(Project::validate_name("my_project").is_ok());
        assert!(Project::validate_name("MyProject123").is_ok());
    }

    #[test]
    fn test_validate_name_invalid() {
        assert!(Project::validate_name("").is_err());
        assert!(Project::validate_name("123project").is_err());
        assert!(Project::validate_name("my project").is_err());
        assert!(Project::validate_name("my@project").is_err());
    }

    #[test]
    fn test_project_init() {
        use crate::cavly::config::ProjectType;

        let temp = TempDir::new().unwrap();
        let project_path = temp.path().join("test-project");

        Project::init(&project_path, Some("test-project"), ProjectType::Bin).unwrap();

        assert!(project_path.join("cavly.toml").exists());
        assert!(project_path.join("src").exists());
        assert!(project_path.join("src/main.cay").exists());
        assert!(project_path.join(".gitignore").exists());
    }

    #[test]
    fn test_lib_project_init() {
        use crate::cavly::config::ProjectType;

        let temp = TempDir::new().unwrap();
        let project_path = temp.path().join("test-lib");

        Project::init(&project_path, Some("test-lib"), ProjectType::Lib).unwrap();

        assert!(project_path.join("cavly.toml").exists());
        assert!(project_path.join("src").exists());
        assert!(project_path.join("src/lib.cay").exists());
        assert!(project_path.join(".gitignore").exists());
    }

    #[test]
    fn test_is_project() {
        let temp = TempDir::new().unwrap();

        // 空目录不是项目
        assert!(!Project::is_project(temp.path()));

        // 创建配置文件后才是项目
        std::fs::write(temp.path().join("cavly.toml"), "").unwrap();
        assert!(Project::is_project(temp.path()));
    }

    #[test]
    fn test_project_info() {
        use crate::cavly::config::ProjectType;

        let temp = TempDir::new().unwrap();
        Project::init(temp.path(), Some("test"), ProjectType::Bin).unwrap();

        let info = Project::info(temp.path()).unwrap();
        assert_eq!(info.name, "test");
        assert!(info.source_exists);
    }
}
