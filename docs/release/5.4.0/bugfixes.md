# Cavvy 5.4.0 Bug 修复清单

- 修复泛型类重载构造器解析，避免无关重载覆盖泛型构造器。
- 修复整数到指针的显式转换 IR，正确生成 `inttoptr`。
- 修复 `Mmap.unmap` 和 `INVALID_HANDLE` 比较相关的目标类型处理。
