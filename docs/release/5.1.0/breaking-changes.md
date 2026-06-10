# Cavvy 5.1.0 破坏性变更

本文档列出从上一版本升级到 5.1.0 时需要注意的兼容性变更。

---

## 1. 隐式类型转换已移除

**变更**: 不再允许 `string` 和 `int` 等类型之间的隐式转换，必须使用显式转换。

**影响**: 以前可以自动转换的代码现在会产生编译错误。

**迁移方式**:

```cay ignore
// 旧代码（不再支持）
int x = 42;
String s = x;  // 隐式转换，之前可能通过

// 新代码（必须显式转换）
int x = 42;
String s = String.valueOf(x);  // 显式转换
// 或
String s = x.toString();
```

**相关 Commit**: `87c71260` - "修复类型打印需手动调用 String.valueOf 的问题，统一自动类型转换"

---

## 2. 标准库引入命名空间

**变更**: 标准库由于添加了 namespace，现在需要通过 `using` 别名显式引入，不再支持全局回退查找。

**影响**: 以前可以直接使用 `File`、`String` 等标准库类型的代码，现在需要显式引入。

**迁移方式**:

```cay ignore
// 旧代码（不再支持）
File f = new File("test.txt");

// 新代码（使用 using 别名）
using File = std::File;
File f = new File("test.txt");
```

**重要限制**:
- 不支持 `using namespace std;` 这种批量引入语法
- 必须对每个使用的类型单独声明 `using std::XXX;`

**相关 Commit**:
- `619baad9` - "支持 using 别名查找类型，移除全局回退查找"
- `b18a00f7` - "重构类型系统，支持命名空间和泛型类型匹配"

---

## 迁移检查清单

升级代码时，请逐项检查：

- [ ] 所有 `int` 到 `String` 的转换已改为显式（`String.valueOf()` 或 `.toString()`）
- [ ] 所有 `String` 到 `int` 的转换已改为显式（`Integer.parseInt()` 等）
- [ ] 所有标准库类型（`File`、`StringBuilder`、`Network` 等）已通过 `using std::XXX` 引入
- [ ] 未使用 `using namespace std;`（该语法不受支持）
