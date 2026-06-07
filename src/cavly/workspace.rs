// 时间复杂度: O(n*m) 依赖解析, O(n) 配置合并
// 空间复杂度: O(n) 存储解析结果

use anyhow::{Context, Result, bail};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::CONFIG_FILE;
use super::config::{CavlyConfig, Dependency, DetailedDependency, ProjectType};

/// 解析后的依赖信息
#[derive(Debug, Clone)]
pub struct ResolvedDependency {
    /// 依赖名
    pub name: String,
    /// 依赖路径
    pub path: PathBuf,
    /// 依赖配置
    pub config: CavlyConfig,
    /// 是否为本地路径依赖
    pub is_local: bool,
    /// 是否为可选依赖
    pub optional: bool,
}

/// 工作区解析器
///
/// 处理工作区成员和依赖的解析
pub struct WorkspaceResolver {
    /// 项目根目录
    project_root: PathBuf,
    /// 已解析的依赖缓存
    resolved_cache: HashMap<String, ResolvedDependency>,
    /// 解析栈（用于检测循环依赖）
    resolution_stack: Vec<String>,
}

impl WorkspaceResolver {
    /// 创建新的工作区解析器
    ///
    /// # 复杂度
    /// - 时间: O(1)
    /// - 空间: O(1)
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            resolved_cache: HashMap::new(),
            resolution_stack: Vec::new(),
        }
    }

    /// 解析所有依赖（包括传递依赖）
    ///
    /// # 说明
    /// 1. 首先解析 workspace.members 中的本地库
    /// 2. 然后解析 dependencies 中的依赖
    /// 3. 合并所有依赖的配置到主配置
    ///
    /// # 复杂度
    /// - 时间: O(n*m)，n 为依赖数量，m 为每个依赖的配置大小
    /// - 空间: O(n) 存储解析结果
    pub fn resolve_all(&mut self, config: &CavlyConfig) -> Result<Vec<ResolvedDependency>> {
        let mut resolved = Vec::new();

        // 1. 解析工作区成员
        for member in &config.workspace.members {
            let member_path = self.project_root.join(member);
            if let Some(dep) = self.resolve_local_lib(&member_path, false)? {
                resolved.push(dep);
            }
        }

        // 2. 解析依赖
        for (name, dep) in &config.dependencies {
            if let Some(resolved_dep) = self.resolve_dependency(name, dep)? {
                resolved.push(resolved_dep);
            }
        }

        Ok(resolved)
    }

    /// 解析单个依赖
    ///
    /// # 复杂度
    /// - 时间: O(1) 缓存命中，O(n) 缓存未命中
    /// - 空间: O(1)
    fn resolve_dependency(
        &mut self,
        name: &str,
        dep: &Dependency,
    ) -> Result<Option<ResolvedDependency>> {
        // 检查缓存
        if let Some(cached) = self.resolved_cache.get(name) {
            return Ok(Some(cached.clone()));
        }

        // 检查循环依赖
        if self.resolution_stack.contains(&name.to_string()) {
            bail!("检测到循环依赖: {} -> {:?}", name, self.resolution_stack);
        }

        self.resolution_stack.push(name.to_string());

        let result = match dep {
            Dependency::Simple(_version) => {
                // TODO: 从 registry 解析版本依赖
                // 目前仅支持本地路径依赖
                None
            }
            Dependency::Detailed(detailed) => self.resolve_detailed_dependency(name, detailed)?,
        };

        self.resolution_stack.pop();

        // 缓存结果
        if let Some(ref resolved) = result {
            self.resolved_cache
                .insert(name.to_string(), resolved.clone());
        }

        Ok(result)
    }

    /// 解析详细依赖配置
    ///
    /// # 复杂度
    /// - 时间: O(n)，n 为搜索路径数量
    /// - 空间: O(1)
    fn resolve_detailed_dependency(
        &mut self,
        name: &str,
        detailed: &DetailedDependency,
    ) -> Result<Option<ResolvedDependency>> {
        // 优先处理本地路径依赖
        if let Some(ref path) = detailed.path {
            let full_path = self.project_root.join(path);
            return self.resolve_local_lib(&full_path, detailed.optional);
        }

        // 处理 Git 依赖
        if let Some(ref git_url) = detailed.git {
            return self.resolve_git_dependency(name, git_url, detailed);
        }

        // 处理版本依赖（从 registry）
        if let Some(ref version) = detailed.version {
            return self.resolve_registry_dependency(name, version, detailed.optional);
        }

        Ok(None)
    }

    /// 解析 Git 依赖
    ///
    /// 从 Git 仓库克隆/更新依赖并解析
    fn resolve_git_dependency(
        &mut self,
        name: &str,
        git_url: &str,
        detailed: &DetailedDependency,
    ) -> Result<Option<ResolvedDependency>> {
        // 确定目标目录
        let cache_dir = self.project_root.join(".cavvy").join("cache").join("git");
        std::fs::create_dir_all(&cache_dir)
            .with_context(|| format!("创建 Git 缓存目录失败: {}", cache_dir.display()))?;

        // 生成唯一的目录名
        let dir_name = format!("{}_{}", name, git_url.replace("://", "_").replace("/", "_"));
        let repo_dir = cache_dir.join(&dir_name);

        // 如果目录不存在，克隆仓库
        if !repo_dir.exists() {
            // 尝试使用 git 命令克隆
            let branch = detailed.branch.as_deref().unwrap_or("main");
            let status = std::process::Command::new("git")
                .args(&[
                    "clone",
                    "--depth",
                    "1",
                    "--branch",
                    branch,
                    git_url,
                    &repo_dir.to_string_lossy(),
                ])
                .status();

            match status {
                Ok(s) if s.success() => {
                    // 克隆成功
                }
                Ok(s) => {
                    if detailed.optional {
                        return Ok(None);
                    }
                    bail!("Git 克隆失败（退出码: {}）: {}", s, git_url);
                }
                Err(e) => {
                    if detailed.optional {
                        return Ok(None);
                    }
                    bail!("无法执行 git 命令: {}", e);
                }
            }
        }

        // 尝试解析克隆的仓库
        self.resolve_local_lib(&repo_dir, detailed.optional)
    }

    /// 解析注册表依赖
    ///
    /// 从本地或远程注册表查找并解析依赖
    fn resolve_registry_dependency(
        &mut self,
        name: &str,
        version: &str,
        optional: bool,
    ) -> Result<Option<ResolvedDependency>> {
        // 搜索本地注册表路径
        let registry_paths = vec![
            self.project_root.join(".cavvy").join("registry"),
            std::env::var("HOME")
                .map(|h| std::path::PathBuf::from(h).join(".cavvy").join("registry"))
                .unwrap_or_default(),
        ];

        for registry_path in &registry_paths {
            if !registry_path.exists() {
                continue;
            }

            // 搜索注册表中的包
            let package_dir = registry_path.join(name);
            if !package_dir.exists() {
                continue;
            }

            // 查找匹配版本的目录
            if let Ok(entries) = std::fs::read_dir(&package_dir) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if entry_path.is_dir() {
                        let version_dir =
                            entry_path.file_name().unwrap_or_default().to_string_lossy();
                        if version == "latest" || version_dir.starts_with(version) {
                            return self.resolve_local_lib(&entry_path, optional);
                        }
                    }
                }
            }
        }

        if optional {
            return Ok(None);
        }

        bail!("在注册表中找不到包 {} 版本 {}", name, version);
    }

    /// 解析本地库项目
    ///
    /// # 复杂度
    /// - 时间: O(1) 文件系统操作 + O(n) 配置解析
    /// - 空间: O(1)
    fn resolve_local_lib(
        &mut self,
        path: &Path,
        optional: bool,
    ) -> Result<Option<ResolvedDependency>> {
        let config_path = path.join(CONFIG_FILE);

        if !config_path.exists() {
            if optional {
                return Ok(None);
            }
            bail!("找不到库项目配置文件: {}", config_path.display());
        }

        let config = CavlyConfig::from_file(&config_path)
            .with_context(|| format!("解析库配置失败: {}", config_path.display()))?;

        // 验证是否为库项目
        if config.package.project_type != ProjectType::Lib {
            if optional {
                return Ok(None);
            }
            bail!("项目 {} 不是库项目", config.package.name);
        }

        let dep = ResolvedDependency {
            name: config.package.name.clone(),
            path: path.to_path_buf(),
            config,
            is_local: true,
            optional,
        };

        Ok(Some(dep))
    }

    /// 搜索库文件
    ///
    /// # 说明
    /// 在以下位置搜索库文件：
    /// 1. 依赖的 target/lib 目录
    /// 2. 配置的 lib_paths
    /// 3. 系统库路径
    ///
    /// # 复杂度
    /// - 时间: O(n*m)，n 为搜索路径数，m 为每个目录的文件数
    /// - 空间: O(1)
    pub fn find_library(
        &self,
        lib_name: &str,
        config: &CavlyConfig,
        dependencies: &[ResolvedDependency],
    ) -> Result<Option<PathBuf>> {
        let lib_filename = Self::format_lib_filename(lib_name);

        // 1. 在依赖的 target/lib 中搜索
        for dep in dependencies {
            let lib_path = dep.config.lib_install_path(&dep.path).join(&lib_filename);
            if lib_path.exists() {
                return Ok(Some(lib_path));
            }
        }

        // 2. 在配置的 lib_paths 中搜索
        for path_str in &config.workspace.lib_paths {
            let path = self.project_root.join(path_str);
            let lib_path = path.join(&lib_filename);
            if lib_path.exists() {
                return Ok(Some(lib_path));
            }
        }

        for path_str in &config.build.lib_paths {
            let path = self.project_root.join(path_str);
            let lib_path = path.join(&lib_filename);
            if lib_path.exists() {
                return Ok(Some(lib_path));
            }
        }

        // 3. 在依赖的库路径中搜索
        for dep in dependencies {
            for path_str in &dep.config.workspace.lib_paths {
                let path = dep.path.join(path_str);
                let lib_path = path.join(&lib_filename);
                if lib_path.exists() {
                    return Ok(Some(lib_path));
                }
            }
        }

        Ok(None)
    }

    /// 格式化库文件名
    ///
    /// # 复杂度
    /// - 时间: O(1)
    /// - 空间: O(1)
    fn format_lib_filename(name: &str) -> String {
        let name = if name.starts_with("lib") {
            name.to_string()
        } else {
            format!("lib{}", name)
        };

        if cfg!(target_os = "windows") {
            format!("{}.lib", name)
        } else {
            format!("{}.a", name)
        }
    }

    /// 获取所有库搜索路径
    ///
    /// # 复杂度
    /// - 时间: O(n)，n 为依赖数量
    /// - 空间: O(n)
    pub fn collect_lib_paths(
        &self,
        config: &CavlyConfig,
        dependencies: &[ResolvedDependency],
    ) -> Vec<PathBuf> {
        let mut paths = HashSet::new();

        // 添加项目配置的 lib_paths
        for path_str in &config.workspace.lib_paths {
            paths.insert(self.project_root.join(path_str));
        }
        for path_str in &config.build.lib_paths {
            paths.insert(self.project_root.join(path_str));
        }

        // 添加依赖的库安装路径（only_include 依赖没有 .lib 产出，跳过其安装路径）
        for dep in dependencies {
            if !dep.config.lib.only_include {
                paths.insert(dep.config.lib_install_path(&dep.path));
            }

            // 添加依赖的 lib_paths（包括 only_include 依赖，因为它们可能提供搜索路径）
            for path_str in &dep.config.workspace.lib_paths {
                paths.insert(dep.path.join(path_str));
            }
            for path_str in &dep.config.build.lib_paths {
                paths.insert(dep.path.join(path_str));
            }
        }
        paths.into_iter().collect()
    }

    /// 合并所有依赖的配置
    ///
    /// # 说明
    /// 按照依赖顺序合并配置，先合并的优先级低
    ///
    /// # 复杂度
    /// - 时间: O(n*m)，n 为依赖数量，m 为配置大小
    /// - 空间: O(1)
    pub fn merge_dependencies_config(
        &self,
        config: &mut CavlyConfig,
        dependencies: &[ResolvedDependency],
    ) {
        for dep in dependencies {
            config.merge(&dep.config);
        }
    }
}

/// 构建依赖图（反向图）
///
/// 返回的图中，graph[dep] 表示哪些节点依赖于 dep
/// 即如果 b 依赖于 a，那么 graph["a"] 包含 "b"
///
/// # 复杂度
/// - 时间: O(n + e)，n 为节点数，e 为边数
/// - 空间: O(n + e)
pub fn build_dependency_graph(dependencies: &[ResolvedDependency]) -> HashMap<String, Vec<String>> {
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();

    // 初始化所有节点
    for dep in dependencies {
        graph.entry(dep.name.clone()).or_default();
    }

    // 构建反向图
    for dep in dependencies {
        for dep_name in dep.config.dependencies.keys() {
            // dep 依赖于 dep_name
            // 所以在反向图中，dep_name -> dep
            graph
                .entry(dep_name.clone())
                .or_default()
                .push(dep.name.clone());
        }
    }

    graph
}

/// 拓扑排序依赖
///
/// # 说明
/// 返回按依赖顺序排序的依赖列表，被依赖的在前
///
/// # 复杂度
/// - 时间: O(n + e)，n 为节点数，e 为边数
/// - 空间: O(n)
pub fn topological_sort(dependencies: &[ResolvedDependency]) -> Result<Vec<ResolvedDependency>> {
    let graph = build_dependency_graph(dependencies);
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    let mut name_to_dep: HashMap<String, ResolvedDependency> = HashMap::new();

    // 初始化所有节点的入度为0
    for dep in dependencies {
        in_degree.entry(dep.name.clone()).or_insert(0);
        name_to_dep.insert(dep.name.clone(), dep.clone());
    }

    // 计算入度：在反向图中，graph[dep] 表示依赖于 dep 的节点列表
    // 所以 dep 的入度 = 依赖于 dep 的节点数量
    for dep in dependencies {
        if let Some(deps) = graph.get(&dep.name) {
            // deps 是依赖于 dep 的节点列表
            // 这些节点的入度应该增加
            for dependent in deps {
                *in_degree.entry(dependent.clone()).or_insert(0) += 1;
            }
        }
    }

    // Kahn 算法：从入度为0的节点开始（即没有其他节点依赖它的节点，也就是最基础的依赖）
    let mut queue: Vec<String> = in_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(name, _)| name.clone())
        .collect();

    let mut result = Vec::new();

    while let Some(name) = queue.pop() {
        if let Some(dep) = name_to_dep.get(&name) {
            result.push(dep.clone());
        }

        // 当前节点已处理，减少依赖于它的节点的入度
        if let Some(deps) = graph.get(&name) {
            for dependent in deps {
                if let Some(deg) = in_degree.get_mut(dependent) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push(dependent.clone());
                    }
                }
            }
        }
    }

    // 检查是否有环
    if result.len() != dependencies.len() {
        bail!("依赖图中存在环");
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_format_lib_filename() {
        assert!(WorkspaceResolver::format_lib_filename("test").contains("libtest"));
        assert!(WorkspaceResolver::format_lib_filename("libtest").contains("libtest"));
    }

    #[test]
    fn test_resolve_local_lib_not_found() {
        let temp = TempDir::new().unwrap();
        let mut resolver = WorkspaceResolver::new(temp.path().to_path_buf());

        let result = resolver.resolve_local_lib(Path::new("/nonexistent"), true);
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_topological_sort() {
        // 创建测试依赖
        let dep_a = ResolvedDependency {
            name: "a".to_string(),
            path: PathBuf::from("/a"),
            config: CavlyConfig::default(),
            is_local: true,
            optional: false,
        };

        let mut config_b = CavlyConfig::default();
        config_b
            .dependencies
            .insert("a".to_string(), Dependency::Simple("1.0".to_string()));
        let dep_b = ResolvedDependency {
            name: "b".to_string(),
            path: PathBuf::from("/b"),
            config: config_b,
            is_local: true,
            optional: false,
        };

        let deps = vec![dep_b.clone(), dep_a.clone()];
        let sorted = topological_sort(&deps).unwrap();

        // a 应该在 b 前面
        let a_index = sorted.iter().position(|d| d.name == "a").unwrap();
        let b_index = sorted.iter().position(|d| d.name == "b").unwrap();
        assert!(a_index < b_index);
    }
}
