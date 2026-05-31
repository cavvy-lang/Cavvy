# 接口使用指南

## 概述

接口（Interface）是 Cavvy 语言中定义行为契约的机制。类可以通过 `implements` 关键字实现接口，承诺提供接口中定义的所有方法的具体实现。

## 基础语法

### 接口声明

```cay
interface Greetable {
    void greet();
}
```

### 类实现接口

```cay
class Person implements Greetable {
    String name;
    
    public Person(String name) {
        this.name = name;
    }
    
    public void greet() {
        println("Hello, I am " + this.name);
    }
}
```

### 接口类型使用

```cay
Greetable g = new Person("Alice");
g.greet();  // 输出: Hello, I am Alice
```

## 多接口实现

一个类可以同时实现多个接口：

```cay
interface Printable {
    void print();
}

interface Drawable {
    void draw();
}

class Widget implements Printable, Drawable {
    public void print() {
        println("Printing widget");
    }
    
    public void draw() {
        println("Drawing widget");
    }
}
```

## 接口作为参数

接口类型可以作为方法参数，实现多态：

```cay
class Greeter {
    public static void greetPerson(Greetable g) {
        g.greet();
    }
}

// 使用
Greeter.greetPerson(new Person("Alice"));
Greeter.greetPerson(new Person("Bob"));
```

## 接口与继承组合

接口可以与类继承组合使用：

```cay
interface Flyable {
    void fly();
}

class Animal {
    String name;
    
    public Animal(String name) {
        this.name = name;
    }
    
    public void speak() {
        println(this.name + " speaks");
    }
}

class Bird extends Animal implements Flyable {
    public Bird(String name) {
        super(name);
    }
    
    public void fly() {
        println(this.name + " flies");
    }
}

// 使用
Bird b = new Bird("Eagle");
b.speak();  // 输出: Eagle speaks
b.fly();    // 输出: Eagle flies

Animal a = b;
a.speak();  // 输出: Eagle speaks

Flyable f = b;
f.fly();    // 输出: Eagle flies
```

## 接口方法带参数

接口方法可以带参数和返回值：

```cay
interface Calculable {
    int calculate(int a, int b);
}

class Adder implements Calculable {
    public int calculate(int a, int b) {
        return a + b;
    }
}

class Multiplier implements Calculable {
    public int calculate(int a, int b) {
        return a * b;
    }
}

// 使用
Calculable calc = new Adder();
println(calc.calculate(3, 4));  // 输出: 7

calc = new Multiplier();
println(calc.calculate(3, 4));  // 输出: 12
```

## 空接口（标记接口）

接口可以没有方法定义，作为标记使用：

```cay
interface Serializable {
}

class User implements Serializable {
    String name;
    
    public User(String name) {
        this.name = name;
    }
}
```

## 已知限制

### 1. 接口方法动态分发

**当前限制**：通过接口类型调用方法时，使用声明类型（接口名）解析方法，而非运行时类型。

```cay
interface Animal {
    void speak();
}

class Dog implements Animal {
    public void speak() {
        println("Woof");
    }
}

class Cat implements Animal {
    public void speak() {
        println("Meow");
    }
}

// 当前行为
Animal a1 = new Dog();
Animal a2 = new Cat();
a1.speak();  // 输出: Meow（使用第一个实现类的方法）
a2.speak();  // 输出: Meow（使用第一个实现类的方法）
```

**解决方案**：后续版本将通过 vtable 支持动态分发。

### 2. 接口常量

接口中不能定义常量字段（与 Java 不同）。

### 3. 接口静态方法

接口中不能定义静态方法。

## 最佳实践

1. **接口命名**：使用形容词或以 `-able`、`-ible` 结尾的名称（如 `Printable`、`Drawable`）。

2. **单一职责**：每个接口应该只定义一个相关的功能集。

3. **接口隔离**：避免创建包含过多方法的"胖接口"。

4. **面向接口编程**：优先使用接口类型而非具体类类型。

```cay
// 好：面向接口编程
void process(Greetable g) {
    g.greet();
}

// 避免：依赖具体类
void process(Person p) {
    p.greet();
}
```

## 示例

### 示例 1：形状计算

```cay
interface Shape {
    double area();
    double perimeter();
}

class Circle implements Shape {
    double radius;
    
    public Circle(double radius) {
        this.radius = radius;
    }
    
    public double area() {
        return 3.14159 * radius * radius;
    }
    
    public double perimeter() {
        return 2 * 3.14159 * radius;
    }
}

class Rectangle implements Shape {
    double width;
    double height;
    
    public Rectangle(double width, double height) {
        this.width = width;
        this.height = height;
    }
    
    public double area() {
        return width * height;
    }
    
    public double perimeter() {
        return 2 * (width + height);
    }
}

// 使用
Shape s1 = new Circle(5.0);
Shape s2 = new Rectangle(4.0, 6.0);

println("Circle area: " + String.valueOf(s1.area()));
println("Rectangle area: " + String.valueOf(s2.area()));
```

### 示例 2：排序策略

```cay
interface SortStrategy {
    void sort(int[] arr);
}

class BubbleSort implements SortStrategy {
    public void sort(int[] arr) {
        // 冒泡排序实现
    }
}

class QuickSort implements SortStrategy {
    public void sort(int[] arr) {
        // 快速排序实现
    }
}

class Sorter {
    public static void doSort(int[] arr, SortStrategy strategy) {
        strategy.sort(arr);
    }
}

// 使用
int[] data = {5, 3, 8, 1, 2};
Sorter.doSort(data, new BubbleSort());
Sorter.doSort(data, new QuickSort());
```

## 相关文档

- [语言指南](language-guide.md)
- [语法参考](syntax-reference.md)
- [类型系统](type-system.md)
