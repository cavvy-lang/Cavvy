# 语言总览

Cavvy 采用 Java/C# 风格的面向对象语法，同时保留 C 风格预处理器和 FFI。

## 类与方法

```cay
public class Counter {
    private int current;

    public Counter(int start) {
        this.current = start;
    }

    public void add(int value) {
        this.current = this.current + value;
    }

    public int value() {
        return this.current;
    }
}

public class App {
    public static void main() {
        Counter c = new Counter(2);
        c.add(3);
        println(String.valueOf(c.value()));
    }
}
```

## 变量与控制流

```cay
public class Flow {
    public static int grade(int score) {
        if (score >= 90) {
            return 1;
        } else if (score >= 60) {
            return 2;
        }
        return 3;
    }

    public static void main() {
        int total = 0;
        for (int i = 0; i < 5; i = i + 1) {
            total = total + i;
        }

        int g = grade(total * 10);
        switch (g) {
            case 1: println("A"); break;
            case 2: println("B"); break;
            default: println("C"); break;
        }
    }
}
```

## 数组

```cay run
public class Arrays {
    public static void main() {
        int[] values = {1, 2, 3};

        println("len = " + String.valueOf(values.length));
        println("last = " + String.valueOf(values[2]));
    }
}
```

## Lambda 与泛型

```cay run
public class Box<T> {
    private T value;

    public Box(T value) {
        this.value = value;
    }

    public T get() {
        return this.value;
    }
}

public class ModernFeatures {
    public static fn(int) -> int makeAdder(int base) {
        return (int value) -> base + value;
    }

    public static void main() {
        Box<int> box = new Box<int>(7);
        var addBox = makeAdder(box.get());
        println(String.valueOf(addBox(5)));
    }
}
```

## Struct 与 Enum

```cay
public struct Point {
    public int x;
    public int y;

    public int sum() {
        return x + y;
    }
}

public enum Status {
    Ready,
    Done
}

public class DataDemo {
    public static void main() {
        Point p = new Point();
        p.x = 2;
        p.y = 5;

        Status status = Status.Done;
        switch (status) {
            case Status.Done: println(String.valueOf(p.sum())); break;
            default: println("waiting"); break;
        }
    }
}
```
