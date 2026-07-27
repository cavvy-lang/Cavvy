// Cavly 测试运行器
//
// 负责测试发现、编译、执行和结果收集。
// 支持两种测试模式：
// 1. harness 模式：使用 --test 标志编译，编译器自动发现 @Test 注解方法
// 2. 非 harness 模式：作为普通程序编译运行，退出码 0 表示通过
//
// 时间复杂度: O(n*m) 测试发现和编译, O(n) 结果收集
// 空间复杂度: O(n) 测试列表和结果

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::builder::find_cayc;
use super::config::{CavlyConfig, TestTarget};
use super::ensure_dir;

/// 单个测试结果
#[derive(Debug, Clone)]
pub struct TestResult {
    /// 测试名称
    pub name: String,
    /// 是否通过
    pub passed: bool,
    /// 执行耗时
    pub duration: Duration,
    /// 失败时的错误信息
    pub error: Option<String>,
    /// 标准输出（用于调试）
    pub stdout: String,
}

/// 测试运行汇总
#[derive(Debug, Clone)]
pub struct TestSummary {
    /// 总测试数
    pub total: usize,
    /// 通过数
    pub passed: usize,
    /// 失败数
    pub failed: usize,
    /// 总耗时
    pub total_duration: Duration,
    /// 每个测试的详细结果
    pub results: Vec<TestResult>,
}

impl TestSummary {
    /// 是否全部通过
    pub fn is_success(&self) -> bool {
        self.failed == 0
    }
}

/// Cavly 测试运行器
pub struct TestRunner {
    /// 项目根目录
    project_root: PathBuf,
    /// 项目配置
    config: CavlyConfig,
    /// 是否 verbose 模式
    verbose: bool,
    /// 测试过滤器（按名称过滤）
    filter: Option<String>,
    /// 是否 fail-fast
    fail_fast: bool,
}

impl TestRunner {
    /// 创建新的测试运行器
    pub fn new(project_root: PathBuf, config: CavlyConfig) -> Self {
        let fail_fast = config.test_config.fail_fast;
        Self {
            project_root,
            config,
            verbose: false,
            filter: None,
            fail_fast,
        }
    }

    /// 设置 verbose 模式
    pub fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// 设置测试过滤器（按名称匹配）
    pub fn filter(mut self, filter: Option<String>) -> Self {
        self.filter = filter;
        self
    }

    /// 运行所有测试
    ///
    /// # 流程
    /// 1. 发现测试目标（显式声明 + tests/ 目录自动扫描）
    /// 2. 编译每个测试目标为可执行文件
    /// 3. 逐个运行测试可执行文件
    /// 4. 收集结果并输出汇总
    ///
    /// # 返回
    /// 测试汇总结果
    pub fn run(&self) -> Result<TestSummary> {
        let start = Instant::now();

        // 1. 发现测试
        let tests = self.discover_tests();
        if tests.is_empty() {
            println!(
                "没有发现测试目标。在 cavly.toml 中添加 [[test]] 或在 tests/ 目录下放置 .cay 文件。"
            );
            return Ok(TestSummary {
                total: 0,
                passed: 0,
                failed: 0,
                total_duration: start.elapsed(),
                results: Vec::new(),
            });
        }

        // 2. 应用过滤器
        let tests: Vec<TestTarget> = if let Some(ref filter) = self.filter {
            let filtered: Vec<_> = tests
                .into_iter()
                .filter(|t| t.name.contains(filter.as_str()))
                .collect();

            if filtered.is_empty() {
                println!("没有测试匹配过滤器: '{}'", filter);
                return Ok(TestSummary {
                    total: 0,
                    passed: 0,
                    failed: 0,
                    total_duration: start.elapsed(),
                    results: Vec::new(),
                });
            }

            if self.verbose {
                println!("Cavly: 过滤器 '{}' 匹配 {} 个测试", filter, filtered.len());
            }
            filtered
        } else {
            tests
        };

        let test_count = tests.len();
        println!(
            "\nrunning {} test{}",
            test_count,
            if test_count > 1 { "s" } else { "" }
        );

        // 3. 准备构建目录和查找编译器
        let target_dir = self.config.target_path(&self.project_root);
        let test_build_dir = target_dir.join("tests");
        ensure_dir(&test_build_dir)?;

        let cayc_path = find_cayc()?;

        // 4. 编译并运行测试
        let mut results = Vec::new();
        let mut passed = 0usize;
        let mut failed = 0usize;

        for test in &tests {
            // 编译测试
            let test_exe = match self.compile_test(&cayc_path, &test_build_dir, test) {
                Ok(exe) => exe,
                Err(e) => {
                    // 编译失败也算测试失败
                    let result = TestResult {
                        name: test.name.clone(),
                        passed: false,
                        duration: Duration::ZERO,
                        error: Some(format!("编译失败: {:#}", e)),
                        stdout: String::new(),
                    };
                    results.push(result);
                    failed += 1;

                    if self.fail_fast {
                        break;
                    }
                    continue;
                }
            };

            // 运行测试
            let result = self.run_test(&test_exe, test);

            if result.passed {
                passed += 1;
            } else {
                failed += 1;
            }

            results.push(result);

            // fail-fast
            if self.fail_fast && failed > 0 {
                if self.verbose {
                    println!("Cavly: fail-fast 模式，停止后续测试");
                }
                break;
            }
        }

        let summary = TestSummary {
            total: test_count,
            passed,
            failed,
            total_duration: start.elapsed(),
            results,
        };

        // 输出汇总
        self.print_summary(&summary);

        Ok(summary)
    }

    /// 发现所有测试目标
    fn discover_tests(&self) -> Vec<TestTarget> {
        self.config.discover_tests(&self.project_root)
    }

    /// 编译单个测试目标
    ///
    /// 使用 cayc 编译器编译测试文件。
    /// harness 模式使用 --test 标志。
    fn compile_test(
        &self,
        cayc_path: &Path,
        test_build_dir: &Path,
        test: &TestTarget,
    ) -> Result<PathBuf> {
        let test_source = self.project_root.join(&test.path);
        if !test_source.exists() {
            bail!("测试文件不存在: {}", test_source.display());
        }

        let exe_name = if cfg!(target_os = "windows") {
            format!("{}.exe", test.name)
        } else {
            test.name.clone()
        };
        let test_exe = test_build_dir.join(&exe_name);

        let mut args = vec![format!("-O{}", self.config.build.opt_level)];

        // harness 模式：添加 --test 标志
        if test.harness {
            args.push("--test".to_string());
        }

        // 调试信息
        if self.config.build.debug {
            args.push("-g".to_string());
        }

        args.push(test_source.to_string_lossy().to_string());
        args.push(test_exe.to_string_lossy().to_string());

        if self.verbose {
            println!(
                "Cavly:   编译测试 {}: {} {}",
                test.name,
                cayc_path.display(),
                args.join(" ")
            );
        }

        let output = Command::new(cayc_path)
            .args(&args)
            .current_dir(&self.project_root)
            .output()
            .with_context(|| format!("编译测试失败: {}", test.name))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            bail!("测试编译错误:\nstdout:\n{}\nstderr:\n{}", stdout, stderr);
        }

        if !test_exe.exists() {
            bail!("编译未生成测试可执行文件: {}", test_exe.display());
        }

        Ok(test_exe)
    }

    /// 运行单个测试可执行文件
    fn run_test(&self, test_exe: &Path, test: &TestTarget) -> TestResult {
        let start = Instant::now();

        let timeout = if self.config.test_config.timeout_secs > 0 {
            Some(Duration::from_secs(self.config.test_config.timeout_secs))
        } else {
            None
        };

        // 构建命令
        let mut cmd = Command::new(test_exe);
        cmd.current_dir(&self.project_root);

        // 设置超时（使用子进程）
        let result = if let Some(timeout_dur) = timeout {
            // 简单超时实现：使用 spawn + wait_with_timeout
            // 注意：Windows 上 Command::output 不支持超时，用 spawn 替代
            match cmd.spawn() {
                Ok(mut child) => {
                    // 轮询等待
                    let poll_interval = Duration::from_millis(100);
                    let deadline = Instant::now() + timeout_dur;
                    let mut timed_out = false;

                    loop {
                        match child.try_wait() {
                            Ok(Some(status)) => {
                                // 进程已退出，收集输出；收集失败时明确报告错误
                                match child.wait_with_output() {
                                    Ok(output) => {
                                        let stdout =
                                            String::from_utf8_lossy(&output.stdout).to_string();

                                        return TestResult {
                                            name: test.name.clone(),
                                            passed: status.success(),
                                            duration: start.elapsed(),
                                            error: if status.success() {
                                                None
                                            } else {
                                                Some(format!("退出码: {:?}", status.code()))
                                            },
                                            stdout,
                                        };
                                    }
                                    Err(e) => {
                                        return TestResult {
                                            name: test.name.clone(),
                                            passed: false,
                                            duration: start.elapsed(),
                                            error: Some(format!(
                                                "收集测试进程输出失败: {}",
                                                e
                                            )),
                                            stdout: String::new(),
                                        };
                                    }
                                }
                            }
                            Ok(None) => {
                                if Instant::now() >= deadline {
                                    timed_out = true;
                                    let _ = child.kill();
                                    let _ = child.wait();
                                    break;
                                }
                                std::thread::sleep(poll_interval);
                            }
                            Err(e) => {
                                return TestResult {
                                    name: test.name.clone(),
                                    passed: false,
                                    duration: start.elapsed(),
                                    error: Some(format!("进程错误: {}", e)),
                                    stdout: String::new(),
                                };
                            }
                        }
                    }

                    if timed_out {
                        return TestResult {
                            name: test.name.clone(),
                            passed: false,
                            duration: start.elapsed(),
                            error: Some(format!("测试超时 ({}s)", timeout_dur.as_secs())),
                            stdout: String::new(),
                        };
                    }

                    unreachable!()
                }
                Err(e) => {
                    return TestResult {
                        name: test.name.clone(),
                        passed: false,
                        duration: start.elapsed(),
                        error: Some(format!("无法启动测试进程: {}", e)),
                        stdout: String::new(),
                    };
                }
            }
        } else {
            // 无超时限制，直接运行
            match cmd.output() {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    TestResult {
                        name: test.name.clone(),
                        passed: output.status.success(),
                        duration: start.elapsed(),
                        error: if output.status.success() {
                            None
                        } else {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            Some(format!(
                                "退出码: {:?}\nstderr:\n{}",
                                output.status.code(),
                                stderr
                            ))
                        },
                        stdout,
                    }
                }
                Err(e) => TestResult {
                    name: test.name.clone(),
                    passed: false,
                    duration: start.elapsed(),
                    error: Some(format!("运行测试失败: {}", e)),
                    stdout: String::new(),
                },
            }
        };

        result
    }

    /// 打印测试结果汇总
    fn print_summary(&self, summary: &TestSummary) {
        // 打印每个测试的结果
        for result in &summary.results {
            if result.passed {
                println!("test {} ... ok ({:.2?})", result.name, result.duration);
            } else {
                println!("test {} ... FAILED", result.name);
                if let Some(ref error) = result.error {
                    // 缩进显示错误信息
                    for line in error.lines() {
                        println!("  {}", line);
                    }
                }
                if self.config.test_config.show_output && !result.stdout.is_empty() {
                    println!("  --- stdout ---");
                    for line in result.stdout.lines() {
                        println!("  {}", line);
                    }
                }
            }
        }

        // 打印汇总
        println!();
        if summary.is_success() {
            println!(
                "test result: ok. {} passed; 0 failed; finished in {:.2?}",
                summary.passed, summary.total_duration
            );
        } else {
            println!(
                "test result: FAILED. {} passed; {} failed; finished in {:.2?}",
                summary.passed, summary.failed, summary.total_duration
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_test_summary_is_success() {
        let summary = TestSummary {
            total: 3,
            passed: 3,
            failed: 0,
            total_duration: Duration::from_secs(1),
            results: Vec::new(),
        };
        assert!(summary.is_success());
    }

    #[test]
    fn test_test_summary_has_failures() {
        let summary = TestSummary {
            total: 3,
            passed: 2,
            failed: 1,
            total_duration: Duration::from_secs(1),
            results: Vec::new(),
        };
        assert!(!summary.is_success());
    }

    #[test]
    fn test_discover_tests_empty_directory() {
        let temp = TempDir::new().unwrap();
        let config = CavlyConfig::default();
        let runner = TestRunner::new(temp.path().to_path_buf(), config);

        let tests = runner.discover_tests();
        assert!(tests.is_empty());
    }
}
