// demo_include_cpp.h 的配套实现：编译为静态库后由 cayc -L/-l 链接

#include "demo_include_cpp.h"

// 存活实例计数：构造 +1、析构 -1，供 Cay 侧验证 RAII 析构被自动调用
static int alive_count = 0;

extern "C" int demo_counter_alive() { return alive_count; }

namespace demo {

Counter::Counter() : v_(0) { alive_count++; }
Counter::Counter(int v) : v_(v) { alive_count++; }
Counter::~Counter() { alive_count--; }
void Counter::add(int delta) { v_ += delta; }
int Counter::value() const { return v_; }
int Counter::version() { return 7; }
int twice(int x) { return x * 2; }

} // namespace demo
