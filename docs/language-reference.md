# 语法参考

## 顶层声明

支持的顶层声明包括：

- `class`
- `struct`
- `enum`
- `interface`
- `extern`
- `namespace`
- `using`
- `alias`
- 受 feature gate 控制的顶层函数

## 类型

| 类型 | 说明 |
|---|---|
| `void` | 无返回值 |
| `int` | 32 位整数 |
| `long` | 64 位整数 |
| `float` | 32 位浮点数 |
| `double` | 64 位浮点数 |
| `bool` / `boolean` | 布尔值 |
| `String` / `string` | 字符串 |
| `char` | 字符 |
| `T[]` | 数组 |
| `T*` | FFI 指针 |
| `fn(T) -> R` | 函数指针类型 |

FFI 类型包括 `c_int`、`c_uint`、`c_long`、`c_ulong`、`c_short`、`c_ushort`、`c_char`、`c_uchar`、`c_float`、`c_double`、`size_t`、`ssize_t`、`uintptr_t`、`intptr_t`、`c_void`、`c_bool`、`c_string`。

## 修饰符与注解

| 语法 | 用途 |
|---|---|
| `public` | 公共声明 |
| `private` | 私有声明，语义分析会阻止跨类访问 |
| `protected` | 受保护声明 |
| `static` | 静态成员 |
| `final` | 禁止重写或继承 |
| `abstract` | 抽象类或方法 |
| `native` | native 方法声明 |
| `@main` | 主类标记 |
| `@Override` | 重写标记 |
| `@Test` | 测试方法标记 |
| `@FreeFunction` | 将类静态方法导出为自由函数 |

## Namespace

```cay
namespace tools {
    public class Name {
        public static String value() {
            return "Cavvy";
        }
    }
}

using tools::Name;

public class Main {
    public static void main() {
        println(Name.value());
    }
}
```

`using namespace` 和通配符导入不受支持。使用单名导入，例如 `using tools::Name;`。

## 泛型

```cay
public class Box<T> {
    private T value;

    public Box(T value) {
        this.value = value;
    }

    public T get() {
        return this.value;
    }
}

public class GenericsDemo {
    public static void main() {
        Box<int> n = new Box<int>(42);
        println(String.valueOf(n.get()));
    }
}
```

泛型类支持一个或多个类型参数，也可以把泛型类型作为字段、参数和返回值使用。

## Lambda

```cay run
public class LambdaDemo {
    public static fn(int) -> int makeAdder(int x) {
        return (int y) -> x + y;
    }

    public static void main() {
        var add5 = makeAdder(5);
        println(add5(10));
    }
}
```

Lambda 支持表达式体、块体、作为参数传递、作为返回值返回，以及捕获外层变量。

## 接口

```cay run
interface Greeter {
    void greet();
}

class Person implements Greeter {
    private String name;

    public Person(String name) {
        this.name = name;
    }

    public void greet() {
        println("Hi, " + this.name);
    }
}

public class InterfaceDemo {
    public static void main() {
        Greeter g = new Person("Cavvy");
        g.greet();
    }
}
```

接口可作为变量类型、参数类型和返回值类型使用。通过接口类型调用方法时，编译器会使用对象运行时 vtable 分派到实际实现类；多个实现类共享同一个接口类型调用入口时，也会按运行时类型选择方法实现。已验证的覆盖范围见[实现状态](current-status.md)。
