# Cavvy 标准库参考手册

Cavvy 标准库提供全面的系统编程能力，涵盖内存管理、字符串处理、文件 I/O、网络通信、数学计算等核心功能。

---

## 目录

1. [核心类型与命名空间](#核心类型与命名空间)
2. [内存管理 (Allocator)](#内存管理-allocator)
3. [字符串处理](#字符串处理)
4. [文件 I/O](#文件-io)
5. [网络编程](#网络编程)
6. [HTTP 客户端](#http-客户端)
7. [数学计算](#数学计算)
8. [容器](#容器)
9. [可选值类型 (Optional)](#可选值类型-optional)
10. [增强 I/O 工具](#增强-io-工具)
11. [FFI 类型系统](#ffi-类型系统)

---

## 核心类型与命名空间

所有标准库组件位于 `std` 命名空间下。通过 `#include` 引入：

```cay
#include <Allocator.cay>
#include <StringBuilder.cay>
#include <File.cay>
#include <Math.cay>
#include <std/vector.cay>
```

---

## 内存管理 (Allocator)

**文件**: `caylibs/Allocator.cay`

### 接口定义

```cay,ignore
public interface Allocator {
    long allocate(long size);                          // O(1) 分配内存
    long allocateAligned(long size, long alignBytes);  // O(1) 对齐分配
    void deallocate(long ptr);                         // O(1) 释放内存
}
```

### GlobalAlloc - 全局堆分配器

基于 C 标准库 `malloc`/`free` 的全局分配器。

```cay,ignore
public class GlobalAlloc implements Allocator {
    public static GlobalAlloc getInstance();           // 获取单例
    public long allocate(long size);                   // 调用 malloc
    public long allocateAligned(long size, long alignBytes);
    public void deallocate(long ptr);                  // 调用 free
}
```

**使用示例**:
```cay
#include <Allocator.cay>

class Main {
    static void main() {
        std::GlobalAlloc alloc = std::GlobalAlloc.getInstance();
        long ptr = alloc.allocate(1024);
        // 使用内存...
        alloc.deallocate(ptr);
    }
}
```

### Arena - 线性分配器

适用于生命周期明确的批量内存分配场景。

```cay,ignore
public class Arena implements Allocator {
    public static Arena create(long capacity);         // O(1) 创建 Arena
    public long allocate(long size);                   // O(1) 线性分配
    public long allocateAligned(long size, long alignBytes);
    public void deallocate(long ptr);                  // 空操作（批量释放）
    public void reset();                               // O(1) 重置分配器
    public long used();                                // O(1) 已用字节数
    public long remaining();                           // O(1) 剩余字节数
}
```

**使用示例**:
```cay
#include <Allocator.cay>

class Main {
    static void main() {
        std::Arena arena = std::Arena.create(1024 * 1024);  // 1MB Arena
        long buf = arena.allocate(256);
        // 多次分配...
        arena.reset();  // 一次性重置所有分配
    }
}
```

### ScopeAlloc - 作用域分配器

配合 `scope` 关键字使用，实现栈式内存管理。

```cay,ignore
public class ScopeAlloc implements Allocator {
    public static ScopeAlloc create();
    public void setMarker(long m);
    public long getMarker();
}
```

### 便捷宏定义

```cay,ignore
#define GLOBAL_ALLOC GlobalAlloc.getInstance()
#define ARENA(capacity) Arena.create(capacity)
#define SCOPE_ALLOC ScopeAlloc.create()
```

---

## 字符串处理

### StringBuilder

**文件**: `caylibs/StringBuilder.cay`

高效的可变字符串构建器，避免字符串拼接的 O(n²) 复杂度。

```cay,ignore
public class StringBuilder {
    // 构造函数
    public StringBuilder();                            // 默认容量 16
    public StringBuilder(int initialCapacity);
    public StringBuilder(String str);
    
    // 追加操作 (均返回 this 支持链式调用)
    public StringBuilder append(String str);           // O(n) 追加字符串
    public StringBuilder append(char c);               // O(1) 追加字符
    public StringBuilder append(int n);                // O(log n) 追加整数
    public StringBuilder append(long n);               // O(log n) 追加长整数
    public StringBuilder append(boolean b);            // O(1) 追加布尔值
    public StringBuilder append(char[] chars);         // O(n) 追加字符数组
    public StringBuilder appendln();                   // 追加换行
    public StringBuilder appendln(String str);         // 追加字符串+换行
    
    // 查询操作
    public int length();                               // O(1) 当前长度
    public int capacity();                             // O(1) 缓冲区容量
    public boolean isEmpty();                          // O(1) 是否为空
    public char charAt(int index);                     // O(1) 获取字符
    
    // 修改操作
    public StringBuilder clear();                      // O(1) 清空
    public StringBuilder delete(int start, int end);   // O(n) 删除范围
    public StringBuilder insert(int offset, String str); // O(n) 插入
    public StringBuilder reverse();                    // O(n) 反转
    public void setLength(int newLength);              // O(n) 设置长度
    
    // 转换操作
    public String toString();                          // O(n) 转为字符串
    public String substring(int start);                // O(n) 子串
    public String substring(int start, int end);       // O(n) 子串范围
    public int indexOf(String str);                    // O(n*m) 查找
    public StringBuilder replace(String target, String replacement); // O(n*m)
    
    // FFI 支持
    public long c_str();                               // 获取 C 字符串指针
    public static StringBuilder fromCString(long ptr); // 从 C 字符串创建
}
```

**使用示例**:
```cay
#include <StringBuilder.cay>

class Main {
    static void main() {
        std::StringBuilder sb = new std::StringBuilder();
        sb.append("Hello").append(", ").append("World").appendln("!");
        sb.append("Count: ").append(42);
        String result = sb.toString();
        println(result);  // "Hello, World!\nCount: 42"
    }
}
```

### StringPlus

**文件**: `caylibs/StringPlus.cay`

字符串增强工具类。

```cay,ignore
public class StringPlus {
    // 分割操作
    public static String[] split(String str);          // O(n) 按空格分割
    public static String[] split(String str, String delimiter); // O(n*m)

    // 格式化操作
    public static String format(String template, String... args);      // {} 占位符
    public static String formatIndexed(String template, String... args); // {0}, {1} 占位符
}
```

**使用示例**:
```cay
#include <StringPlus.cay>

class Main {
    static void main() {
        String[] parts = std::StringPlus.split("a,b,c", ",");
        String msg = std::StringPlus.format("Hello, {}! You have {} messages.", "Alice", "5");
        String msg2 = std::StringPlus.formatIndexed("{0} + {1} = {2}", "1", "2", "3");
    }
}
```

---

## 文件 I/O

**文件**: `caylibs/File.cay`

### 错误处理

错误处理统一使用 `std::Result<T, E>` 与 `std::IOError`（见 `caylibs/Result.cay` 与 `caylibs/Error.cay`）：

```cay,ignore
public enum IOErrorKind {
    NotFound,
    PermissionDenied,
    UnexpectedEof,
    InvalidInput,
    Other
}

public class IOError implements Error {
    public IOError(IOErrorKind kind, int rawOsError, String msg);
    public IOError(IOErrorKind kind, String msg);
    public IOErrorKind kind();
    public int rawOsError();
    public String message();
    public Optional<Error> cause();
}
```

bool 风格 API 的最后错误码仍以 `FILE_ERROR_*` 整数常量返回（`getLastError()` / `getLastFileError()`）。

### 文件模式

```cay,ignore
public class FileMode {
    public static FileMode read();                     // "r"  只读
    public static FileMode write();                    // "w"  只写（创建/截断）
    public static FileMode append();                   // "a"  追加
    public static FileMode readWrite();                // "r+" 读写
    public static FileMode writeRead();                // "w+" 读写（创建/截断）
    public static FileMode appendRead();               // "a+" 读写追加
    public static FileMode custom(String mode);
}
```

### 定位原点

```cay,ignore
public class SeekOrigin {
    public static SeekOrigin begin();                  // 文件开头 (SEEK_SET)
    public static SeekOrigin current();                // 当前位置 (SEEK_CUR)
    public static SeekOrigin end();                    // 文件末尾 (SEEK_END)
}
```

### File 类

```cay,ignore
public class File {
    // 构造函数
    public File();
    public File(String path, FileMode mode);
    
    // 打开/关闭
    public bool open(String path, FileMode mode);      // O(1) 打开文件
    public static Result<File, IOError> openResult(String path, FileMode mode);
    public bool close();                               // O(1) 关闭文件
    public bool isOpened();                            // O(1) 检查是否打开
    
    // 状态查询
    public bool isEof();                               // O(1) 是否到文件尾
    public bool hasError();                            // O(1) 是否有错误
    public void clearError();                          // O(1) 清除错误
    public int getLastError();                         // O(1) 获取最后错误码
    public long position();                            // O(1) 当前位置
    public long size();                                // O(1) 文件大小
    
    // 定位操作
    public bool seek(long offset, SeekOrigin origin);  // O(1) 定位
    public void rewind();                              // O(1) 重置到开头
    
    // 读写操作
    public int readChar();                             // O(1) 读一个字符
    public bool writeChar(int charCode);               // O(1) 写一个字符
    public long readBytes(c_void* buffer, long size);  // O(n) 读字节块
    public long writeBytes(c_void* buffer, long size); // O(n) 写字节块
    public String readLine(int maxLength);             // O(n) 读一行
    public bool writeString(String str);               // O(n) 写字符串
    public bool writeLine(String str);                 // O(n) 写一行
    public int writeInterpolated(String template, String... args); // O(n) 模板写入
    public String readAllText();                       // O(n) 读取全部文本
    public bool writeAllText(String content);          // O(n) 写入全部文本
    public bool flush();                               // O(1) 刷新缓冲区
    
    // 属性访问
    public String getPath();
    public FileMode getMode();
    
    // 静态工具方法
    public static bool exists(String path);            // O(1) 文件是否存在
    public static Result<boolean, IOError> existsResult(String path);
    public static bool delete(String path);            // O(1) 删除文件
    public static bool rename(String oldPath, String newPath); // O(1) 重命名
    public static bool copy(String srcPath, String dstPath, bool overwrite);
    public static long getSize(String path);
}
```

### FileInfo 类

```cay,ignore
public class FileInfo {
    public static FileInfo fromPath(String path);
    public bool exists();
    public long getSize();
    public String getPath();
}
```

**使用示例**:
```cay
#include <File.cay>

class Main {
    static void main() {
        // 写入文件
        std::File file = new std::File();
        if (file.open("test.txt", std::FileMode.write())) {
            file.writeLine("Hello, Cavvy!");
            file.close();
        }

        // 读取文件
        if (file.open("test.txt", std::FileMode.read())) {
            String content = file.readAllText();
            println(content);
            file.close();
        }
    }
}
```

---

## 网络编程

**文件**: `caylibs/Network.cay`

### SocketAddr - 网络地址

```cay,ignore
public class SocketAddr {
    public SocketAddr();
    public SocketAddr fromString(String ip, int port);     // O(1) 从字符串创建
    public SocketAddr fromIpPort(String ip, int port);     // fromString 别名
    public SocketAddr localhost(int port);                 // O(1) 127.0.0.1:port
    public SocketAddr any(int port);                       // O(1) 0.0.0.0:port
    
    public int getPort();                                  // O(1) 获取端口
    public int port();                                     // O(1) 端口别名
    public int family();                                   // O(1) 地址族
    public int addr();                                     // O(1) IP地址（网络序）
    public String getIp();                                 // O(1) 获取IP字符串
}
```

### NetworkUtils - 网络工具

```cay,ignore
public class NetworkUtils {
    public static bool init();                             // O(1) 初始化网络库
    public static void cleanup();                          // O(1) 清理网络库
    public static int getLastError();                      // O(1) 获取最后错误
    
    // 字节序转换
    public static int htons(int hostshort);                // O(1) 主机序转网络序(16位)
    public static int htonl(int hostlong);                 // O(1) 主机序转网络序(32位)
    public static int ntohs(int netshort);                 // O(1) 网络序转主机序(16位)
    public static int ntohl(int netlong);                  // O(1) 网络序转主机序(32位)
    
    // DNS 解析
    public static String resolveHost(String hostname);     // O(n) 解析主机名
    
    // 便捷创建
    public static TcpSocket connectTcp(String ip, int port); // O(1) 连接TCP
    public static UdpSocket createUdp();                   // O(1) 创建UDP
    public static TcpServer createTcpServer(int port);     // O(1) 创建TCP服务器
}
```

### TcpSocket - TCP客户端

```cay,ignore
public class TcpSocket {
    public TcpSocket();
    
    // 连接管理
    public bool connectTo(String ip, int port);            // O(1) 连接到服务器
    public void close();                                   // O(1) 关闭连接
    public bool isConnected();                             // O(1) 是否已连接
    public bool isValid();                                 // O(1) 是否有效
    
    // 数据传输
    public int send(String data);                          // O(n) 发送数据
    public String receive(int maxLen);                     // O(n) 接收数据
    public String receiveString(int maxLen);               // receive 别名
    
    // 半关闭
    public bool shutdownWrite();                           // O(1) 关闭写入端
    public bool shutdownRead();                            // O(1) 关闭读取端
    public bool shutdownBoth();                            // O(1) 关闭两端
    
    // Socket 选项
    public void setReuseAddr(bool enable);                 // O(1) 地址重用
    public void setTcpNoDelay(bool enable);                // O(1) 禁用Nagle
    public void setSendBufferSize(int size);               // O(1) 发送缓冲区
    public void setRecvBufferSize(int size);               // O(1) 接收缓冲区
    public void setSendTimeout(int ms);                    // O(1) 发送超时
    public void setRecvTimeout(int ms);                    // O(1) 接收超时
}
```

### TcpServer - TCP服务器

```cay,ignore
public class TcpServer {
    public TcpServer();
    
    // 服务器管理
    public bool bindTo(int port);                          // O(1) 绑定端口
    public bool listen(int backlog);                       // O(1) 开始监听
    public TcpSocket accept();                             // O(1) 接受连接
    public void close();                                   // O(1) 关闭服务器
    public bool isValid();                                 // O(1) 是否有效
    
    // Socket 选项
    public void setReuseAddr(bool enable);
}
```

### UdpSocket - UDP套接字

```cay,ignore
public class UdpSocket {
    public UdpSocket();
    
    // 绑定和关闭
    public bool bind(int port);                            // O(1) 绑定端口
    public void close();                                   // O(1) 关闭
    public bool isValid();                                 // O(1) 是否有效
    
    // 数据传输
    public int sendTo(String data, SocketAddr addr);       // O(n) 发送数据报
    public String receiveFrom(int maxLen, SocketAddr fromAddr); // O(n) 接收数据报
    
    // Socket 选项
    public void setBroadcast(bool enable);                 // O(1) 广播选项
    public void setReuseAddr(bool enable);
}
```

**使用示例**:
```cay
#include <Network.cay>

class Main {
    static void main() {
        // TCP 客户端
        std::TcpSocket client = std::NetworkUtils.connectTcp("127.0.0.1", 8080);
        if (client != null && client.isConnected()) {
            client.send("Hello, Server!");
            String response = client.receive(1024);
            println(response);
            client.close();
        }
        
        // TCP 服务器
        std::TcpServer server = std::NetworkUtils.createTcpServer(8080);
        if (server != null) {
            println("Server listening on port 8080");
            std::TcpSocket client = server.accept();
            if (client != null) {
                String msg = client.receive(1024);
                client.send("Echo: " + msg);
                client.close();
            }
            server.close();
        }
    }
}
```

---

## HTTP 客户端

**文件**: `caylibs/EasyHTTP.cay`

### HttpHeaders - HTTP头部管理

```cay,ignore
public class HttpHeaders {
    public HttpHeaders();
    
    public HttpHeaders set(String name, String value);     // O(n) 设置/更新头部
    public HttpHeaders add(String name, String value);     // O(1) 添加头部
    public String get(String name);                        // O(n) 获取头部值
    public String[] getAll(String name);                   // O(n) 获取所有同名头部
    public HttpHeaders remove(String name);                // O(n) 移除头部
    public bool contains(String name);                     // O(n) 是否包含
    public int size();                                     // O(1) 头部数量
    public HttpHeaders clear();                            // O(1) 清空
    public String build();                                 // O(n) 构建头部字符串
}
```

### HttpParams - URL参数管理

```cay,ignore
public class HttpParams {
    public HttpParams();
    
    public HttpParams add(String name, String value);      // O(1) 添加参数
    public HttpParams add(String name, int value);         // O(1) 添加整数参数
    public HttpParams add(String name, long value);        // O(1) 添加长整数参数
    public HttpParams add(String name, bool value);        // O(1) 添加布尔参数
    public String build();                                 // O(n) 构建查询字符串
    public bool isEmpty();                                 // O(1) 是否为空
    public int size();                                     // O(1) 参数数量
    public HttpParams clear();                             // O(1) 清空
}
```

### HttpResponse - HTTP响应

```cay,ignore
public class HttpResponse {
    public int getStatusCode();                            // O(1) 状态码
    public String getStatusText();                         // O(1) 状态文本
    public HttpHeaders getHeaders();                       // O(1) 响应头部
    public String getBody();                               // O(1) 响应体
    public long getResponseTime();                         // O(1) 响应时间(ms)
    public String getError();                              // O(1) 错误信息
    public bool isSuccess();                               // O(1) 是否成功(2xx)
    public bool isJson();                                  // O(n) 是否为JSON响应
    public String toString();                              // O(n) 字符串表示
}
```

### HttpRequest - HTTP请求构建器

```cay,ignore
public class HttpRequest {
    public HttpRequest(String url);
    
    // 构建器方法（链式调用）
    public HttpRequest method(String method);              // 设置方法
    public HttpRequest header(String name, String value);  // 设置头部
    public HttpRequest param(String name, String value);   // 设置URL参数
    public HttpRequest body(String body);                  // 设置请求体
    public HttpRequest timeout(int connectMs, int readMs); // 设置超时
    public HttpRequest followRedirects(bool follow);       // 是否跟随重定向
    
    // 便捷方法
    public HttpRequest json(String jsonBody);              // 发送JSON
    public HttpRequest form(String... keyValues);          // 发送表单
    
    // 执行请求
    public HttpResponse send();                            // O(n) 发送请求
    public HttpResponse get();                             // GET 请求
    public HttpResponse post();                            // POST 请求
    public HttpResponse put();                             // PUT 请求
    public HttpResponse delete();                          // DELETE 请求
}
```

### EasyHTTP - 静态工具类

```cay,ignore
public class EasyHTTP {
    // 便捷 GET 请求
    public static HttpResponse get(String url);
    public static HttpResponse get(String url, HttpHeaders headers);
    public static HttpResponse get(String url, HttpParams params);
    public static HttpResponse get(String url, HttpHeaders headers, HttpParams params);
    
    // 便捷 POST 请求
    public static HttpResponse post(String url, String body);
    public static HttpResponse post(String url, String body, HttpHeaders headers);
    public static HttpResponse postJson(String url, String json);
    public static HttpResponse postForm(String url, String... keyValues);
    
    // 其他方法
    public static HttpResponse put(String url, String body);
    public static HttpResponse delete(String url);
    public static HttpResponse head(String url);
    public static HttpResponse options(String url);
    
}
```

**使用示例**:
```cay
#include <EasyHTTP.cay>

class Main {
    static void main() {
        // 简单 GET
        http::HttpResponse resp = http::EasyHTTP.get("https://api.example.com/data");
        if (resp.isSuccess()) {
            println(resp.getBody());
        }

        // 带参数的 GET
        http::HttpParams params = new http::HttpParams();
        params.add("page", 1).add("limit", 10);
        resp = http::EasyHTTP.get("https://api.example.com/items", params);

        // POST JSON
        resp = http::EasyHTTP.postJson("https://api.example.com/users",
            "{\"name\":\"Alice\",\"age\":30}");
    }
}
```

---

## 数学计算

**文件**: `caylibs/Math.cay`

### 数学常量

```cay,ignore
#define MATH_PI         3.14159265358979323846   // 圆周率
#define MATH_E          2.71828182845904523536   // 自然对数底
#define MATH_LN2        0.69314718055994530942   // ln(2)
#define MATH_LN10       2.30258509299404568402   // ln(10)
#define MATH_LOG2E      1.44269504088896340736   // log2(e)
#define MATH_LOG10E     0.43429448190325182765   // log10(e)
#define MATH_SQRT2      1.41421356237309504880   // sqrt(2)
#define MATH_SQRT1_2    0.70710678118654752440   // 1/sqrt(2)
#define MATH_DEG_TO_RAD 0.01745329251994329577   // 度转弧度
#define MATH_RAD_TO_DEG 57.2957795130823208768   // 弧度转度
#define MATH_EPSILON    1e-10                    // 浮点精度容差
```

### Math 类 - 静态数学工具

```cay,ignore
public class Math {
    // 三角函数 (O(1))
    public static double sin(double x);
    public static double cos(double x);
    public static double tan(double x);
    public static double asin(double x);
    public static double acos(double x);
    public static double atan(double x);
    public static double atan2(double y, double x);
    
    // 双曲函数 (O(1))
    public static double sinh(double x);
    public static double cosh(double x);
    public static double tanh(double x);
    
    // 指数和对数 (O(1))
    public static double exp(double x);
    public static double log(double x);
    public static double log10(double x);
    public static double log2(double x);
    public static double logBase(double x, double base);
    
    // 幂函数 (O(1))
    public static double pow(double x, double y);
    public static double sqrt(double x);
    public static double cbrt(double x);
    public static double sqr(double x);
    
    // 取整函数 (O(1))
    public static double ceil(double x);
    public static double floor(double x);
    public static double round(double x);
    public static double trunc(double x);
    public static double frac(double x);
    
    // 绝对值 (O(1))
    public static double abs(double x);
    public static int abs(int x);
    public static long abs(long x);
    
    // 符号和取模 (O(1))
    public static int sign(double x);
    public static double fmod(double x, double y);
    
    // 角度转换 (O(1))
    public static double toRadians(double degrees);
    public static double toDegrees(double radians);
    
    // 最值函数 (O(1))
    public static int max(int a, int b);
    public static double max(double a, double b);
    public static long max(long a, long b);
    public static int min(int a, int b);
    public static double min(double a, double b);
    public static long min(long a, long b);
    public static int clamp(int value, int min, int max);
    public static double clamp(double value, double min, double max);
    
    // 比较函数 (O(1))
    public static bool approxEqual(double a, double b, double epsilon);
    public static bool approxEqual(double a, double b);
    public static bool approxEqualRelative(double a, double b, double epsilon);
    
    // 插值函数 (O(1))
    public static double lerp(double a, double b, double t);
    public static int lerp(int a, int b, double t);
    public static double smoothStep(double a, double b, double t);
    
    // GCD 和 LCM (O(log(min(a,b))))
    public static int gcd(int a, int b);
    public static int lcm(int a, int b);
}
```

### Random 类 - 随机数生成器

```cay,ignore
public class Random {
    public static void init();                             // O(1) 初始化（使用时间种子）
    public static void setSeed(int seed);                  // O(1) 设置种子
    
    public static int nextInt();                           // O(1) [0, RAND_MAX]
    public static int nextInt(int bound);                  // O(1) [0, bound)
    public static int nextInt(int min, int max);           // O(1) [min, max]
    public static double nextDouble();                     // O(1) [0.0, 1.0)
    public static double nextDouble(double min, double max); // O(1) [min, max)
    public static bool nextBool();                         // O(1) true/false
    public static double nextGaussian();                   // O(1) 正态分布
}
```

**使用示例**:
```cay
#include <Math.cay>

class Main {
    static void main() {
        // 三角函数
        double rad = std::Math.toRadians(45);
        double s = std::Math.sin(rad);
        
        // 随机数
        std::Random.init();
        int r = std::Random.nextInt(100);  // 0-99
        double d = std::Random.nextDouble(0.0, 1.0);
        
        // 插值
        double val = std::Math.lerp(0.0, 100.0, 0.5);  // 50.0
        int clamped = std::Math.clamp(150, 0, 100);    // 100
    }
}
```

---

## 容器

### vector<T>

**文件**: `caylibs/std/vector.cay`

`std::vector<T>` 是基于 Cavvy 内置数组 `T[]` 的源代码级动态数组。它只保存数组缓冲区和逻辑长度，不引入额外运行时对象；元素存储和访问沿用内置数组表示。容量按 2 倍增长，`push_back` 仅在容量不足时搬迁元素；`pop_back`、`clear` 和容量内的 `resize` 不重新分配缓冲区。

```cay,ignore
public class vector<T> {
    public vector();                                  // O(1) 创建空 vector
    public vector(int n);                            // O(n) 创建 n 个默认元素
    public vector(int n, T val);                     // O(n) 创建并填充值

    public T get(int index);                         // O(1)
    public T at(int index);                          // O(1)
    public void set(int index, T val);               // O(1)
    public T front();                                // O(1)
    public T back();                                 // O(1)

    public void push_back(T val);                    // 均摊 O(1)
    public void pop_back();                          // O(1)
    public void erase(int index);                    // O(n)
    public void clear();                             // O(1)
    public void resize(int n);                       // O(k)，k 为新增默认槽位数；扩容时 O(size)
    public void resize(int n, T val);                // O(k)，k 为新增填充值数；扩容时 O(size)
    public void reserve(int n);                      // 容量不足时 O(size)，否则 O(1)
    public void shrink_to_fit();                     // O(n)

    public int size();                               // O(1)
    public int length();                             // O(1)
    public int capacity();                           // O(1)
    public bool empty();                             // O(1)
}
```

**使用示例**:
```cay
#include <std/vector.cay>

using std::vector;

public int main() {
    vector<int> nums = new vector<int>();
    nums.push_back(10);
    nums.push_back(20);
    nums.set(1, 25);

    println(nums.get(0));    // 10
    println(nums.back());    // 25
    println(nums.size());    // 2
    return 0;
}
```

---

## 可选值类型 (Optional)

**文件**: `caylibs/Optional.cay`

零开销可选值容器，编译期通过单态化为每个具体类型生成特化代码。

```cay,ignore
public class Optional<T> {
    // 构造方法
    public static Optional<T> of(T value);               // O(1) 创建有值 Optional
    public static Optional<T> empty();                   // O(1) 创建空 Optional
    
    // 查询方法 (O(1))
    public boolean isPresent();                          // 是否有值
    public boolean isEmpty();                            // 是否为空
    
    // 取值方法 (O(1))
    public T get();                                      // 获取值（不安全）
    public T orElse(T defaultValue);                     // 安全取值（带默认值）
}
```

**使用示例**:
```cay
#include <Optional.cay>

using std::Optional;

class Main {
    static void main() {
        Optional<int> maybeValue = Optional.of(42);

        if (maybeValue.isPresent()) {
            int val = maybeValue.get();
            println(val);  // 42
        }

        Optional<int> empty = Optional.empty();
        int result = empty.orElse(0);  // 0
    }
}
```

---

## Result 与错误处理 (6.1.0 / 6.2.0 补完)

**文件**: `caylibs/Result.cay`、`caylibs/Error.cay`、`caylibs/Into.cay`

显式错误传播容器。`unwrap`/`unwrapErr`/`expect` 在状态不符时 panic；
`map` 家族为实例泛型方法（新类型参数在调用点从 lambda 推断并单态化），
支持链式调用（`r.map(f).map(g).getValue()`）与 `auto` 推断；lambda 参数
类型按期望的 `fn` 签名自动确定，块体 lambda（`{ ... }`）不参与方法级类型实参推断。

```cay,ignore
public class Result<T, E> {
    // 构造 (O(1))
    public static Result<T, E> ok(T value);
    public static Result<T, E> err(E error);

    // 检查 (O(1))
    public boolean isOk();
    public boolean isErr();

    // 取值 (O(1))，状态不符时 panic
    public T unwrap();
    public T unwrapOr(T defaultValue);
    public T unwrapOrElse(fn(E) -> T handler);
    public T expect(String message);
    public E unwrapErr();

    // 转换（实例泛型方法）
    public Result<U, E> map<U>(fn(T) -> U mapper);
    public Result<T, F> mapErr<F>(fn(E) -> F mapper);
    public Result<U, E> andThen<U>(fn(T) -> Result<U, E> handler);
    public Result<U, E> flatMap<U>(fn(T) -> Result<U, E> mapper);

    // 副作用
    public Result<T, E> inspect(fn(T) -> void action);
    public Result<T, E> inspectErr(fn(E) -> void action);
}

// 错误层级
public interface Error { String message(); Optional<Error> cause(); }
public enum IOErrorKind { NotFound, PermissionDenied, UnexpectedEof, InvalidInput, Other }
public class IOError implements Error { /* kind()/rawOsError()/message()/cause() */ }
public class ParseError implements Error { /* line()/column()/sourceSnippet() */ }

// ? 运算符错误转换
public interface Into<T> { T into(); }
```

`?` 传播时，表达式错误类型 E 与函数返回错误类型 E2 不同且 E 实现 `Into<E2>` 的，
自动插入 `e.into()` 转换。一个类可同时实现多个 `Into` 实例化
（`Into<A>` 与 `Into<B>` 各提供一个仅返回类型不同的 `into()`），`?` 按目标
错误类型静态分派到正确的 `into()`；这些重载不能被普通调用直接命中
（`e.into()` 报歧义错误），经接口引用的动态分派也只命中其中之一
（vtable 槽位按裸方法名分配）。

---

## 增强 I/O 工具

**文件**: `caylibs/IOPlus.cay`

提供类似 Python 的便捷打印功能。

```cay,ignore
public class IOPlus {
    // 字符串可变参数打印
    public static void prints(String... args);           // 空格分隔 + 换行
    public static void printsNoLn(String... args);       // 空格分隔，不换行
    public static void printsSep(String separator, String... args);      // 指定分隔符
    public static void printsSepNoLn(String separator, String... args);  // 指定分隔符，不换行
    
    // 整数可变参数打印
    public static void printi(int... args);              // 空格分隔 + 换行
    public static void printiNoLn(int... args);          // 空格分隔，不换行
    
    // 浮点数可变参数打印
    public static void printfl(float... args);           // float 版本
    public static void printdb(double... args);          // double 版本
    
    // 混合类型打印
    public static void printsi(String s1, int i1);       // 字符串 + 整数
    public static void printsf(String s1, float f1);     // 字符串 + float
    public static void printsis(String s1, int i1, String s2);  // 字符串 + 整数 + 字符串
    public static void printssi(String s1, String s2, int i1);  // 字符串 + 字符串 + 整数
    
    // 输入方法
    public static String input(String prompt);           // 显示提示并读取输入
    public static String input();                        // 读取输入
    public static int inputInt(String prompt);           // 读取整数
    public static float inputFloat(String prompt);       // 读取浮点数
    
    // 辅助方法
    public static void println();                        // 打印空行
    public static void repeat(String str, int count);    // 重复打印
    public static void repeatLn(String str, int count);  // 重复打印 + 换行
    public static void divider(int length);              // 水平分割线
    public static void divider(int length, String ch);   // 指定字符的分割线
}
```

**使用示例**:
```cay
#include <IOPlus.cay>

class Main {
    static void main() {
        // 便捷打印
        std::IOPlus.prints("Hello", "World");        // "Hello World\n"
        std::IOPlus.printi(1, 2, 3);                  // "1 2 3\n"
        std::IOPlus.printsSep(", ", "a", "b", "c");  // "a, b, c\n"
        
        // 输入
        String name = std::IOPlus.input("Enter name: ");
        int age = std::IOPlus.inputInt("Enter age: ");
        
        // 分割线
        std::IOPlus.divider(20);           // "--------------------"
        std::IOPlus.divider(20, "=");      // "===================="
    }
}
```

---

## FFI 类型系统

**文件**: `caylibs/std/ffi.cay`, `caylibs/std/ffia.cay`

### 原始 FFI 类型

| Cavvy 类型 | C 类型 | 说明 |
|-----------|--------|------|
| `c_char` | `char` | 8位字符 |
| `c_uchar` | `unsigned char` | 无符号8位 |
| `c_short` | `short` | 16位有符号 |
| `c_ushort` | `unsigned short` | 无符号16位 |
| `c_int` | `int` | 32位有符号 |
| `c_uint` | `unsigned int` | 无符号32位 |
| `c_long` | `long` | 平台相关 |
| `c_ulong` | `unsigned long` | 无符号长整型 |
| `c_float` | `float` | 32位浮点 |
| `c_double` | `double` | 64位浮点 |
| `c_bool` | `_Bool` | C99布尔 |
| `c_void` | `void` | 空类型 |
| `c_string` | `char*` | C字符串 |
| `size_t` | `size_t` | 大小类型 |
| `ssize_t` | `ssize_t` | 有符号大小 |
| `intptr_t` | `intptr_t` | 指针宽度整数 |
| `uintptr_t` | `uintptr_t` | 无符号指针宽度 |

### 类型别名 (std/ffi.cay)

```cay
// 指针类型别名
alias ptr = c_void*;
alias void_ptr = c_void*;
alias const_void_ptr = c_void*;
alias char_ptr = c_char*;
alias const_char_ptr = c_char*;

// 大写风格类型别名
alias CInt = c_int;
alias CLong = c_long;
alias CShort = c_short;
alias CChar = c_char;
alias CByte = c_byte;
alias CUInt = c_uint;
alias CULong = c_ulong;
alias CUShort = c_ushort;
alias CUChar = c_uchar;
alias CFloat = c_float;
alias CDouble = c_double;
alias CBool = c_bool;
alias CVoid = c_void;

// 固定宽度整数
alias Int8T = int8_t;
alias Int16T = int16_t;
alias Int32T = int32_t;
alias Int64T = int64_t;
alias UInt8T = uint8_t;
alias UInt16T = uint16_t;
alias UInt32T = uint32_t;
alias UInt64T = uint64_t;

// 裸指针类型
alias RawPtrInt = c_int*;
alias RawPtrLong = c_long*;
alias RawPtrVoid = c_void*;
alias RawPtrChar = c_char*;
alias RawPtrByte = c_byte*;
alias RawPtrFloat = c_float*;
alias RawPtrDouble = c_double*;
```

### C 标准库函数声明

**stdio.h** (`caylibs/c/stdio.cay`):
```cay
extern {
    c_int printf(c_string fmt, ...);
    c_int fprintf(c_void* stream, c_string fmt, ...);
    c_int sprintf(c_char* str, c_string fmt, ...);
    c_int snprintf(c_char* str, size_t size, c_string fmt, ...);
    c_int scanf(c_string fmt, ...);
    c_void* fopen(c_string filename, c_string mode);
    c_int fclose(c_void* stream);
    size_t fread(c_void* ptr, size_t size, size_t nmemb, c_void* stream);
    size_t fwrite(c_void* ptr, size_t size, size_t nmemb, c_void* stream);
    c_int fseek(c_void* stream, c_long offset, c_int whence);
    c_long ftell(c_void* stream);
    // ... 更多函数
}
```

**stdlib.h** (`caylibs/c/stdlib.cay`):
```cay
extern {
    c_void* malloc(size_t size);
    c_void* calloc(size_t nmemb, size_t size);
    c_void* realloc(c_void* ptr, size_t size);
    void free(c_void* ptr);
    void exit(c_int status);
    void qsort(c_void* base, size_t nmemb, size_t size, CompareFn compar);
    c_void* bsearch(c_void* key, c_void* base, size_t nmemb, size_t size, CompareFn compar);
    c_int rand();
    void srand(c_uint seed);
    c_int atoi(c_string nptr);
    c_double atof(c_string nptr);
}
```

**string.h** (`caylibs/c/string.cay`):
```cay
extern {
    c_void* memcpy(c_void* dest, c_void* src, size_t n);
    c_void* memmove(c_void* dest, c_void* src, size_t n);
    c_void* memset(c_void* s, c_int c, size_t n);
    c_int memcmp(c_void* s1, c_void* s2, size_t n);
    c_char* strcpy(c_char* dest, c_string src);
    c_char* strncpy(c_char* dest, c_string src, size_t n);
    c_int strcmp(c_string s1, c_string s2);
    size_t strlen(c_string s);
    c_char* strstr(c_string haystack, c_string needle);
}
```

**math.h** (`caylibs/c/math.cay`):
```cay
extern {
    c_double sin(c_double x);
    c_double cos(c_double x);
    c_double tan(c_double x);
    c_double exp(c_double x);
    c_double log(c_double x);
    c_double pow(c_double base, c_double exp);
    c_double sqrt(c_double x);
    c_double ceil(c_double x);
    c_double floor(c_double x);
    c_double fabs(c_double x);
    // ... 更多函数
}
```

**ctype.h** (`caylibs/c/ctype.cay`):
```cay
extern {
    c_int isalnum(c_int c);
    c_int isalpha(c_int c);
    c_int isdigit(c_int c);
    c_int isspace(c_int c);
    c_int islower(c_int c);
    c_int isupper(c_int c);
    c_int tolower(c_int c);
    c_int toupper(c_int c);
}
```

**time.h** (`caylibs/c/time.cay`):
```cay
extern {
    c_int64_t time(c_int64_t* timer);
    c_double difftime(c_int64_t end, c_int64_t start);
    c_int64_t clock();
    c_string ctime(c_int64_t* timer);
}
```

---

## 5.2.0～6.1.0 新增库模块

| 模块/类型 | 版本 | 用途 |
|---|---:|---|
| `std::sys` | 5.3.0 | 进程、环境变量和命令行参数 |
| `std::ArrayList<T>` / `std::vector<T>` | 5.3.0 | 泛型容器和迭代器 |
| `UniquePtr<T>` / `ScopedPtr<T>` / `Rc<T>` / `WeakPtr<T>` | 5.3.0 | 所有权、RAII、共享引用和弱引用 |
| `Mmap` / `MmapSlice` | 5.4.0 | 跨平台内存映射和零拷贝切片 |
| `Result<T,E>` | 6.1.0 | 显式成功/错误分支和错误传播 |
| `Error` / `IOError` / `ParseError` | 6.1.0 | 统一错误类型层级 |
| `Into<T>` | 6.2.0 | `?` 运算符的错误类型自动转换（6.2.0 起 `File`/`Mmap` 返回 `Result<_, IOError>`） |

详细 API 以 `caylibs/` 源码为准；版本演进和示例见[版本演进总览](release/version-history-5.2-to-6.1.md)。

## 版本信息

- **文档版本**: 6.1.0
- **标准库版本**: 0.5.1.x - 1.1.0
- **最后更新**: 2026-07-14
