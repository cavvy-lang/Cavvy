# Cavvy 语言语法参考

本文档提供 Cavvy 编程语言的完整语法规范，基于 EBNF（扩展巴科斯-瑙尔范式）描述。

## 目录

- [词法规则](#词法规则)
- [程序结构](#程序结构)
- [类型系统](#类型系统)
- [声明与定义](#声明与定义)
- [语句](#语句)
- [表达式](#表达式)
- [预处理器](#预处理器)
- [完整 EBNF](#完整-ebnf)

---

## 词法规则

### 空白字符

```ebnf
whitespace = " " | "\t" | "\n" | "\r" | "\f";
comment    = "//", { any_character - "\n" }, "\n"
           | "/*", { any_character }, "*/";
```

### 标识符

```ebnf
identifier = letter, { letter | digit | "_" };
letter     = "a".."z" | "A".."Z" | "_";
digit      = "0".."9";
```

### 关键字

```
abstract   alias      auto       boolean    break
case       char       class      continue   default
do         double     else       enum       extends
extern     false      final      float      fn
for        if         implements instanceof int
interface  let        long       namespace  native
new        null       private    protected  public
return     scope      static     struct     super
switch     this       true       using      var
void       while      __ir
```
> `enum`、`struct`、`fn`、`namespace`、`using`、`scope`、`alias`、`__ir` 为 Cavvy 扩展关键字。

### 字面量

#### 整数字面量

```cay
// 十进制
int a = 42;
int b = 1_000_000;  // 下划线分隔

// 十六进制
int c = 0x2A;
int d = 0xFF_FF;

// 二进制
int e = 0b101010;

// 八进制
int f = 0o52;

// 长整型
long g = 42L;
long h = 0x2AL;
```

#### 浮点数字面量

```cay
// double（默认）
double a = 3.14159;
double b = 2.5e10;
double c = 1.5E-5;

// float
float d = 3.14f;
float e = 2.5e10F;
```

#### 字符字面量

```cay
char a = 'A';
char b = '\n';    // 换行
char c = '\t';    // 制表符
char d = '\\';    // 反斜杠
char e = '\'';    // 单引号
char f = '\u0041'; // Unicode (A)
```

#### 字符串字面量

```cay
String a = "Hello, World!";
String b = "Line 1\nLine 2";
String c = "Tab\there";
String d = "Quote: \"text\"";
```

#### 布尔字面量

```cay
boolean a = true;
boolean b = false;
```

#### null 字面量

```cay
String s = null;
Object obj = null;
```

---

## 程序结构

### 程序组成

```cay
// 程序由声明组成
program = { preprocessor_directive | declaration };

declaration = class_declaration
            | interface_declaration
            | top_level_function
            | extern_declaration
            | namespace_declaration;
```

### 包声明

```cay
package com.example.myapp;

public class MyClass {
    // ...
}
```

### 导入声明

```cay
// 导入单个类
import com.example.utils.StringUtils;

// 导入整个包
import com.example.utils.*;

// 静态导入
import static com.example.utils.MathUtils.*;
```

---

## 类型系统

### 基本类型

| 类型 | 大小 | 默认值 | 范围 |
|------|------|--------|------|
| `boolean` | 1 byte | `false` | `true`, `false` |
| `byte` | 1 byte | 0 | -128 ~ 127 |
| `short` | 2 bytes | 0 | -32768 ~ 32767 |
| `char` | 2 bytes | `'\u0000'` | 0 ~ 65535 |
| `int` | 4 bytes | 0 | -2^31 ~ 2^31-1 |
| `long` | 8 bytes | 0L | -2^63 ~ 2^63-1 |
| `float` | 4 bytes | 0.0f | IEEE 754 |
| `double` | 8 bytes | 0.0 | IEEE 754 |
| `void` | - | - | 无返回值 |

```cay
// 基本类型示例
boolean flag = true;
char c = 'A';
// byte 和 short 类型暂不支持直接赋值，使用 int 替代
int b = 100;
int s = 1000;
int i = 10000;
long l = 100000L;
float f = 3.14f;
double d = 3.14159;
String str = "Hello";
```

### 引用类型

```cay
// 类类型
String s;
MyClass obj;

// 数组类型
int[] arr1;
int[][] arr2;
String[] strs;

// 接口类型
Runnable r;
Comparable c;
```

### 类型修饰符

```cay
// 数组类型
int[] a;           // 一维数组
int[][] b;         // 二维数组
int[][][] c;       // 三维数组

// 可变参数类型（方法参数）
void method(int... args);
```

### 类型别名

Cavvy 支持使用 `alias` 关键字定义类型别名，简化复杂类型的使用：

```cay
// 基本类型别名
alias MyInt = int;
alias MyString = String;

// C类型别名
alias CInt = c_int;
alias CLong = c_long;
alias VoidPtr = ptr;

// 使用类型别名
MyInt count = 10;
VoidPtr data = malloc(100);
```

### 函数指针类型

Cavvy 支持函数指针类型定义，用于回调函数和动态函数调用：

```cay
// 定义函数指针类型
alias CompareFn = fn(int, int) -> int;
alias Callback = fn(ptr) -> void;
alias Predicate = fn(int) -> bool;

// 使用函数指针类型
CompareFn cmp = compare_function;
int result = cmp(5, 3);

// 函数指针作为参数
void sort(int[] arr, CompareFn comparator);

// 函数指针作为返回值
CompareFn getComparator(bool ascending);
```

函数指针类型语法：
```ebnf
function_pointer_type = "fn", "(", [ parameter_types ], ")", "->", return_type;
type_alias = "alias", identifier, "=", type, ";";
parameter_types       = type, { ",", type };
```

---

## 声明与定义

### 类声明

```cay
// 基本类
public class ClassName {
    // 类成员
}

// 继承
public class Child : Parent {
    // ...
}

// 实现接口
public class MyClass : MyInterface {
    // ...
}

// 继承并实现接口
public class Child : Parent, Interface1, Interface2 {
    // ...
}

// 泛型类（计划中）
public class Box<T> {
    private T value;
}

// 抽象类
public abstract class Shape {
    public abstract double getArea();
}

// final 类
public final class String {
    // ...
}

// 注解
@Deprecated
public class OldClass {
    // ...
}
```

### struct 声明 (v5.1.0+)

```cay
// 基本 struct — 值类型，栈分配
public struct Point {
    public int x;
    public int y;

    public int getX() { return this.x; }
    public int getY() { return this.y; }
}

// 带默认值的 struct
public struct Config {
    public int timeout = 5000;
    public String host = "localhost";
}
```

### enum 声明 (v5.1.0+)

```cay
// 简单枚举（无 payload）
public enum Color {
    Red,
    Green,
    Blue
}

// 带 payload 的 tagged union
public enum Result {
    Ok(int),
    Err(String)
}

// 泛型 enum
public enum Option<T> {
    Some(T),
    None
}
```

### @FreeFunction 注解 (v5.1.0+)

```cay
// 将类的静态方法导出为可直接调用的自由函数
public class MathUtils {
    @FreeFunction
    public static int add(int a, int b) {
        return a + b;
    }
}

// 直接调用，无需 MathUtils. 前缀
int result = add(1, 2);
```

### 泛型类型参数 (v5.1.0+)

```cay
// 声明泛型类型参数
class Box<T> {
    private T value;
}

enum Optional<T> {
    Some(T),
    None
}

// 使用泛型类型实参
Optional<int> opt = Optional<int>.of(42);
Box<String> b = new Box<String>();
```

### 接口声明

```cay
// 基本接口
public interface InterfaceName {
    void method1();
    int method2(String arg);
}

// 继承多个接口
public interface SubInterface : Base1, Base2 {
    void newMethod();
}

// 默认方法（计划中）
public interface MyInterface {
    void abstractMethod();
    
    default void defaultMethod() {
        // 默认实现
    }
}
```

### 字段声明

```cay
public class FieldExamples {
    // 实例字段
    private int instanceField;
    
    // 静态字段
    private static int staticField;
    
    // 常量
    public static final int MAX_SIZE = 100;
    
    // 多种类型
    private String name;
    private double value;
    private boolean flag;
    private int[] array;
    
    // 初始化
    private int x = 10;
    private String s = "default";
}
```

### 方法声明

```cay
public class MethodExamples {
    // 实例方法
    public void doSomething() {
        // ...
    }
    
    // 静态方法
    public static void staticMethod() {
        // ...
    }
    
    // 带参数
    public int add(int a, int b) {
        return a + b;
    }
    
    // 带多个参数
    public void configure(String name, int port, boolean enabled) {
        // ...
    }
    
    // 可变参数
    public int sum(int... numbers) {
        int total = 0;
        for (int n : numbers) {
            total += n;
        }
        return total;
    }
    
    // 抽象方法
    public abstract void abstractMethod();
    
    // final 方法
    public final void cannotOverride() {
        // ...
    }
    
    // 方法重载
    public void method(int x) { }
    public void method(String s) { }
    public void method(int x, String s) { }
}
```

### 构造函数

```cay
public class ConstructorExamples {
    private int value;
    private String name;
    
    // 默认构造函数
    public ConstructorExamples() {
        this.value = 0;
        this.name = "";
    }
    
    // 带参数构造函数
    public ConstructorExamples(int value) {
        this.value = value;
        this.name = "";
    }
    
    // 完整构造函数
    public ConstructorExamples(int value, String name) {
        this.value = value;
        this.name = name;
    }
    
    // 构造函数重载调用
    public ConstructorExamples(String name) {
        this(0, name);  // 调用另一个构造函数
    }
}
```

### 顶层函数（0.4.3+）

```cay
// 顶层函数声明
public void helper() {
    // ...
}

// 使用类静态方法模拟顶层函数（也早就支持了，这文档有问题，不过默认会报错，有关详细信息——自己编译一个顶层函数试试）
class Utils {
    public static void helper() {
        println("Helper called");
    }
    
    public static int calculate(int x, int y) {
        return x + y;
    }
}

// 程序入口
public int main() {
    Utils.helper();
    int result = Utils.calculate(1, 2);
    return 0;
}

带参数的 main（早就支持了，这文档有问题）
public int main(String[] args) {
    for (int i = 0; i < args.length; i++) {
        println(args[i]);
    }
    return 0;
}
```

### 外部函数声明

```cay
// 声明 C 函数
extern int printf(String fmt, ...);
extern long malloc(int size);
extern void free(long ptr);

// 使用
public int main() {
    printf("Hello from Cavvy!\n");
    
    long ptr = malloc(100);
    free(ptr);
    
    return 0;
}
```

---

## 语句

### 变量声明语句

```cay
public int main() {
    // 基本声明
    int x;
    int y = 10;
    
    // 多个变量
    int a, b, c;
    int d = 1, e = 2, f = 3;
    
    // 现代语法
    var x1: int = 10;
    let y1: String = "hello";
    auto z = 3.14;  // double
    
    // final 变量
    final int MAX = 100;
    final var MIN: int = 0;
    
    // 数组声明
    int[] arr;
    int[] arr2 = new int[10];
    int[] arr3 = {1, 2, 3};
    
    // 多变量声明（一行多个语句）
    int a = 1; int b = 2; int c = 3;
    
    return 0;
}
```

### 表达式语句

```cay
public void expressionStatements() {
    // 赋值
    x = 10;
    
    // 自增/自减
    i++;
    --j;
    
    // 方法调用
    doSomething();
    
    // 对象创建
    new MyClass();
    
    // 复合赋值
    x += 5;
    y *= 2;
}
```

### if 语句

```cay
public void ifStatement(int x) {
    // 基本 if
    if (x > 0) {
        println("Positive");
    }
    
    // if-else
    if (x > 0) {
        println("Positive");
    } else {
        println("Non-positive");
    }
    
    // if-else-if
    if (x > 0) {
        println("Positive");
    } else if (x < 0) {
        println("Negative");
    } else {
        println("Zero");
    }
    
    // 嵌套 if
    if (x > 0) {
        if (x > 10) {
            println("Large positive");
        }
    }
}
```

### switch 语句

```cay
public void switchStatement(int day) {
    switch (day) {
        case 1:
            println("Monday");
            break;
        case 2:
            println("Tuesday");
            break;
        case 3:
            println("Wednesday");
            break;
        case 4:
        case 5:
            println("Thursday or Friday");
            break;
        default:
            println("Weekend");
            break;
    }
}
```

### for 语句

```cay
public void forStatement() {
    // 基本 for
    for (int i = 0; i < 10; i++) {
        println(i);
    }
    
    // 多个初始化
    for (int i = 0, j = 10; i < j; i++, j--) {
        println("i=" + i + ", j=" + j);
    }
    
    // 无限循环
    for (;;) {
        // 无限循环
        break;
    }
    
    // 省略部分
    int i = 0;
    for (; i < 10;) {
        i++;
    }
}
```

### while 语句

```cay
public void whileStatement() {
    // 基本 while
    int i = 0;
    while (i < 10) {
        println(i);
        i++;
    }
    
    // 条件始终为 true
    while (true) {
        // 无限循环
        break;
    }
}
```

### do-while 语句

```cay
public void doWhileStatement() {
    int i = 0;
    do {
        println(i);
        i++;
    } while (i < 10);
}
```

### 增强 for 语句

```cay
public void enhancedFor() {
    int[] numbers = {1, 2, 3, 4, 5};
    
    // 遍历数组
    for (int n : numbers) {
        println(n);
    }
    
    // 遍历字符串数组
    String[] names = {"Alice", "Bob", "Charlie"};
    for (String name : names) {
        println(name);
    }
}
```

### break 语句

```cay
public void breakStatement() {
    // 跳出循环
    for (int i = 0; i < 10; i++) {
        if (i == 5) {
            break;
        }
        println(i);
    }
    
    // 带标签的 break
    outer: for (int i = 0; i < 3; i++) {
        for (int j = 0; j < 3; j++) {
            if (i == 1 && j == 1) {
                break outer;
            }
            println("i=" + i + ", j=" + j);
        }
    }
}
```

### continue 语句

```cay
public void continueStatement() {
    // 跳过当前迭代
    for (int i = 0; i < 10; i++) {
        if (i % 2 == 0) {
            continue;
        }
        println(i);  // 只打印奇数
    }
    
    // 带标签的 continue
    outer: for (int i = 0; i < 3; i++) {
        for (int j = 0; j < 3; j++) {
            if (j == 1) {
                continue outer;
            }
            println("i=" + i + ", j=" + j);
        }
    }
}
```

### return 语句

```cay
public int returnValue() {
    return 42;
}

public void returnVoid() {
    return;  // 可选
}

public String conditionalReturn(boolean flag) {
    if (flag) {
        return "Yes";
    } else {
        return "No";
    }
}
```

### 同步语句（计划中）

```cay
public void synchronizedStatement() {
    Object lock = new Object();
    
    synchronized (lock) {
        // 临界区
    }
}
```

### try-catch-finally 语句（计划中）

```cay
public void exceptionHandling() {
    try {
        riskyOperation();
    } catch (IOException e) {
        println("IO error: " + e.message);
    } catch (Exception e) {
        println("Error: " + e.message);
    } finally {
        cleanup();
    }
}
```

---

## 表达式

### 基本表达式

```cay
// 字面量
42          // 整数
3.14        // 浮点数
"hello"     // 字符串
'x'         // 字符
true        // 布尔值
null        // null

// 标识符
variableName
this
super
```

### 算术表达式

```cay
// 二元运算
a + b
a - b
a * b
a / b
a % b

// 一元运算
-a
+a

// 复合赋值
a += b
a -= b
a *= b
a /= b
a %= b
```

### 关系表达式

```cay
a == b
a != b
a < b
a > b
a <= b
a >= b
```

### 逻辑表达式

```cay
// 逻辑与
a && b

// 逻辑或
a || b

// 逻辑非
!a
```

### 位运算表达式

```cay
// 位与
a & b

// 位或
a | b

// 位异或
a ^ b

// 位取反
~a

// 位移
a << 2
a >> 2
a >>> 2  // 无符号右移（计划中）

// 复合位赋值
a &= b
a |= b
a ^= b
a <<= 2
a >>= 2
```

### 条件表达式

```cay
// 三元运算符
int max = (a > b) ? a : b;

// 嵌套
String result = (x > 0) ? "positive" : (x < 0) ? "negative" : "zero";
```

### 自增自减表达式

```cay
// 前置
++i
--j

// 后置
i++
j--
```

### 类型表达式

```cay
// 类型转换
(int)3.14
(double)42
(String)obj

// instanceof
if (obj instanceof String) {
    // ...
}
```

### 数组表达式

```cay
// 数组创建
new int[10]
new int[][] {{1, 2}, {3, 4}}
new String[] {"a", "b", "c"}

// 数组访问
arr[0]
matrix[i][j]

// 数组长度
arr.length
```

### 方法调用表达式

```cay
// 实例方法调用
obj.method()
obj.method(arg1, arg2)

// 静态方法调用
ClassName.staticMethod()

// 链式调用
obj.getName().toUpperCase()

// 方法引用（计划中）
String::length
obj::method
```

### 对象创建表达式

```cay
// 基本对象创建
new ClassName()
new ClassName(arg1, arg2)

// 匿名类（计划中）
new Interface() {
    @Override
    public void method() {
        // ...
    }
};
```

### 字段访问表达式

```cay
// 实例字段
obj.field

// 静态字段
ClassName.staticField

// this 字段
this.field

// super 字段
super.field
```

### Lambda 表达式

```cay
// 无参数
() -> { }

// 单参数
x -> x * 2

// 多参数
(a, b) -> a + b

// 带类型
(int x, int y) -> x + y

// 多语句
(x, y) -> {
    int sum = x + y;
    return sum;
}
```

### 赋值表达式

```cay
// 简单赋值
x = 10

// 复合赋值
x += 5
x -= 3
x *= 2
x /= 2
x %= 3
x &= 0xFF
x |= 0x10
x ^= 0xFF
x <<= 2
x >>= 2
```

---

## 预处理器

### 文件包含

```cay
// 系统库
#include <Network.cay>
#include <StringBuilder.cay>

// 用户库
#include "mylib.cay"
#include "utils/helper.cay"
```

### 宏定义

```cay
// 对象宏
#define DEBUG
#define VERSION "1.0.0"
#define MAX_SIZE 100

// 使用宏
#ifdef DEBUG
    println("Debug mode");
#endif
```

### 条件编译

```cay
#ifdef MACRO
    // 如果 MACRO 已定义
#endif

#ifndef MACRO
    // 如果 MACRO 未定义
#endif

#if EXPRESSION
    // 如果表达式为真
#elif ANOTHER_EXPRESSION
    // 否则如果
#else
    // 否则
#endif
```

### 防止重复包含

```cay
// MyLib.cay
#ifndef MYLIB_CAY
#define MYLIB_CAY

// 库内容...

#endif  // MYLIB_CAY
```

### 平台检测

```cay
#ifdef _WIN32
    // Windows 代码
#elif defined(__linux__)
    // Linux 代码
#elif defined(__APPLE__)
    // macOS 代码
#endif
```

---

## 完整 EBNF

```ebnf
(* ============================================================================
 * Cavvy 编程语言完整语法规范 - EBNF
 * 版本: 0.5.1.0
 * ============================================================================ *)

(* ------------------------------------------------------------------------
 * 词法元素
 * ------------------------------------------------------------------------ *)

identifier = letter, { letter | digit | "_" };
letter     = "a".."z" | "A".."Z" | "_";
digit      = "0".."9";

integer_literal = decimal_literal | hex_literal | binary_literal | octal_literal;
decimal_literal = digit, { digit | "_" }, [ "L" | "l" ];
hex_literal     = "0x" | "0X", hex_digit, { hex_digit | "_" }, [ "L" | "l" ];
binary_literal  = "0b" | "0B", binary_digit, { binary_digit | "_" }, [ "L" | "l" ];
octal_literal   = "0o" | "0O", octal_digit, { octal_digit | "_" }, [ "L" | "l" ];
hex_digit       = digit | "a".."f" | "A".."F";
binary_digit    = "0" | "1";
octal_digit     = "0".."7";

floating_literal = decimal_float | hex_float;
decimal_float    = digit, { digit }, ".", { digit }, [ exponent ], [ "f" | "F" | "d" | "D" ];
hex_float        = "0x", hex_digit, { hex_digit }, ".", { hex_digit }, [ binary_exponent ];
exponent         = ( "e" | "E" ), [ "+" | "-" ], digit, { digit };
binary_exponent  = ( "p" | "P" ), [ "+" | "-" ], digit, { digit };

char_literal   = "'", ( char_char | escape_sequence ), "'";
string_literal = """, { string_char | escape_sequence }, """;
char_char      = any_character - ( "'" | "\"" | "\\" | control_char );
string_char    = any_character - ( """ | "\\" | control_char );
escape_sequence = "\\", ( "n" | "t" | "r" | "\\" | "'" | """ | "b" | "f" 
                         | "u", hex_digit, hex_digit, hex_digit, hex_digit );

boolean_literal = "true" | "false";
null_literal    = "null";

(* ------------------------------------------------------------------------
 * 程序结构
 * ------------------------------------------------------------------------ *)

program = { preprocessor_directive | declaration };

declaration = class_declaration
            | interface_declaration
            | top_level_function
            | extern_declaration
            | namespace_declaration
            | package_declaration
            | import_declaration;

package_declaration = "package", qualified_name, ";";

import_declaration = "import", [ "static" ], qualified_name, [ ".", "*" ], ";";

qualified_name = identifier, { ".", identifier };

(* ------------------------------------------------------------------------
 * 预处理器
 * ------------------------------------------------------------------------ *)

preprocessor_directive = define_directive
                       | ifdef_directive
                       | ifndef_directive
                       | if_directive
                       | elif_directive
                       | else_directive
                       | endif_directive
                       | include_directive;

define_directive   = "#define", identifier, [ replacement_text ];
ifdef_directive    = "#ifdef", identifier;
ifndef_directive   = "#ifndef", identifier;
if_directive       = "#if", constant_expression;
elif_directive     = "#elif", constant_expression;
else_directive     = "#else";
endif_directive    = "#endif";
include_directive  = "#include", ( "<", header_name, ">" | """, header_name, """ );

replacement_text = { any_character - newline };
header_name      = { any_character - ( ">" | """ ) };

(* ------------------------------------------------------------------------
 * 类型
 * ------------------------------------------------------------------------ *)

type = primitive_type | reference_type;

primitive_type = "boolean" | "byte" | "short" | "char" | "int" | "long" 
               | "float" | "double" | "void";

reference_type = class_or_interface_type | array_type;

class_or_interface_type = qualified_name;

array_type = type, "[", "]", { "[", "]" };

(* ------------------------------------------------------------------------
 * 类声明
 * ------------------------------------------------------------------------ *)

class_declaration = [ annotation ], [ modifiers ], "class", identifier,
                    [ inheritance_clause ], [ implementation_clause ],
                    "{", { class_member }, "}";

modifiers = modifier, { modifier };
modifier  = "public" | "private" | "protected" | "static" | "abstract" | "final"
          | "native" | "synchronized" | "transient" | "volatile" | "strictfp";

inheritance_clause = "extends", qualified_name | ":", qualified_name;

implementation_clause = "implements", qualified_name, { ",", qualified_name };

annotation = "@", identifier, [ "(", [ annotation_arguments ], ")" ];
annotation_arguments = annotation_argument, { ",", annotation_argument };
annotation_argument = identifier, "=", expression | expression;

class_member = field_declaration
             | method_declaration
             | constructor_declaration
             | static_initializer
             | instance_initializer
             | nested_class_declaration
             | nested_interface_declaration;

(* ------------------------------------------------------------------------
 * 字段声明
 * ------------------------------------------------------------------------ *)

field_declaration = [ annotation ], [ modifiers ], type, variable_declarators, ";";

variable_declarators = variable_declarator, { ",", variable_declarator };
variable_declarator = identifier, [ "=", variable_initializer ];
variable_initializer = expression | array_initializer;

array_initializer = "{", [ variable_initializer, { ",", variable_initializer } ], "}";

(* ------------------------------------------------------------------------
 * 方法声明
 * ------------------------------------------------------------------------ *)

method_declaration = [ annotation ], [ modifiers ], ( type | "void" ), identifier,
                     "(", [ formal_parameters ], ")", [ throws_clause ],
                     ( block | ";" );

formal_parameters = formal_parameter, { ",", formal_parameter };
formal_parameter = [ annotation ], [ "final" ], type, [ "..." ], identifier;

throws_clause = "throws", qualified_name, { ",", qualified_name };

(* ------------------------------------------------------------------------
 * 构造函数声明
 * ------------------------------------------------------------------------ *)

constructor_declaration = [ annotation ], [ modifiers ], identifier,
                          "(", [ formal_parameters ], ")", [ throws_clause ],
                          block;

(* ------------------------------------------------------------------------
 * 初始化块
 * ------------------------------------------------------------------------ *)

static_initializer = "static", block;
instance_initializer = block;

(* ------------------------------------------------------------------------
 * 接口声明
 * ------------------------------------------------------------------------ *)

interface_declaration = [ annotation ], [ modifiers ], "interface", identifier,
                        [ interface_extends_clause ],
                        "{", { interface_member }, "}";

interface_extends_clause = "extends", qualified_name, { ",", qualified_name };

interface_member = interface_method_declaration;

interface_method_declaration = [ annotation ], [ modifiers ], ( type | "void" ),
                              identifier, "(", [ formal_parameters ], ")", ";";

(* ------------------------------------------------------------------------
 * 顶层函数
 * ------------------------------------------------------------------------ *)

top_level_function = [ annotation ], [ modifiers ], ( type | "void" ), "main",
                     "(", [ formal_parameters ], ")", block;

(* ------------------------------------------------------------------------
 * 外部函数声明
 * ------------------------------------------------------------------------ *)

extern_declaration = "extern", [ modifiers ], ( type | "void" ), identifier,
                     "(", [ extern_parameters ], ")", ";";

extern_parameters = extern_parameter, { ",", extern_parameter };
extern_parameter = type, [ "..." ], identifier;

(* ------------------------------------------------------------------------
 * 命名空间声明
 * ------------------------------------------------------------------------ *)

namespace_declaration = "namespace", identifier, "{", { declaration }, "}";

(* ------------------------------------------------------------------------
 * 语句
 * ------------------------------------------------------------------------ *)

statement = block
          | variable_declaration_statement
          | expression_statement
          | if_statement
          | switch_statement
          | for_statement
          | while_statement
          | do_statement
          | break_statement
          | continue_statement
          | return_statement
          | synchronized_statement
          | try_statement
          | throw_statement
          | labeled_statement
          | empty_statement;

block = "{", { statement }, "}";

variable_declaration_statement = [ "final" ], ( var_declaration | auto_declaration | type_declaration ), ";";

var_declaration = "var", identifier, ":", type, [ "=", expression ];
auto_declaration = "auto", identifier, "=", expression;
type_declaration = type, variable_declarators;

expression_statement = expression, ";";

if_statement = "if", "(", expression, ")", statement, [ "else", statement ];

switch_statement = "switch", "(", expression, ")", "{", { switch_block }, "}";
switch_block = switch_label, { statement };
switch_label = "case", expression, ":" | "default", ":";

for_statement = "for", "(", for_init, ";", [ expression ], ";", for_update, ")", statement
              | "for", "(", type, identifier, ":", expression, ")", statement;
for_init = [ "final" ], type, variable_declarators | expression, { ",", expression };
for_update = expression, { ",", expression };

while_statement = "while", "(", expression, ")", statement;

do_statement = "do", statement, "while", "(", expression, ")", ";";

break_statement = "break", [ identifier ], ";";

continue_statement = "continue", [ identifier ], ";";

return_statement = "return", [ expression ], ";";

synchronized_statement = "synchronized", "(", expression, ")", block;

try_statement = "try", block, { catch_clause }, [ finally_clause ];
catch_clause = "catch", "(", [ "final" ], type, [ "|", type ], identifier, ")", block;
finally_clause = "finally", block;

throw_statement = "throw", expression, ";";

labeled_statement = identifier, ":", statement;

empty_statement = ";";

(* ------------------------------------------------------------------------
 * 表达式
 * ------------------------------------------------------------------------ *)

expression = assignment_expression;

assignment_expression = conditional_expression, [ assignment_operator, assignment_expression ];

assignment_operator = "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=";

conditional_expression = or_expression, [ "?", expression, ":", conditional_expression ];

or_expression = and_expression, { "||", and_expression };
and_expression = bitwise_or_expression, { "&&", bitwise_or_expression };
bitwise_or_expression = bitwise_xor_expression, { "|", bitwise_xor_expression };
bitwise_xor_expression = bitwise_and_expression, { "^", bitwise_and_expression };
bitwise_and_expression = equality_expression, { "&", equality_expression };

equality_expression = relational_expression, { ( "==" | "!=" ), relational_expression };

relational_expression = shift_expression, { ( "<" | ">" | "<=" | ">=" | "instanceof" ), shift_expression };

shift_expression = additive_expression, { ( "<<" | ">>" | ">>>" ), additive_expression };

additive_expression = multiplicative_expression, { ( "+" | "-" ), multiplicative_expression };

multiplicative_expression = unary_expression, { ( "*" | "/" | "%" ), unary_expression };

unary_expression = ( "+" | "-" | "!" | "~" | "++" | "--" ), unary_expression
                 | postfix_expression
                 | cast_expression;

cast_expression = "(", type, ")", unary_expression;

postfix_expression = primary_expression, { postfix_op };
postfix_op = "[", expression, "]"
           | ".", identifier
           | "(", [ argument_list ], ")"
           | "++"
           | "--";

primary_expression = literal
                   | identifier
                   | "this"
                   | "super"
                   | "(", expression, ")"
                   | class_instance_creation
                   | array_creation
                   | lambda_expression;

literal = integer_literal | floating_literal | char_literal | string_literal 
        | boolean_literal | null_literal;

class_instance_creation = "new", [ qualified_name, "." ], identifier, "(", [ argument_list ], ")";

array_creation = "new", type, "[", expression, "]", { "[", [ expression ], "]" };

argument_list = expression, { ",", expression };

lambda_expression = lambda_parameters, "->", lambda_body;
lambda_parameters = identifier
                  | "(", [ formal_parameters ], ")";
lambda_body = expression | block;

(* ------------------------------------------------------------------------
 * 常量表达式（用于预处理器）
 * ------------------------------------------------------------------------ *)

constant_expression = conditional_expression;
```

---

## 运算符优先级

| 优先级 | 运算符 | 结合性 | 描述 |
|--------|--------|--------|------|
| 1 | `()` `[]` `.` | 左 | 括号、数组访问、成员访问 |
| 2 | `++` `--` | 右 | 后置自增/自减 |
| 3 | `++` `--` `+` `-` `!` `~` `(type)` | 右 | 前置自增/自减、一元运算、类型转换 |
| 4 | `*` `/` `%` | 左 | 乘、除、取模 |
| 5 | `+` `-` | 左 | 加、减 |
| 6 | `<<` `>>` `>>>` | 左 | 位移 |
| 7 | `<` `<=` `>` `>=` `instanceof` | 左 | 关系运算 |
| 8 | `==` `!=` | 左 | 相等运算 |
| 9 | `&` | 左 | 按位与 |
| 10 | `^` | 左 | 按位异或 |
| 11 | `\|` | 左 | 按位或 |
| 12 | `&&` | 左 | 逻辑与 |
| 13 | `\|\|` | 左 | 逻辑或 |
| 14 | `?:` | 右 | 条件运算 |
| 15 | `=` `+=` `-=` `*=` `/=` `%=` `&=` `^=` `\|=` `<<=` `>>=` | 右 | 赋值运算 |

---

## 相关文档

- [快速开始](quickstart.md) - 5 分钟上手 Cavvy
- [语言文档](language-guide.md) - 深入了解 Cavvy 语言特性
- [FFI 使用指南](ffi-guide.md) - 调用 C 函数和系统库
