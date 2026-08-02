//! File.cay 库集成测试
//!
//! 测试 File.cay 库的所有功能，包括文件读写、FileInfo、FileMode 等

mod common;
use common::compile_and_run_eol;

/// 测试文件基本读写操作
#[test]
fn test_file_basic_read_write() {
    let code = r#"
#include <std/ffi.cay>
#include <File.cay>

using std::File;
using std::FileMode;

public class FileTest {
    public static int main() {
        String path = "test_basic.txt";
        String content = "Hello, File.cay!";
        
        // 写入文件
        File writeFile = new File();
        if (!writeFile.open(path, FileMode.write())) {
            println("Failed to open file for writing");
            return 1;
        }
        if (!writeFile.writeString(content)) {
            println("Failed to write content");
            return 1;
        }
        writeFile.close();
        
        // 读取文件
        File readFile = new File();
        if (!readFile.open(path, FileMode.read())) {
            println("Failed to open file for reading");
            return 1;
        }
        String readContent = readFile.readAllText();
        readFile.close();
        
        // 验证内容
        if (readContent == null) {
            println("readAllText returned null");
            return 1;
        }
        
        // 验证长度
        if (readContent.length() != content.length()) {
            println("Content length mismatch");
            return 1;
        }
        
        // 清理
        File.delete(path);
        
        println("File basic read/write test passed!");
        return 0;
    }
}
"#;

    let temp_path = format!("tests/temp_file_basic_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("File basic read/write test passed!"),
                "Test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试 File.readAllText 静态方法
#[test]
fn test_file_read_all_text_static() {
    let code = r#"
#include <std/ffi.cay>
#include <File.cay>

using std::File;

public class FileTest {
    public static int main() {
        String path = "test_static.txt";
        String content = "Static method test content";
        
        // 使用 writeAllText 写入
        if (!File.writeAllText(path, content)) {
            println("writeAllText failed");
            return 1;
        }
        
        // 使用 readAllText 静态方法读取
        String readContent = File.readAllText(path);
        if (readContent == null) {
            println("readAllText returned null");
            return 1;
        }
        
        // 验证长度
        if (readContent.length() != content.length()) {
            println("Content length mismatch");
            return 1;
        }
        
        // 清理
        File.delete(path);
        
        println("File.readAllText static method test passed!");
        return 0;
    }
}
"#;

    let temp_path = format!("tests/temp_file_static_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("File.readAllText static method test passed!"),
                "Test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试 FileInfo 功能
#[test]
fn test_file_info() {
    let code = r#"
#include <std/ffi.cay>
#include <File.cay>

using std::File;
using std::FileInfo;

public class FileTest {
    public static int main() {
        String path = "test_info.txt";
        String content = "Test content for FileInfo";
        
        // 创建文件
        File.writeAllText(path, content);
        
        // 测试 FileInfo
        FileInfo info = FileInfo.fromPath(path);
        if (!info.exists()) {
            println("FileInfo.exists() returned false");
            return 1;
        }
        
        long size = info.getSize();
        if (size != content.length()) {
            println("FileInfo.getSize() returned wrong size");
            return 1;
        }
        
        // 测试 File.exists 和 File.getSize 静态方法
        if (!File.exists(path)) {
            println("File.exists() returned false");
            return 1;
        }
        
        long staticSize = File.getSize(path);
        if (staticSize != content.length()) {
            println("File.getSize() returned wrong size");
            return 1;
        }
        
        // 清理
        File.delete(path);
        
        println("FileInfo test passed!");
        return 0;
    }
}
"#;

    let temp_path = format!("tests/temp_file_info_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("FileInfo test passed!"),
                "Test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试 readLine 方法
#[test]
fn test_file_read_line() {
    let code = r#"
#include <std/ffi.cay>
#include <File.cay>

using std::File;
using std::FileMode;

public class FileTest {
    public static int main() {
        String path = "test_lines.txt";
        
        // 写入多行内容
        File file = new File();
        file.open(path, FileMode.write());
        file.writeLine("Line 1");
        file.writeLine("Line 2");
        file.writeLine("Line 3");
        file.close();
        
        // 逐行读取
        File readFile = new File();
        readFile.open(path, FileMode.read());
        
        String line1 = readFile.readLine(100);
        String line2 = readFile.readLine(100);
        String line3 = readFile.readLine(100);
        
        readFile.close();
        
        if (line1 == null || line2 == null || line3 == null) {
            println("readLine returned null");
            return 1;
        }
        
        // 清理
        File.delete(path);
        
        println("readLine test passed!");
        return 0;
    }
}
"#;

    let temp_path = format!("tests/temp_file_readline_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("readLine test passed!"),
                "Test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试文件追加模式
#[test]
fn test_file_append_mode() {
    let code = r#"
#include <std/ffi.cay>
#include <File.cay>

using std::File;
using std::FileMode;

public class FileTest {
    public static int main() {
        String path = "test_append.txt";
        
        // 第一次写入
        File.writeAllText(path, "First");
        
        // 追加内容
        File file = new File();
        file.open(path, FileMode.append());
        file.writeString("Second");
        file.close();
        
        // 读取验证
        String content = File.readAllText(path);
        
        // 清理
        File.delete(path);
        
        if (content == null) {
            println("readAllText returned null");
            return 1;
        }
        
        // 验证长度（FirstSecond = 11 字符）
        if (content.length() != 11) {
            println("Append content length mismatch");
            return 1;
        }
        
        println("Append mode test passed!");
        return 0;
    }
}
"#;

    let temp_path = format!("tests/temp_file_append_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("Append mode test passed!"),
                "Test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试静态方法与实例方法共存
#[test]
fn test_static_and_instance_methods() {
    let code = r#"
#include <std/ffi.cay>
#include <File.cay>

using std::File;
using std::FileMode;

public class FileTest {
    public static int main() {
        String path = "test_coexist.txt";
        String content = "Test content";
        
        // 使用静态方法写入
        File.writeAllText(path, content);
        
        // 使用实例方法读取
        File file = new File();
        file.open(path, FileMode.read());
        String content1 = file.readAllText();
        file.close();
        
        // 使用静态方法读取
        String content2 = File.readAllText(path);
        
        // 清理
        File.delete(path);
        
        if (content1 == null || content2 == null) {
            println("readAllText returned null");
            return 1;
        }
        
        // 验证长度
        if (content1.length() != content.length() || content2.length() != content.length()) {
            println("Content length mismatch");
            return 1;
        }
        
        println("Static and instance methods coexist test passed!");
        return 0;
    }
}
"#;

    let temp_path = format!("tests/temp_file_coexist_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("Static and instance methods coexist test passed!"),
                "Test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试内存映射文件 Mmap 完整功能
///
/// 覆盖只读映射、读写映射、切片视图、越界访问、错误处理、
/// 空文件映射、读写持久化以及 64KB 压力测试等分支。
#[test]
fn test_mmap_full() {
    let result = compile_and_run_eol("examples/test_mmap.cay");

    // 清理示例程序可能遗留的测试文件
    let _ = std::fs::remove_file("test_mmap_data.txt");
    let _ = std::fs::remove_file("test_mmap_rw.txt");
    let _ = std::fs::remove_file("test_mmap_invalid.txt");
    let _ = std::fs::remove_file("test_mmap_empty.txt");
    let _ = std::fs::remove_file("test_mmap_persist.txt");
    let _ = std::fs::remove_file("test_mmap_stress.txt");
    let _ = std::fs::remove_file("examples/test_mmap.exe");
    let _ = std::fs::remove_file("examples/test_mmap.ll");

    match result {
        Ok(output) => {
            assert!(
                output.contains("All Mmap tests passed!"),
                "Mmap test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Mmap test failed with error: {}", e);
        }
    }
}

/// 测试 Mmap 对空文件的支持
#[test]
fn test_mmap_empty_file() {
    let code = r#"
#include <std/ffi.cay>
#include <File.cay>
#include <Mmap.cay>

using std::File;
using std::Mmap;
using std::Result;
using std::IOError;
using std::MmapSlice;

public class MmapEmptyTest {
    public static int main() {
        String path = "test_mmap_empty.txt";
        File.writeAllText(path, "");

        Result<Mmap, IOError> result = Mmap.mapReadOnly(path);
        if (result.isErr()) {
            println("mapReadOnly on empty file failed");
            File.delete(path);
            return 1;
        }

        Mmap mmap = result.unwrap();
        if (mmap.size() != 0L) {
            println("Empty file mmap size should be 0");
            File.delete(path);
            return 1;
        }

        // 切片长度为 0 时不应越界
        MmapSlice emptySlice = mmap.slice(0L, 0L);
        if (emptySlice.size() != 0L) {
            println("Empty slice size mismatch");
            File.delete(path);
            return 1;
        }

        File.delete(path);
        println("Empty file mmap test passed!");
        return 0;
    }
}
"#;

    let temp_path = format!("tests/temp_mmap_empty_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);
    let _ = std::fs::remove_file("test_mmap_empty.txt");

    match result {
        Ok(output) => {
            assert!(
                output.contains("Empty file mmap test passed!"),
                "Test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}
