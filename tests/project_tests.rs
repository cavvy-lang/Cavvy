//! Cavvy 项目集成测试 - 20个多文件Cavvy项目
//!
//! 每个项目包含多个文件，通过 #include 集成编译

mod common;
use common::compile_and_run_eol;
use std::fs;
use std::process::Command;

/// 确保项目测试目录存在
fn ensure_project_dir(name: &str) -> String {
    let dir = format!("fuzz_tests/projects/{}", name);
    let _ = fs::create_dir_all(&dir);
    dir
}

/// 编译并运行项目的主入口文件
fn run_project(name: &str, entry: &str) -> Result<String, String> {
    let path = format!("fuzz_tests/projects/{}/{}", name, entry);
    compile_and_run_eol(&path)
}

// ============================================================
// Project 01: CalculatorApp - 基本计算器
// ============================================================
#[test]
fn project_calculator() {
    let dir = ensure_project_dir("calculator");
    fs::write(format!("{}/math_ops.cay", dir), r#"
public class MathOps {
    public static int add(int a, int b) { return a + b; }
    public static int sub(int a, int b) { return a - b; }
    public static int mul(int a, int b) { return a * b; }
    public static int div(int a, int b) { return a / b; }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "math_ops.cay"
public int main() {
    println(MathOps.add(10, 20));
    println(MathOps.sub(50, 30));
    println(MathOps.mul(7, 6));
    println(MathOps.div(100, 4));
    return 0;
}
"#).ok();
    let output = run_project("calculator", "main.cay").unwrap();
    assert!(output.contains("30"));
    assert!(output.contains("20"));
    assert!(output.contains("42"));
    assert!(output.contains("25"));
}

// ============================================================
// Project 02: StudentManager - 学生管理系统
// ============================================================
#[test]
fn project_student_manager() {
    let dir = ensure_project_dir("student");
    fs::write(format!("{}/student_model.cay", dir), r#"
public class Student {
    public int id;
    public String name;
    public int score;
    public Student(int i, String n, int s) {
        this.id = i;
        this.name = n;
        this.score = s;
    }
    public String getGrade() {
        if (this.score >= 90) { return "A"; }
        else if (this.score >= 80) { return "B"; }
        else if (this.score >= 70) { return "C"; }
        else { return "D"; }
    }
}
"#).ok();
    fs::write(format!("{}/student_utils.cay", dir), r#"
#include "student_model.cay"
public class StudentUtils {
    public static int averageScore(Student[] students) {
        int sum = 0;
        for (int i = 0; i < students.length; i = i + 1) {
            sum = sum + students[i].score;
        }
        return sum / students.length;
    }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "student_utils.cay"
public int main() {
    Student[] students = new Student[3];
    students[0] = new Student(1, "Alice", 95);
    students[1] = new Student(2, "Bob", 82);
    students[2] = new Student(3, "Charlie", 67);
    println(students[0].name + " grade: " + students[0].getGrade());
    println(students[1].name + " grade: " + students[1].getGrade());
    println(students[2].name + " grade: " + students[2].getGrade());
    println(StudentUtils.averageScore(students));
    return 0;
}
"#).ok();
    let output = run_project("student", "main.cay").unwrap();
    assert!(output.contains("Alice grade: A"));
    assert!(output.contains("Bob grade: B"));
    assert!(output.contains("Charlie grade: D"));
    assert!(output.contains("81"));
}

// ============================================================
// Project 03: MatrixOps - 矩阵运算库
// ============================================================
#[test]
fn project_matrix_ops() {
    let dir = ensure_project_dir("matrix");
    fs::write(format!("{}/matrix_lib.cay", dir), r#"
public class MatrixLib {
    public static void fill(int[][] m) {
        for (int i = 0; i < m.length; i = i + 1) {
            for (int j = 0; j < m[i].length; j = j + 1) {
                m[i][j] = i * m[i].length + j + 1;
            }
        }
    }
    public static int sum(int[][] m) {
        int s = 0;
        for (int i = 0; i < m.length; i = i + 1) {
            for (int j = 0; j < m[i].length; j = j + 1) {
                s = s + m[i][j];
            }
        }
        return s;
    }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "matrix_lib.cay"
public int main() {
    int[][] m = new int[3][4];
    MatrixLib.fill(m);
    int total = MatrixLib.sum(m);
    println(m[0][0]);
    println(m[2][3]);
    println(total);
    return 0;
}
"#).ok();
    let output = run_project("matrix", "main.cay").unwrap();
    assert!(output.contains("1"));
    assert!(output.contains("12"));
    assert!(output.contains("78"));
}

// ============================================================
// Project 04: StringFormatter - 字符串格式化
// ============================================================
#[test]
fn project_string_formatter() {
    let dir = ensure_project_dir("formatter");
    fs::write(format!("{}/fmt_utils.cay", dir), r#"
public class FmtUtils {
    public static String padLeft(String s, int len, char pad) {
        String result = s;
        while (result.length() < len) {
            result = pad + result;
        }
        return result;
    }
    public static String repeat(String s, int times) {
        String result = "";
        for (int i = 0; i < times; i = i + 1) {
            result = result + s;
        }
        return result;
    }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "fmt_utils.cay"
public int main() {
    println(FmtUtils.padLeft("42", 5, '0'));
    println(FmtUtils.repeat("ha", 3));
    println(FmtUtils.padLeft("123", 8, ' '));
    return 0;
}
"#).ok();
    let output = run_project("formatter", "main.cay").unwrap();
    assert!(output.contains("00042"));
    assert!(output.contains("hahaha"));
    assert!(output.contains("     123"));
}

// ============================================================
// Project 05: NumberTheory - 数论工具
// ============================================================
#[test]
fn project_number_theory() {
    let dir = ensure_project_dir("numtheory");
    fs::write(format!("{}/prime.cay", dir), r#"
public class PrimeUtils {
    public static boolean isPrime(int n) {
        if (n < 2) { return false; }
        for (int i = 2; i * i <= n; i = i + 1) {
            if (n % i == 0) { return false; }
        }
        return true;
    }
    public static int gcd(int a, int b) {
        while (b != 0) {
            int t = b;
            b = a % b;
            a = t;
        }
        return a;
    }
    public static int lcm(int a, int b) {
        return (a / gcd(a, b)) * b;
    }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "prime.cay"
public int main() {
    println(PrimeUtils.isPrime(17));
    println(PrimeUtils.isPrime(100));
    println(PrimeUtils.gcd(48, 18));
    println(PrimeUtils.lcm(12, 18));
    return 0;
}
"#).ok();
    let output = run_project("numtheory", "main.cay").unwrap();
    assert!(output.contains("true"));
    assert!(output.contains("false"));
    assert!(output.contains("6"));
    assert!(output.contains("36"));
}

// ============================================================
// Project 06: SortBench - 排序算法集合
// ============================================================
#[test]
fn project_sort_bench() {
    let dir = ensure_project_dir("sort");
    fs::write(format!("{}/sorting.cay", dir), r#"
public class Sorting {
    public static void bubbleSort(int[] arr) {
        int n = arr.length;
        for (int i = 0; i < n; i = i + 1) {
            for (int j = 0; j < n - i - 1; j = j + 1) {
                if (arr[j] > arr[j + 1]) {
                    int t = arr[j];
                    arr[j] = arr[j + 1];
                    arr[j + 1] = t;
                }
            }
        }
    }
    public static int sumArray(int[] arr) {
        int s = 0;
        for (int i = 0; i < arr.length; i = i + 1) { s = s + arr[i]; }
        return s;
    }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "sorting.cay"
public int main() {
    int[] arr = {5, 3, 8, 1, 9, 2};
    println(Sorting.sumArray(arr));
    Sorting.bubbleSort(arr);
    println(arr[0]);
    println(arr[5]);
    println(Sorting.sumArray(arr));
    return 0;
}
"#).ok();
    let output = run_project("sort", "main.cay").unwrap();
    assert!(output.contains("28"));
    assert!(output.contains("1"));
    assert!(output.contains("9"));
}

// ============================================================
// Project 07: GeometryLib - 几何计算
// ============================================================
#[test]
fn project_geometry() {
    let dir = ensure_project_dir("geometry");
    fs::write(format!("{}/geo_lib.cay", dir), r#"
public class GeoLib {
    public static final double PI = 3.14159;
    public static double circleArea(double r) { return PI * r * r; }
    public static int rectArea(int w, int h) { return w * h; }
    public static double triangleArea(double b, double h) { return 0.5 * b * h; }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "geo_lib.cay"
public int main() {
    println(GeoLib.circleArea(10.0));
    println(GeoLib.rectArea(5, 8));
    println(GeoLib.triangleArea(6.0, 4.0));
    return 0;
}
"#).ok();
    let output = run_project("geometry", "main.cay").unwrap();
    assert!(output.contains("314.159"));
    assert!(output.contains("40"));
    assert!(output.contains("12.0"));
}

// ============================================================
// Project 08: GameOfLife - Conway生命游戏
// ============================================================
#[test]
fn project_game_of_life() {
    let dir = ensure_project_dir("gol");
    fs::write(format!("{}/gol_lib.cay", dir), r#"
public class GoLLib {
    public static int countNeighbors(int[][] grid, int x, int y) {
        int count = 0;
        for (int dx = -1; dx <= 1; dx = dx + 1) {
            for (int dy = -1; dy <= 1; dy = dy + 1) {
                if (dx != 0 || dy != 0) {
                    int nx = x + dx;
                    int ny = y + dy;
                    if (nx >= 0 && nx < grid.length && ny >= 0 && ny < grid[0].length) {
                        count = count + grid[nx][ny];
                    }
                }
            }
        }
        return count;
    }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "gol_lib.cay"
public int main() {
    int[][] g = {{0,1,0},{0,0,1},{1,1,1}};
    println(GoLLib.countNeighbors(g, 1, 1));
    println(GoLLib.countNeighbors(g, 0, 0));
    println(GoLLib.countNeighbors(g, 2, 2));
    return 0;
}
"#).ok();
    let output = run_project("gol", "main.cay").unwrap();
    assert!(output.contains("5"));
    assert!(output.contains("1"));
    assert!(output.contains("1"));
}

// ============================================================
// Project 09: BankSystem - 银行账户系统
// ============================================================
#[test]
fn project_bank_system() {
    let dir = ensure_project_dir("bank");
    fs::write(format!("{}/account.cay", dir), r#"
public class Account {
    public int id;
    public String owner;
    public int balance;
    public Account(int id, String owner, int initial) {
        this.id = id;
        this.owner = owner;
        this.balance = initial;
    }
    public void deposit(int amount) { this.balance = this.balance + amount; }
    public void withdraw(int amount) { this.balance = this.balance - amount; }
    public int getBalance() { return this.balance; }
}
"#).ok();
    fs::write(format!("{}/bank_service.cay", dir), r#"
#include "account.cay"
public class BankService {
    public static void transfer(Account from, Account to, int amount) {
        from.withdraw(amount);
        to.deposit(amount);
    }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "bank_service.cay"
public int main() {
    Account a1 = new Account(1, "Alice", 1000);
    Account a2 = new Account(2, "Bob", 500);
    println(a1.getBalance());
    println(a2.getBalance());
    BankService.transfer(a1, a2, 300);
    println(a1.getBalance());
    println(a2.getBalance());
    return 0;
}
"#).ok();
    let output = run_project("bank", "main.cay").unwrap();
    assert!(output.contains("1000"));
    assert!(output.contains("500"));
    assert!(output.contains("700"));
    assert!(output.contains("800"));
}

// ============================================================
// Project 10: FileDB - 文件数据库模拟
// ============================================================
#[test]
fn project_file_db() {
    let dir = ensure_project_dir("filedb");
    fs::write(format!("{}/record.cay", dir), r#"
public class Record {
    public int key;
    public String value;
    public Record(int k, String v) { this.key = k; this.value = v; }
}
"#).ok();
    fs::write(format!("{}/db_ops.cay", dir), r#"
#include "record.cay"
public class DBOps {
    public static String findValue(Record[] db, int key) {
        for (int i = 0; i < db.length; i = i + 1) {
            if (db[i].key == key) { return db[i].value; }
        }
        return "NOT_FOUND";
    }
    public static int countByPrefix(Record[] db, String prefix) {
        int count = 0;
        for (int i = 0; i < db.length; i = i + 1) {
            if (db[i].value.startsWith(prefix)) { count = count + 1; }
        }
        return count;
    }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "db_ops.cay"
public int main() {
    Record[] db = new Record[4];
    db[0] = new Record(1, "apple");
    db[1] = new Record(2, "banana");
    db[2] = new Record(3, "apricot");
    db[3] = new Record(4, "cherry");
    println(DBOps.findValue(db, 2));
    println(DBOps.findValue(db, 99));
    println(DBOps.countByPrefix(db, "ap"));
    return 0;
}
"#).ok();
    let output = run_project("filedb", "main.cay").unwrap();
    assert!(output.contains("banana"));
    assert!(output.contains("NOT_FOUND"));
    assert!(output.contains("2"));
}

// ============================================================
// Project 11: StatsEngine - 统计引擎
// ============================================================
#[test]
fn project_stats_engine() {
    let dir = ensure_project_dir("stats");
    fs::write(format!("{}/stats_lib.cay", dir), r#"
public class StatsLib {
    public static int max(int[] arr) {
        int m = arr[0];
        for (int i = 1; i < arr.length; i = i + 1) { if (arr[i] > m) m = arr[i]; }
        return m;
    }
    public static int min(int[] arr) {
        int m = arr[0];
        for (int i = 1; i < arr.length; i = i + 1) { if (arr[i] < m) m = arr[i]; }
        return m;
    }
    public static double average(int[] arr) {
        int s = 0;
        for (int i = 0; i < arr.length; i = i + 1) { s = s + arr[i]; }
        return (double)s / arr.length;
    }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "stats_lib.cay"
public int main() {
    int[] data = {45, 78, 23, 91, 56, 12, 67, 89};
    println(StatsLib.max(data));
    println(StatsLib.min(data));
    println(StatsLib.average(data));
    return 0;
}
"#).ok();
    let output = run_project("stats", "main.cay").unwrap();
    assert!(output.contains("91"));
    assert!(output.contains("12"));
}

// ============================================================
// Project 12: TodoList - 待办事项
// ============================================================
#[test]
fn project_todo_list() {
    let dir = ensure_project_dir("todo");
    fs::write(format!("{}/task.cay", dir), r#"
public class Task {
    public int id;
    public String desc;
    public boolean done;
    public Task(int i, String d) { this.id = i; this.desc = d; this.done = false; }
    public void markDone() { this.done = true; }
}
"#).ok();
    fs::write(format!("{}/todo_manager.cay", dir), r#"
#include "task.cay"
public class TodoManager {
    public static int countDone(Task[] tasks) {
        int c = 0;
        for (int i = 0; i < tasks.length; i = i + 1) { if (tasks[i].done) c = c + 1; }
        return c;
    }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "todo_manager.cay"
public int main() {
    Task[] tasks = new Task[4];
    tasks[0] = new Task(1, "Buy milk");
    tasks[1] = new Task(2, "Call mom");
    tasks[2] = new Task(3, "Write code");
    tasks[3] = new Task(4, "Exercise");
    tasks[0].markDone();
    tasks[2].markDone();
    println(TodoManager.countDone(tasks));
    println(tasks[2].done);
    println(tasks[3].done);
    return 0;
}
"#).ok();
    let output = run_project("todo", "main.cay").unwrap();
    assert!(output.contains("2"));
    assert!(output.contains("true"));
    assert!(output.contains("false"));
}

// ============================================================
// Project 13: TextAnalyzer - 文本分析器
// ============================================================
#[test]
fn project_text_analyzer() {
    let dir = ensure_project_dir("textalyze");
    fs::write(format!("{}/text_utils.cay", dir), r#"
public class TextUtils {
    public static int countWords(String text) {
        int count = 0;
        boolean inWord = false;
        int i = 0;
        while (i < text.length()) {
            char c = text.charAt(i);
            if (c != ' ') {
                if (!inWord) { count = count + 1; inWord = true; }
            } else {
                inWord = false;
            }
            i = i + 1;
        }
        return count;
    }
    public static int countChar(String text, char target) {
        int count = 0;
        int i = 0;
        while (i < text.length()) {
            if (text.charAt(i) == target) count = count + 1;
            i = i + 1;
        }
        return count;
    }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "text_utils.cay"
public int main() {
    String msg = "hello world from cavvy";
    println(TextUtils.countWords(msg));
    println(TextUtils.countChar(msg, 'l'));
    return 0;
}
"#).ok();
    let output = run_project("textalyze", "main.cay").unwrap();
    assert!(output.contains("4"));
    assert!(output.contains("3"));
}

// ============================================================
// Project 14: ChessUtils - 国际象棋工具
// ============================================================
#[test]
fn project_chess_utils() {
    let dir = ensure_project_dir("chess");
    fs::write(format!("{}/chess_lib.cay", dir), r#"
public class ChessLib {
    public static boolean isValidPos(int row, int col) {
        return row >= 0 && row < 8 && col >= 0 && col < 8;
    }
    public static int rookMoves(int row, int col) {
        int moves = 0;
        if (row > 0) moves = moves + 1;
        if (row < 7) moves = moves + 1;
        if (col > 0) moves = moves + 1;
        if (col < 7) moves = moves + 1;
        return moves;
    }
    public static int kingMoves(int row, int col) {
        int moves = 0;
        for (int dr = -1; dr <= 1; dr = dr + 1) {
            for (int dc = -1; dc <= 1; dc = dc + 1) {
                if (dr != 0 || dc != 0) {
                    if (isValidPos(row + dr, col + dc)) moves = moves + 1;
                }
            }
        }
        return moves;
    }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "chess_lib.cay"
public int main() {
    println(ChessLib.rookMoves(0, 0));
    println(ChessLib.rookMoves(3, 4));
    println(ChessLib.kingMoves(0, 0));
    println(ChessLib.kingMoves(4, 4));
    return 0;
}
"#).ok();
    let output = run_project("chess", "main.cay").unwrap();
    assert!(output.contains("2"));
    assert!(output.contains("4"));
    assert!(output.contains("3"));
    assert!(output.contains("8"));
}

// ============================================================
// Project 15: ECommerce - 电商购物车
// ============================================================
#[test]
fn project_ecommerce() {
    let dir = ensure_project_dir("ecommerce");
    fs::write(format!("{}/cart_item.cay", dir), r#"
public class CartItem {
    public String name;
    public int price;
    public int qty;
    public CartItem(String n, int p, int q) { this.name = n; this.price = p; this.qty = q; }
    public int subtotal() { return this.price * this.qty; }
}
"#).ok();
    fs::write(format!("{}/cart.cay", dir), r#"
#include "cart_item.cay"
public class Cart {
    public static int total(CartItem[] items) {
        int t = 0;
        for (int i = 0; i < items.length; i = i + 1) { t = t + items[i].subtotal(); }
        return t;
    }
    public static int itemCount(CartItem[] items) {
        int c = 0;
        for (int i = 0; i < items.length; i = i + 1) { c = c + items[i].qty; }
        return c;
    }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "cart.cay"
public int main() {
    CartItem[] items = new CartItem[3];
    items[0] = new CartItem("Book", 25, 2);
    items[1] = new CartItem("Pen", 3, 10);
    items[2] = new CartItem("Notebook", 8, 5);
    println(Cart.total(items));
    println(Cart.itemCount(items));
    return 0;
}
"#).ok();
    let output = run_project("ecommerce", "main.cay").unwrap();
    assert!(output.contains("120"));
    assert!(output.contains("17"));
}

// ============================================================
// Project 16: CryptoUtils - 加密工具
// ============================================================
#[test]
fn project_crypto_utils() {
    let dir = ensure_project_dir("crypto");
    fs::write(format!("{}/cipher.cay", dir), r#"
public class Cipher {
    public static String caesarEncrypt(String s, int shift) {
        String result = "";
        for (int i = 0; i < s.length(); i = i + 1) {
            char c = s.charAt(i);
            if (c >= 'A' && c <= 'Z') {
                c = (char)(((int)(c - 'A') + shift) % 26 + (int)'A');
            }
            result = result + c;
        }
        return result;
    }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "cipher.cay"
public int main() {
    println(Cipher.caesarEncrypt("HELLO", 3));
    println(Cipher.caesarEncrypt("ABC", 1));
    return 0;
}
"#).ok();
    let output = run_project("crypto", "main.cay").unwrap();
    assert!(output.contains("KHOOR"));
    assert!(output.contains("BCD"));
}

// ============================================================
// Project 17: TariffCalc - 税率计算器
// ============================================================
#[test]
fn project_tariff_calc() {
    let dir = ensure_project_dir("tariff");
    fs::write(format!("{}/tax.cay", dir), r#"
public class TaxCalc {
    public static int calcTax(int income) {
        if (income <= 10000) return 0;
        else if (income <= 50000) return (income - 10000) * 10 / 100;
        else return 4000 + (income - 50000) * 20 / 100;
    }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "tax.cay"
public int main() {
    println(TaxCalc.calcTax(5000));
    println(TaxCalc.calcTax(30000));
    println(TaxCalc.calcTax(100000));
    return 0;
}
"#).ok();
    let output = run_project("tariff", "main.cay").unwrap();
    assert!(output.contains("0"));
    assert!(output.contains("2000"));
    assert!(output.contains("14000"));
}

// ============================================================
// Project 18: GraphAlgo - 图算法
// ============================================================
#[test]
fn project_graph_algo() {
    let dir = ensure_project_dir("graph");
    fs::write(format!("{}/graph_lib.cay", dir), r#"
public class GraphLib {
    public static int pathSum(int[] values, int[][] edges) {
        int sum = 0;
        for (int i = 0; i < values.length; i = i + 1) { sum = sum + values[i]; }
        return sum;
    }
    public static int degreeCount(int[][] edges, int node) {
        int count = 0;
        for (int i = 0; i < edges.length; i = i + 1) {
            if (edges[i][0] == node || edges[i][1] == node) count = count + 1;
        }
        return count;
    }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "graph_lib.cay"
public int main() {
    int[] values = {10, 20, 30, 40};
    int[][] edges = {{0,1},{1,2},{2,3},{0,2}};
    println(GraphLib.pathSum(values, edges));
    println(GraphLib.degreeCount(edges, 1));
    println(GraphLib.degreeCount(edges, 2));
    return 0;
}
"#).ok();
    let output = run_project("graph", "main.cay").unwrap();
    assert!(output.contains("100"));
    assert!(output.contains("2"));
    assert!(output.contains("3"));
}

// ============================================================
// Project 19: WeatherStation - 气象站
// ============================================================
#[test]
fn project_weather_station() {
    let dir = ensure_project_dir("weather");
    fs::write(format!("{}/weather_lib.cay", dir), r#"
public class WeatherLib {
    public static double tempSpread(double[] temps) {
        double max = temps[0];
        double min = temps[0];
        for (int i = 1; i < temps.length; i = i + 1) {
            if (temps[i] > max) max = temps[i];
            if (temps[i] < min) min = temps[i];
        }
        return max - min;
    }
    public static double tempAvg(double[] temps) {
        double sum = 0.0;
        for (int i = 0; i < temps.length; i = i + 1) { sum = sum + temps[i]; }
        return sum / temps.length;
    }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "weather_lib.cay"
public int main() {
    double[] temps = {23.5, 25.0, 21.0, 28.5, 26.0};
    println(WeatherLib.tempSpread(temps));
    println(WeatherLib.tempAvg(temps));
    return 0;
}
"#).ok();
    let output = run_project("weather", "main.cay").unwrap();
    assert!(output.contains("7.5"));
    assert!(output.contains("24.8"));
}

// ============================================================
// Project 20: CompilerMini - 微型编译器前端
// ============================================================
#[test]
fn project_compiler_mini() {
    let dir = ensure_project_dir("compmini");
    fs::write(format!("{}/tokenizer.cay", dir), r#"
public class Tokenizer {
    public static int countTokens(String line) {
        int count = 0;
        boolean inToken = false;
        for (int i = 0; i < line.length(); i = i + 1) {
            char c = line.charAt(i);
            if (c != ' ' && c != '\t') {
                if (!inToken) { count = count + 1; inToken = true; }
            } else {
                inToken = false;
            }
        }
        return count;
    }
    public static boolean hasOperator(String line, char op) {
        for (int i = 0; i < line.length(); i = i + 1) {
            if (line.charAt(i) == op) return true;
        }
        return false;
    }
}
"#).ok();
    fs::write(format!("{}/main.cay", dir), r#"
#include "tokenizer.cay"
public int main() {
    println(Tokenizer.countTokens("int x = 42;"));
    println(Tokenizer.countTokens("x = x + 1;"));
    println(Tokenizer.hasOperator("int a = 5 + 3;", '+'));
    println(Tokenizer.hasOperator("println(x);", '-'));
    return 0;
}
"#).ok();
    let output = run_project("compmini", "main.cay").unwrap();
    assert!(output.contains("4"));
    assert!(output.contains("5"));
    assert!(output.contains("true"));
    assert!(output.contains("false"));
}
