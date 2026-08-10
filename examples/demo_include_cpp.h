// 演示 #include_c 的 C++ 头文件支持（无模板部分会被提取）
//
// 本头文件虽然是 .h 扩展名，但含有 class/namespace/template 关键字，
// 提取器会按 C++ 模式处理：class 提取为 Cay `interop class`（字段镜像 +
// native 构造/析构/方法），命名空间内的自由函数按 Itanium ABI mangle 链接名。

#pragma once

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

// 模板声明：需要 C++ 编译器实例化展开后才能使用，提取器会跳过并告警
template <typename T>
T identity(T v);

} // namespace demo

// RAII 验证：当前存活的 Counter 实例数（C 链接，提取器按 C 原型提取）
extern "C" int demo_counter_alive(void);
