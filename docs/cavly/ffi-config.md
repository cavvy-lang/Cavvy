# FFI 配置

Cavly 通过 `cavly.toml` 的 `[ffi]` 段配置外部 C 库链接。支持系统库、第三方库和平台特定配置。

## 系统库

直接链接系统提供的库：

```toml
[ffi]
system_libs = ["m", "pthread", "dl"]
```

| 平台 | 常用系统库 |
|---|---|
| Windows | `user32`, `kernel32`, `gdi32`, `winmm`, `ws2_32` |
| Linux | `m`, `pthread`, `dl`, `rt` |
| macOS | `m`, `pthread` |

**添加系统库**：

```bash
cavly add m          # 数学库
cavly add user32     # Windows GUI
```

## 第三方库

使用 `[ffi.libraries.<名称>]` 配置第三方库：

```toml
[ffi.libraries.sdl2]
name = "SDL2"
lib = "SDL2"
path = "./lib"
static_lib = false
deps = ["SDL2main"]
```

| 字段 | 类型 | 说明 |
|---|---|---|
| `name` | String | 库的显示名称 |
| `lib` | String | 库文件名（不含扩展名和 `lib` 前缀） |
| `path` | String | 库文件搜索路径 |
| `static_lib` | bool | 是否静态链接 |
| `deps` | String[] | 此库依赖的其他库 |

**添加第三方库**：

```bash
cavly ffi sdl2 SDL2
```

## 平台特定配置

同一个库在不同平台上可能需要不同的文件名或路径：

```toml
[ffi.libraries.sdl2]
name = "SDL2"
lib = "SDL2"

[ffi.libraries.sdl2.platform.windows]
lib = "SDL2"
path = "C:/SDL2/lib/x64"
ldflags = ["-lSDL2main", "-lSDL2"]

[ffi.libraries.sdl2.platform.linux]
lib = "SDL2"
path = "/usr/lib/x86_64-linux-gnu"

[ffi.libraries.sdl2.platform.macos]
lib = "SDL2"
path = "/usr/local/lib"
```

平台标识：`windows`、`linux`、`macos`。

## 头文件路径

```toml
[ffi]
include_paths = [
    "./include",
    "C:/SDL2/include",
    "/usr/include/SDL2"
]
```

## 链接选项

```toml
[ffi]
link_options = ["-Wl,-rpath,/usr/local/lib"]
```

## 链接器脚本

```toml
[ffi]
linker_script = "linker.ld"
```

## 完整示例

### SDL2 配置

```toml
[ffi]
system_libs = []

[ffi.libraries.sdl2]
name = "SDL2"
lib = "SDL2"
static_lib = false
deps = ["SDL2main"]

[ffi.libraries.sdl2.platform.windows]
lib = "SDL2"
path = "C:/SDL2/lib/x64"
ldflags = ["-lSDL2main", "-lSDL2"]

[ffi.libraries.sdl2.platform.linux]
lib = "SDL2"
path = "/usr/lib/x86_64-linux-gnu"
```

### OpenGL + GLFW 配置

```toml
[ffi]
system_libs = []

[ffi.libraries.opengl32]
name = "OpenGL"
lib = "opengl32"

[ffi.libraries.opengl32.platform.windows]
lib = "opengl32"

[ffi.libraries.opengl32.platform.linux]
lib = "GL"

[ffi.libraries.glfw3]
name = "GLFW"
lib = "glfw3"
deps = ["opengl32"]

[ffi.libraries.glfw3.platform.windows]
lib = "glfw3"
path = "./lib/glfw"

[ffi.libraries.glfw3.platform.linux]
lib = "glfw"
```

### 使用 FFI 库

在 `.cay` 源文件中声明外部函数：

```cay
extern {
    // SDL2 函数
    int SDL_Init(int flags);
    void SDL_Quit();
    
    // 标准 C 函数
    int printf(String format, ...);
    int abs(int x);
    double sqrt(double x);
}

public class SdlExample {
    public static void main() {
        int result = SDL_Init(0x00000020);  // SDL_INIT_VIDEO
        if (result < 0) {
            println("SDL_Init failed!");
            return;
        }
        println("SDL2 initialized successfully!");
        SDL_Quit();
    }
}
```
