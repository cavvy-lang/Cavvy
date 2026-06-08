# FFI

Cavvy 的 FFI 通过 `extern` 声明 C ABI 函数。默认调用约定是 `cdecl`，也支持 `stdcall`、`fastcall`、`sysv64`、`win64`。

## 基本声明

```cay
extern {
    size_t strlen(c_char* text);
}

public class FfiDemo {
    public static void main() {
        c_char* text = "Cavvy";
        size_t len = strlen(text);
        if (len > 0) {
            println("ffi ok");
        }
    }
}
```

## 别名

当 C 函数名和 Cavvy 方法名冲突时，用 `as` 定义 Cavvy 侧名称：

```text
extern {
    c_double sqrt(c_double x) as c_sqrt;
}
```

## 链接库

使用 `cayc` 的 `-l` 和 `-L`：

```powershell
.\target\release\cayc.exe app.cay app.exe -lm
.\target\release\cayc.exe app.cay app.exe -L.\native -lmyffi
```

Windows 下编译器会在检测到 socket API 时自动加入 `ws2_32`。
