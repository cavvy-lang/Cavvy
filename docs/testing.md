# 测试与文档示例

## 项目测试

```powershell
cargo build --release
cargo test --release --verbose
```

集成测试会调用 release 目录中的 `cayc`，编译 `examples/` 下的 `.cay` 文件并运行生成程序。

## 文档测试

一键命令：

```powershell
.\scripts\test-docs.ps1
```

跨平台命令：

```bash
python scripts/doc-test.py --build
```

`scripts/doc-test.py` 自动扫描 `README.md` 和 `docs/**/*.md`，抽取语言为 `cay`、`cavvy`、`eol` 的代码块。

## 代码块标记

默认标记会运行 `cay-check`：

````text
```cay
public class Example {
    public static void main() {
        println("checked");
    }
}
```
````

运行示例：

````text
```cay run
public class Example {
    public static void main() {
        println("runs");
    }
}
```
````

带 feature 的示例会用 `cayc` 编译：

````text
```cay compile feature=top_level_function
public int main() {
    return 0;
}
```
````

非完整程序不要标为 `cay`。用 `text` 展示片段，或显式写 `cay ignore`。
