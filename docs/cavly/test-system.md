# Cavly 测试系统

Cavly 内置完整的测试框架，支持自动发现、编译、运行测试，并提供两种测试编写模式。

## 快速开始

```bash
# 初始化项目（自动生成测试目录和示例）
cavly init my-project
cd my-project

# 运行测试
cavly test
```

初始化后 `tests/` 目录包含一个示例测试文件：

```cay
// tests/test_basic.cay
public class BasicTests {
    @Test
    public static void testAddition() {
        int result = 1 + 1;
        if (result != 2) {
            println("FAILED: testAddition expected 2, got " + result);
        }
        println("  testAddition passed");
    }
}
```

## 两种测试模式

### 1. @Test 注解模式（推荐）

使用 `@Test` 注解标记测试方法。Cavly 编译器在 `--test` 模式下自动发现并调用它们。

**规则：**

- 必须是 `public void methodName()` 或 `public static void methodName()`
- 返回类型必须是 `void`
- 不能有参数
- 不能是 `private`

```cay
public class MathTests {
    @Test
    public static void testAddition() {
        int a = 10;
        int b = 20;
        int result = a + b;
        if (result != 30) {
            println("FAILED: testAddition");
        }
    }

    @Test
    public static void testMultiplication() {
        int result = 5 * 6;
        if (result != 30) {
            println("FAILED: testMultiplication");
        }
    }

    @Test
    public static void testDivision() {
        int result = 100 / 5;
        if (result != 20) {
            println("FAILED: testDivision");
        }
    }
}
```

**harness 模式**下，编译器自动生成 `__cavvy_test_main` 入口函数，逐个调用 `@Test` 方法并打印结果。

### 2. 约定模式

`tests/` 下的 `.cay` 文件作为普通程序编译运行。程序 `exit(0)` 表示通过，`exit(非0)` 表示失败。

```cay
// tests/convention_test.cay
public class ConventionTest {
    public static void main() {
        int result = 1 + 1;
        if (result != 2) {
            println("FAILED");
            // exit(1) — 在 Cavvy 中用 return 1 表示
        }
        println("All tests passed!");
    }
}
```

要禁用 harness，在 `cavly.toml` 中设置：

```toml
[[test]]
name = "convention_test"
path = "tests/convention_test.cay"
harness = false
```

## 测试配置

在 `cavly.toml` 中通过 `[test-config]` 控制测试行为：

```toml
[test-config]
threads = 4         # 4 个线程并发运行
timeout_secs = 30   # 单个测试 30 秒超时
fail_fast = true    # 第一个失败立即停止
show_output = true  # 显示所有测试的输出
```

## 过滤测试

```bash
# 只运行名称包含 "math" 的测试
cavly test --filter math

# 只运行名称包含 "string" 的测试
cavly test --filter string
```

## 测试发现规则

`cavly test` 按以下顺序发现测试：

1. 读取 `cavly.toml` 中显式声明的 `[[test]]` 目标
2. 扫描 `tests/` 目录下的所有 `*.cay` 文件
3. 去重：已显式声明的路径不会被重复添加

**示例配置：**

```toml
# 显式声明
[[test]]
name = "unit"
path = "tests/unit.cay"
harness = true

[[test]]
name = "integration"
path = "tests/integration.cay"
harness = false

# tests/ 下的其他 .cay 文件会被自动发现
# 例如 tests/extra.cay 会被添加为名为 "extra" 的测试
```

## 编写有效测试的建议

### 断言模式

在 Cavvy 正式支持 `assert` 关键字前，用条件 + 输出模拟断言：

```cay
public class StringTests {
    @Test
    public static void testLength() {
        String s = "Hello";
        int len = s.length();
        if (len != 5) {
            println("FAILED: testLength expected 5, got " + len);
        }
    }

    @Test
    public static void testConcat() {
        String result = "Hello, " + "World!";
        if (result != "Hello, World!") {
            println("FAILED: testConcat");
        }
    }
}
```

### 组织测试文件

```
tests/
├── unit/
│   ├── math_tests.cay
│   ├── string_tests.cay
│   └── array_tests.cay
├── integration/
│   ├── database_tests.cay
│   └── api_tests.cay
└── regression/
    └── bug_123.cay
```

### 测试最佳实践

- 每个 `@Test` 方法测试一个独立的行为
- 测试方法名使用 `test` 前缀 + 被测试功能名
- 失败时打印期望值和实际值
- 将测试文件放在 `tests/` 目录下，按模块分子目录
- 使用 `cavly test --filter` 在开发时快速迭代
