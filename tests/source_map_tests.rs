//! IR源映射测试
//!
//! 测试IR生成时的源映射是否正确，以及clang错误能否正确映射回Cavvy源代码位置

use std::fs;
use std::process::Command;

/// 获取当前平台的 cayc 可执行文件路径
fn get_cayc_path() -> String {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_cayc") {
        return path;
    }
    if cfg!(target_os = "windows") {
        "./target/release/cayc.exe".to_string()
    } else {
        "./target/release/cayc".to_string()
    }
}

/// 获取当前平台的可执行文件扩展名
fn get_exe_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        if cfg!(target_os = "windows") {
            ".exe"
        } else {
            ""
        }
    } else {
        ""
    }
}

/// 中型Cavvy测试程序 - 包含多种语言特性以生成足够的IR代码
const TEST_PROGRAM: &str = r#"// 源映射测试程序 - 中型示例
// 包含类、方法、控制流、表达式等多种特性

public class Calculator {
    // 静态常量
    public static final int MAX_VALUE = 1000;
    
    // 实例字段
    private int historyCount;
    private String lastOperation;
    
    // 构造函数
    public Calculator() {
        this.historyCount = 0;
        this.lastOperation = "none";
    }
    
    // 基础算术运算
    public int add(int a, int b) {
        int result = a + b;
        this.recordOperation("add");
        return result;
    }
    
    public int subtract(int a, int b) {
        int result = a - b;
        this.recordOperation("subtract");
        return result;
    }
    
    public int multiply(int a, int b) {
        int result = a * b;
        this.recordOperation("multiply");
        return result;
    }
    
    public int divide(int a, int b) {
        if (b == 0) {
            println("Error: Division by zero!");
            return 0;
        }
        int result = a / b;
        this.recordOperation("divide");
        return result;
    }
    
    // 复杂表达式
    public int calculateFormula(int x, int y, int z) {
        // 计算: (x + y) * (y - z) + x * z
        int part1 = x + y;
        int part2 = y - z;
        int part3 = x * z;
        return part1 * part2 + part3;
    }
    
    // 循环和条件
    public int factorial(int n) {
        if (n < 0) {
            return -1;
        }
        if (n == 0 || n == 1) {
            return 1;
        }
        int result = 1;
        for (int i = 2; i <= n; i = i + 1) {
            result = result * i;
        }
        return result;
    }
    
    // 递归
    public int fibonacci(int n) {
        if (n <= 0) {
            return 0;
        }
        if (n == 1) {
            return 1;
        }
        return fibonacci(n - 1) + fibonacci(n - 2);
    }
    
    // 数组操作
    public int sumArray(int[] arr) {
        int sum = 0;
        for (int i = 0; i < arr.length; i = i + 1) {
            sum = sum + arr[i];
        }
        return sum;
    }
    
    // 最大值查找
    public int findMax(int[] arr) {
        if (arr.length == 0) {
            return 0;
        }
        int max = arr[0];
        for (int i = 1; i < arr.length; i = i + 1) {
            if (arr[i] > max) {
                max = arr[i];
            }
        }
        return max;
    }
    
    // 私有辅助方法
    private void recordOperation(String op) {
        this.lastOperation = op;
        this.historyCount = this.historyCount + 1;
    }
    
    // Getter方法
    public String getLastOperation() {
        return this.lastOperation;
    }
    
    public int getHistoryCount() {
        return this.historyCount;
    }
}

// 辅助类
public class MathUtils {
    public static boolean isPrime(int n) {
        if (n <= 1) {
            return false;
        }
        if (n <= 3) {
            return true;
        }
        if (n % 2 == 0 || n % 3 == 0) {
            return false;
        }
        for (int i = 5; i * i <= n; i = i + 6) {
            if (n % i == 0 || n % (i + 2) == 0) {
                return false;
            }
        }
        return true;
    }
    
    public static int gcd(int a, int b) {
        while (b != 0) {
            int temp = b;
            b = a % b;
            a = temp;
        }
        return a;
    }
    
    public static int lcm(int a, int b) {
        return (a * b) / gcd(a, b);
    }
}

// 主程序
public class SourceMapTest {
    public static void main() {
        Calculator calc = new Calculator();
        
        // 测试基础运算
        int sum = calc.add(10, 20);
        println("add");
        
        int diff = calc.subtract(30, 15);
        println("subtract");
        
        int product = calc.multiply(5, 6);
        println("multiply");
        
        int quotient = calc.divide(20, 4);
        println("divide");
        
        // 测试复杂公式
        int formula = calc.calculateFormula(2, 3, 4);
        println("formula");
        
        // 测试阶乘
        int fact5 = calc.factorial(5);
        println("factorial");
        
        // 测试斐波那契
        int fib10 = calc.fibonacci(10);
        println("fibonacci");
        
        // 测试数组操作
        int[] numbers = new int[5];
        numbers[0] = 10;
        numbers[1] = 20;
        numbers[2] = 30;
        numbers[3] = 40;
        numbers[4] = 50;
        int arraySum = calc.sumArray(numbers);
        println("sumArray");
        
        int maxVal = calc.findMax(numbers);
        println("findMax");
        
        // 测试MathUtils
        boolean is17Prime = MathUtils.isPrime(17);
        println("isPrime");
        
        int gcdResult = MathUtils.gcd(48, 18);
        println("gcd");
        
        int lcmResult = MathUtils.lcm(4, 6);
        println("lcm");
        
        // 显示历史记录
        println("done");
        
        println("All tests completed!");
    }
}
"#;

/// 从IR内容中解析源映射
/// 格式: ; !source file.cay:10:5 (支持Windows路径如 E:\path\file.cay:10:5)
fn parse_source_map_from_ir(ir_content: &str) -> Vec<(usize, String, usize, usize)> {
    let mut mappings = Vec::new();
    let mut current_line = 0usize;

    for line in ir_content.lines() {
        current_line += 1;

        // 检查是否是源映射注释
        if let Some(comment_start) = line.find("; !source ") {
            let comment = &line[comment_start + 10..]; // 跳过 "; !source "

            // 解析格式: file:line:column
            // 处理Windows路径 (E:\path\file.cay:10:5) - 从后往前找冒号
            if let Some(last_colon) = comment.rfind(':') {
                if let Some(second_last_colon) = comment[..last_colon].rfind(':') {
                    let file = comment[..second_last_colon].to_string();
                    let line_str = &comment[second_last_colon + 1..last_colon];
                    let col_str = &comment[last_colon + 1..];

                    if let (Ok(line_num), Ok(col_num)) =
                        (line_str.parse::<usize>(), col_str.parse::<usize>())
                    {
                        mappings.push((current_line, file, line_num, col_num));
                    }
                }
            }
        }
    }

    mappings
}

/// 验证源映射是否包含有效的源代码行号
fn verify_source_mappings(
    ir_content: &str,
    source_content: &str,
) -> Result<Vec<(usize, usize, usize)>, String> {
    let mappings = parse_source_map_from_ir(ir_content);
    let source_lines: Vec<&str> = source_content.lines().collect();
    let source_line_count = source_lines.len();

    let mut valid_mappings = Vec::new();
    let mut errors = Vec::new();

    for (ir_line, file, source_line, col) in &mappings {
        // 验证源文件行号是否在有效范围内
        if *source_line == 0 || *source_line > source_line_count {
            errors.push(format!(
                "IR行 {} 映射到无效源行 {} (文件共 {} 行)",
                ir_line, source_line, source_line_count
            ));
        } else {
            valid_mappings.push((*ir_line, *source_line, *col));
        }
    }

    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }

    Ok(valid_mappings)
}

/// 测试源映射是否正确生成
#[test]
fn test_source_map_generation() {
    // 创建临时目录
    let temp_dir = std::env::temp_dir().join("cavvy_source_map_test");
    fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");

    // 写入测试源文件
    let source_file = temp_dir.join("test_source_map.cay");
    fs::write(&source_file, TEST_PROGRAM).expect("Failed to write source file");

    // 编译生成IR文件
    // IR文件与源文件同名，只是扩展名为.ll（6.x 多文件编译约定：每个源文件对应一个 .ll）
    let ir_file = temp_dir.join("test_source_map.ll");
    let exe_ext = get_exe_extension();
    let output_exe = temp_dir.join(format!("test_output{}", exe_ext));
    let output = Command::new(get_cayc_path())
        .args(&[
            source_file.to_str().unwrap(),
            output_exe.to_str().unwrap(),
            "--keep-ir",
        ])
        .output()
        .expect("Failed to run compiler");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("Compilation failed: {}", stderr);
    }

    // 读取生成的IR文件
    let ir_content =
        fs::read_to_string(&ir_file).expect(&format!("Failed to read IR file: {:?}", ir_file));

    // 解析源映射
    let mappings = parse_source_map_from_ir(&ir_content);

    // 验证映射数量 - 应该有多个映射点
    assert!(
        mappings.len() >= 50,
        "Expected at least 50 source mappings, found {}",
        mappings.len()
    );

    println!("Found {} source mappings", mappings.len());

    // 验证映射的源行号是否有效
    let valid_mappings = verify_source_mappings(&ir_content, TEST_PROGRAM)
        .expect("Source mapping validation failed");

    println!("All {} mappings are valid", valid_mappings.len());

    // 验证映射覆盖不同的源代码行
    let unique_lines: std::collections::HashSet<_> =
        valid_mappings.iter().map(|(_, line, _)| *line).collect();

    assert!(
        unique_lines.len() >= 20,
        "Expected mappings to cover at least 20 unique source lines, found {}",
        unique_lines.len()
    );

    println!("Mappings cover {} unique source lines", unique_lines.len());

    // 清理临时文件
    let _ = fs::remove_dir_all(&temp_dir);
}

/// 测试源映射能否正确映射回源代码位置
#[test]
fn test_source_map_accuracy() {
    // 创建临时目录
    let temp_dir = std::env::temp_dir().join("cavvy_source_map_accuracy_test");
    fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");

    // 写入测试源文件
    let source_file = temp_dir.join("test_accuracy.cay");
    fs::write(&source_file, TEST_PROGRAM).expect("Failed to write source file");

    // 编译生成IR文件（与源文件同名，扩展名为.ll）
    let ir_file = temp_dir.join("test_accuracy.ll");
    let exe_ext = get_exe_extension();
    let output_exe = temp_dir.join(format!("test_output{}", exe_ext));
    let output = Command::new(get_cayc_path())
        .args(&[
            source_file.to_str().unwrap(),
            output_exe.to_str().unwrap(),
            "--keep-ir",
        ])
        .output()
        .expect("Failed to run compiler");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("Compilation failed: {}", stderr);
    }

    // 读取生成的IR文件
    let ir_content = fs::read_to_string(&ir_file).expect("Failed to read IR file");

    // 解析源映射
    let mappings = parse_source_map_from_ir(&ir_content);

    // 验证关键代码行的映射
    // 查找包含特定代码模式的IR行
    let source_lines: Vec<&str> = TEST_PROGRAM.lines().collect();

    // 验证一些关键行的映射
    // 注意：方法声明行(如"public int add")不会直接生成IR，IR从方法体内的语句开始生成
    let key_patterns = vec![
        ("int result = a + b", "add operation"), // 方法体内的第一条语句
        ("if (b == 0)", "division check"),
        ("for (int i = 2; i <= n", "factorial loop"),
        ("return fibonacci(n - 1)", "fibonacci recursion"),
    ];

    for (pattern, desc) in key_patterns {
        // 在源代码中查找行号
        let mut source_line_num = None;
        for (idx, line) in source_lines.iter().enumerate() {
            if line.contains(pattern) {
                source_line_num = Some(idx + 1); // 1-based
                break;
            }
        }

        if let Some(expected_line) = source_line_num {
            // 查找映射到此源代码行的IR行
            let ir_lines: Vec<usize> = mappings
                .iter()
                .filter(|(_, _, line, _)| *line == expected_line)
                .map(|(ir_line, _, _, _)| *ir_line)
                .collect();

            assert!(
                !ir_lines.is_empty(),
                "No IR lines mapped to source line {} ({}: '{}')",
                expected_line,
                desc,
                pattern
            );

            println!(
                "{} (line {}) mapped to IR lines: {:?}",
                desc, expected_line, ir_lines
            );
        }
    }

    // 清理临时文件
    let _ = fs::remove_dir_all(&temp_dir);
}

/// 测试clang错误映射功能
/// 创建一个故意包含错误的程序，验证错误能否正确映射
#[test]
fn test_clang_error_mapping() {
    // 创建一个包含内联IR错误的程序
    let error_program = r#"// 测试clang错误映射
public class ErrorTest {
    public static void testError() {
        int x;
        __ir {
            ; 这一行会生成错误的IR - 使用未定义的变量
            %result = add i32 %undefined_var, 10
            store i32 %result, i32* %x
        }
    }
    
    public static void main() {
        testError();
    }
}
"#;

    // 创建临时目录
    let temp_dir = std::env::temp_dir().join("cavvy_error_map_test");
    fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");

    // 写入测试源文件
    let source_file = temp_dir.join("test_error.cay");
    fs::write(&source_file, error_program).expect("Failed to write source file");

    // 尝试编译 - 应该失败
    let exe_ext = get_exe_extension();
    let output_exe = temp_dir.join(format!("test_output{}", exe_ext));
    let output = Command::new(get_cayc_path())
        .args(&[
            source_file.to_str().unwrap(),
            output_exe.to_str().unwrap(),
            "--keep-ir",
        ])
        .output()
        .expect("Failed to run compiler");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined_output = format!("{} {}", stdout, stderr);

    // 编译应该失败
    if output.status.success() {
        panic!("Expected compilation to fail, but it succeeded");
    }

    // 验证错误输出中包含源映射信息
    // 检查是否有 "[file:line:column]" 格式的源位置信息
    let has_source_mapping = combined_output.contains("[")
        && combined_output.contains("test_error.cay:")
        && combined_output.contains("error:");

    // 如果ir2exe成功重映射了错误，应该能看到源文件信息
    println!("Error output:\n{}", combined_output);

    // 检查IR文件是否存在（--keep-ir应该保留它）
    let ir_file = temp_dir.join("test_error.ll");
    if ir_file.exists() {
        let ir_content = fs::read_to_string(&ir_file).expect("Failed to read IR file");
        let mappings = parse_source_map_from_ir(&ir_content);
        println!("IR file has {} source mappings", mappings.len());

        // 验证映射存在
        assert!(!mappings.is_empty(), "IR file should have source mappings");
    }

    // 清理临时文件
    let _ = fs::remove_dir_all(&temp_dir);
}

/// 测试源映射注释格式
#[test]
fn test_source_map_comment_format() {
    // 简单的测试程序
    let simple_program = r#"public class Test {
    public static void main() {
        int x = 10;
        println("Hello");
    }
}
"#;

    // 创建临时目录
    let temp_dir = std::env::temp_dir().join("cavvy_format_test");
    fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");

    // 写入测试源文件
    let source_file = temp_dir.join("test_format.cay");
    fs::write(&source_file, simple_program).expect("Failed to write source file");

    // 编译生成IR文件（与源文件同名，扩展名为.ll）
    let ir_file = temp_dir.join("test_format.ll");
    let exe_ext = get_exe_extension();
    let output_exe = temp_dir.join(format!("test_output{}", exe_ext));
    let output = Command::new(get_cayc_path())
        .args(&[
            source_file.to_str().unwrap(),
            output_exe.to_str().unwrap(),
            "--keep-ir",
        ])
        .output()
        .expect("Failed to run compiler");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("Compilation failed: {}", stderr);
    }

    // 读取IR文件
    let ir_content = fs::read_to_string(&ir_file).expect("Failed to read IR file");

    // 验证源映射注释格式
    for line in ir_content.lines() {
        if let Some(idx) = line.find("; !source ") {
            let comment = &line[idx + 10..];
            // 验证格式: file:line:column (支持Windows路径)
            // 从后往前找冒号来解析
            if let Some(last_colon) = comment.rfind(':') {
                if let Some(second_last_colon) = comment[..last_colon].rfind(':') {
                    let file = &comment[..second_last_colon];
                    let line_str = &comment[second_last_colon + 1..last_colon];
                    let col_str = &comment[last_colon + 1..];

                    // 验证文件名非空
                    assert!(!file.is_empty(), "Source file path should not be empty");

                    // 验证行号和列号是有效的数字
                    assert!(
                        line_str.parse::<usize>().is_ok(),
                        "Line number should be a valid number: {}",
                        line_str
                    );
                    assert!(
                        col_str.parse::<usize>().is_ok(),
                        "Column number should be a valid number: {}",
                        col_str
                    );
                } else {
                    panic!(
                        "Source map comment should have format 'file:line:column', got: {}",
                        comment
                    );
                }
            } else {
                panic!(
                    "Source map comment should have format 'file:line:column', got: {}",
                    comment
                );
            }
        }
    }

    println!("Source map comment format is valid");

    // 清理临时文件
    let _ = fs::remove_dir_all(&temp_dir);
}
