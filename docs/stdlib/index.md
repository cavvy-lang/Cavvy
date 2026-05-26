# Cavvy 标准库文档 (caylib)

## 概述

Cavvy 标准库 (caylib) 提供了一系列用于系统编程、网络通信、文件操作和字符串处理的高级工具类。所有标准库模块都位于 `caylibs/` 目录下，可通过 `#include <模块名.cay>` 引入。

## 标准库模块列表

| 模块 | 文件 | 功能描述 | 版本 |
|------|------|----------|------|
| [Allocator](./allocator.md) | Allocator.cay | 内存分配器接口与实现 | 0.5.0.0 |
| [File](./file.md) | File.cay | 文件操作与IO | 0.5.2.0 |
| [Network](./network.md) | Network.cay | TCP/UDP网络编程 | 0.5.1.0 |
| [EasyHTTP](./easyhttp.md) | EasyHTTP.cay | HTTP客户端 | 0.5.1.0 |
| [StringBuilder](./stringbuilder.md) | StringBuilder.cay | 高效字符串构建 | 0.5.1.0 |
| [StringPlus](./stringplus.md) | StringPlus.cay | 字符串增强工具 | 0.5.0.0 |
| [IOPlus](./ioplus.md) | IOPlus.cay | 输入输出增强 | 0.5.0.0 |
| [Optional<T>](./optional.md) | Optional.cay | 零开销可选值容器 | 0.5.1.x |

## 使用示例

```cay
#include <File.cay>
#include <EasyHTTP.cay>
#include <StringBuilder.cay>

public class Main {
    public static void main(String[] args) {
        // 文件操作
        String content = File.readAllText("data.txt");
        
        // HTTP请求
        HttpResponse response = EasyHTTP.get("https://api.example.com/data");
        
        // 字符串构建
        StringBuilder sb = new StringBuilder();
        sb.append("Hello").append(" ").append("World");
        String result = sb.toString();
    }
}
```

## 模块依赖关系

```
EasyHTTP.cay
    ├── Network.cay
    └── StringBuilder.cay

File.cay
    └── std/ffi.cay

Network.cay
    └── (平台特定: Windows使用ws2_32, Linux使用socket)

StringBuilder.cay
    └── (外部辅助函数: __cay_memcpy_byte, __cay_memset_byte)

Allocator.cay
    └── (C标准库: malloc, free)
```

## 设计原则

1. **面向对象设计**: 所有模块采用类封装，提供清晰的API
2. **错误处理**: 使用Result类型或返回码进行错误处理
3. **资源管理**: 自动资源释放，避免内存泄漏
4. **性能优化**: 标注时间/空间复杂度，避免O(n²)操作
5. **跨平台**: 支持Windows和Linux平台

## 版本兼容性

- 所有标准库模块与 Cavvy 0.5.x 版本兼容
- 建议使用最新版本以获得最佳性能和功能
