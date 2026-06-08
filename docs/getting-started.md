# 快速开始

## 准备工具链

在 Windows 上，仓库需要这些目录存在：

```text
llvm-minimal/
mingw-minimal/
lib/
```

如果缺失，先运行：

```powershell
python setup-llvm.py
```

然后构建 release 版编译器：

```powershell
cargo build --release
```

## 编译程序

```cay run
public class Main {
    public static void main() {
        int x = 10;
        int y = 32;
        println("sum = " + String.valueOf(x + y));
    }
}
```

编译命令：

```powershell
.\target\release\cayc.exe main.cay main.exe
.\main.exe
```

## 只检查代码

`cay-check` 执行预处理、词法分析、语法分析和语义分析，不生成可执行文件：

```powershell
.\target\release\cay-check.exe main.cay
```

常用检查级别：

```powershell
.\target\release\cay-check.exe --lex-only main.cay
.\target\release\cay-check.exe --parse-only main.cay
```

## 顶层函数

Cavvy 默认是面向对象入口。顶层函数是受 feature 控制的扩展：

```cay compile feature=top_level_function
public int main() {
    println("top-level main");
    return 0;
}
```

启用方式：

```powershell
.\target\release\cayc.exe app.cay app.exe --feature=top_level_function
```
