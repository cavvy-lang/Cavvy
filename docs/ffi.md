# FFI 外部函数接口

Cavvy 的 FFI（外部函数接口）允许直接调用 C ABI 函数。默认调用约定是 `cdecl`，也支持 `stdcall`、`fastcall`、`sysv64`、`win64`。

---

## 基本声明

使用 `extern` 块声明 C 函数：

```cay
extern {
    int printf(c_string fmt, ...);
    size_t strlen(c_string str);
}
```

然后在 Cavvy 代码中直接调用：

```cay
#include "std/ffi.cay"

class Main {
    static void main() {
        printf("Hello from C! %d\n", 42);

        c_string msg = "Cavvy";
        size_t len = strlen(msg);
        printf("字符串长度: %d\n", len);
    }
}
```

---

## FFI 类型映射

### 基础类型

| C 类型 | Cavvy FFI 类型 | 说明 |
|---|---|---|
| `char` | `c_char` | 8 位字符 |
| `unsigned char` | `c_uchar` | 无符号 8 位 |
| `short` | `c_short` | 16 位有符号整数 |
| `unsigned short` | `c_ushort` | 16 位无符号整数 |
| `int` | `c_int` | 32 位有符号整数 |
| `unsigned int` | `c_uint` | 32 位无符号整数 |
| `long` | `c_long` | 平台相关长度 |
| `unsigned long` | `c_ulong` | 无符号长整数 |
| `long long` | `c_longlong` | 64 位整数 |
| `float` | `c_float` | 32 位浮点数 |
| `double` | `c_double` | 64 位浮点数 |
| `void` | `c_void` / `void` | 无返回值或 `void*` |
| `char*` | `c_string` | C 风格字符串 |
| `size_t` | `size_t` | 大小类型 |
| `ssize_t` | `ssize_t` | 有符号大小类型 |
| `bool` | `c_bool` | C 布尔值 |
| `intptr_t` | `intptr_t` | 指针宽度整数 |
| `uintptr_t` | `uintptr_t` | 无符号指针宽度整数 |

---

## 调用约定

```cay
// 默认 cdecl
extern {
    int add(int a, int b);
}

// 指定调用约定
extern "stdcall" {
    int win32_api(int param);
}

extern "fastcall" {
    int fast_func(int a, int b);
}

extern "sysv64" {
    int linux_syscall(int code);
}

extern "win64" {
    int windows_x64_func(int param);
}
```

---

## 函数别名

当 C 函数名与 Cavvy 命名冲突时，使用 `as` 语法：

```cay
extern {
    // C 的 sqrt 在 Cavvy 以 c_sqrt 访问
    c_double sqrt(c_double x) as c_sqrt;

    // 避免关键字冲突
    int open(c_string path, int flags) as c_open;
}
```

---

## 链接外部库

使用 `-l` 和 `-L` 链接外部库：

```powershell
# 链接数学库
cayc app.cay -lm

# 链接自定义库
cayc app.cay -L./native -lmyffi

# 链接多个库
cayc app.cay -lssl -lcrypto
```

Windows 下编译器会在检测到 socket API 时自动链接 `ws2_32`。

---

## 标准库 FFI 封装

推荐使用标准库中已封装的 FFI：

```cay
#include "std/ffi.cay"
#include <File.cay>
#include <Math.cay>
```

---

## 完整示例

```cay
#include "std/ffi.cay"

extern {
    int printf(c_string fmt, ...);
    int rand();
    void srand(c_uint seed);
}

class Main {
    static void main() {
        srand(42);
        int r = rand();
        printf("随机数: %d\n", r);
    }
}
```

---

## 注意事项

1. **类型安全**：FFI 调用不进行类型安全检查，错误声明可能造成崩溃
2. **指针管理**：C 堆内存（`malloc`/`free`）不受 Cavvy GC 管理
3. **字符串区别**：`c_string`（`char*`）与 `string` 为不同类型
4. **调用约定**：不同平台需要正确的调用约定，否则栈可能损坏
5. **头文件路径**：通过 `-I` 添加 FFI 头文件搜索路径
