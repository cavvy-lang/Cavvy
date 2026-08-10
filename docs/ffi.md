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

## `#include_c` 的 C++ 头文件支持

> 注：`#include_c` 面向 C/C++ 头文件，只产出 C 类型集的 FFI 声明。若要按
> C 式 `.h`/`.c` 分离共享 Cavvy 声明（enum 等高级 ADT、native 类声明），
> 请使用 `.cayh` 声明文件与 `#include_h`，见
> [预处理器指南](preprocessor.md#cavvy-声明文件cayh)。

`#include_c` 默认直接解析磁盘上的真实头文件；唯一的例外是 `<...>` 系统形式且
头名命中标准库白名单包装（`caylibs/c/<name>.cay`，如 stdio/stdlib/string 等）时
使用手写包装。`"..."` 形式永不匹配 `.cay` 包装——同名 Cay 文件不会遮蔽真实头文件。
当头文件扩展名为 `.hpp`/`.hh`/`.hxx`，或内容中出现 `class`/`template`/`namespace`/`extern "C++"` 时，
提取器进入 C++ 模式，支持**无模板**的 C++ 头文件：

- class/struct 提取为 Cay **`interop class`**：数据成员按声明顺序镜像为等尺寸
  Cay 字段（布局与 C++ 一致），构造/析构/成员函数/静态成员函数渲染为 `native`
  声明，由 Cay 编译器按 Itanium ABI（g++/clang/MinGW，不支持 MSVC）mangle
  链接名；
- 顶层自由函数按 Itanium mangle 后以 `<ns>__<fn>` 别名注入 `extern` 块；
  `extern "C"` 块内保持 C 链接。

以这个头文件为例：

```cpp
namespace demo {
class Counter {
public:
    Counter();
    Counter(int v);
    ~Counter();
    void add(int delta);
    int value() const;
    static int version();
private:
    int v_;
};
int twice(int x);
}
```

提取结果形如：

```cay ignore
extern {
    c_int _ZN4demo5twiceEi(c_int) as demo__twice;
}
namespace demo {
    interop class Counter {
        public c_int v_;
        public native Counter();
        public native Counter(c_int p0);
        public native ~Counter();
        public native void add(c_int p0);
        public native c_int value() const;
        public native static c_int version();
    }
}
```

Cavvy 侧用**原生类语法**：`new` 构造、方法调用、离开作用域自动析构（RAII）：

```cay ignore
#include_c "demo_include_cpp.h"

using demo::Counter;

public class Main {
    public static void main() {
        Counter c = new Counter(40);   // 调用 C++ 构造函数
        c.add(2);                      // 成员函数（_ZN4demo7Counter3addEi）
        println(c.value());            // const 成员函数（_ZNK4demo7Counter5valueEv）
        println(c.v_);                 // 字段镜像（private 成员也会镜像）
        println(Counter::version());   // 静态成员函数
        println(demo__twice(21));      // 命名空间自由函数维持别名形式
    }   // 离开作用域自动调用 ~Counter（RAII）
}
```

注意事项：

- **对象布局**：标量数据成员镜像为等尺寸 Cay 字段（`int`→`c_int`、
  `long`→`c_long`、`float`→`c_float`、`bool`→`c_bool` 等），指针/引用成员
  镜像为 `c_void*`；字段按声明顺序排列，布局与 C++ 一致，可直接读写；
- **哪些类能 `new`**：含基类、位域、按值类成员、模板类型成员、匿名 union
  或未识别类型成员的类**布局不完整**，提取器不生成构造/析构（`new` 被自然
  封死）并告警，此类对象请通过 C++ 工厂函数创建（指针以 `c_void*` 传递）；
- **虚函数**：含 virtual 的类在字段最前补 `c_void* __cpp_vptr;` 保持布局，
  Cay 侧为**直接调用**（静态绑定），不支持虚分派语义；纯虚函数无独立符号，
  跳过并告警；
- **const/重载**：尾随 `const` 成员函数按 Itanium `K` 标记 mangle；仅 const
  区分的重载对跳过 const 版本并告警，其余重载由 Cay 原生重载决议处理；
- **运算符重载**：无法对应 Cay 方法语法，维持 `<Class>__operator_<op>`
  自由函数别名形式（首参 `c_void*` 为 `this`），如
  `demo__Counter__operator_plus(a, b)`；
- **跳过并告警**：模板声明、嵌套类（Cay 无嵌套类语法）、静态数据成员、
  按值传递/返回 class 类型的函数；
- **链接**：需要把配套 `.cpp` 用 g++/clang++ 编译成库后用 `-L`/`-l` 链接，
  见 `examples/demo_include_cpp.{h,cpp,cay}`。

---

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
