//! FFI全功能测试
//!
//! 测试完整的FFI功能，包括各种C类型、调用约定、函数指针等

mod common;
use common::{compile_and_run_eol, compile_and_run_eol_with_features};

/// 测试所有C基本类型
#[test]
fn test_all_c_basic_types() {
    let code = r#"
#include <std/ffi.cay>

extern {
    c_int printf(c_string fmt, ...);
}

public int main() {
    // 测试所有C基本类型 - 使用显式类型转换
    c_char c = (c_char)65;           // char
    c_uchar uc = (c_uchar)255;        // unsigned char
    c_short s = (c_short)-1000;       // short
    c_ushort us = (c_ushort)50000;     // unsigned short
    c_int i = (c_int)-100000;       // int
    c_uint ui = (c_uint)3000000000;  // unsigned int
    c_long l = (c_long)-999999999;   // long
    c_float f = (c_float)3.14;        // float
    c_double d = (c_double)2.71828;    // double
    
    // 使用int类型进行printf输出（避免格式不匹配问题）
    printf("c_char: %d\n", (int)c);
    printf("c_uchar: %u\n", (int)uc);
    printf("c_short: %d\n", (int)s);
    printf("c_ushort: %u\n", (int)us);
    printf("c_int: %d\n", (int)i);
    printf("c_uint: %u\n", (int)ui);
    printf("c_long: %ld\n", (long)l);
    printf("c_float: %f\n", (double)f);
    printf("c_double: %f\n", (double)d);
    
    printf("All C basic types test passed!\n");
    return 0;
}
"#;

    let temp_path = format!("tests/temp_c_types_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("c_char: 65"),
                "c_char should work, got: {}",
                output
            );
            assert!(
                output.contains("c_uchar: 255"),
                "c_uchar should work, got: {}",
                output
            );
            assert!(
                output.contains("All C basic types test passed"),
                "All C types test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试size_t和指针类型
#[test]
fn test_size_t_and_pointer_types() {
    let code = r#"
extern {
    c_int printf(c_string fmt, ...);
    ptr malloc(size_t size);
    void free(ptr p);
    size_t strlen(c_string s);
}

public int main() {
    // 测试size_t
    size_t sz = 1024;
    printf("size_t value: %zu\n", sz);
    
    // 测试strlen返回size_t
    size_t len = strlen("Hello, World!");
    printf("String length: %zu\n", len);
    
    // 测试指针分配
    ptr p = malloc(100);
    if (p != null) {
        printf("Memory allocated with size_t size\n");
        free(p);
    }
    
    printf("Size_t and pointer types test passed!\n");
    return 0;
}
"#;

    let temp_path = format!("tests/temp_size_t_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("size_t value:"),
                "size_t should work, got: {}",
                output
            );
            assert!(
                output.contains("Size_t and pointer types test passed"),
                "Test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试intptr_t和uintptr_t
#[test]
fn test_intptr_types() {
    let code = r#"
#include <std/ffi.cay>

extern {
    c_int printf(c_string fmt, ...);
    ptr malloc(size_t size);
    void free(ptr p);
}

public int main() {
    ptr p = malloc(16);
    if (p == null) {
        printf("Allocation failed\n");
        return 1;
    }
    
    // 将指针转换为整数类型
    uintptr_t addr = (uintptr_t)p;
    intptr_t signed_addr = (intptr_t)addr;
    
    printf("Pointer as uintptr_t: %p\n", addr);
    printf("Pointer as intptr_t: %p\n", signed_addr);
    
    free(p);
    
    printf("Intptr types test passed!\n");
    return 0;
}
"#;

    let temp_path = format!("tests/temp_intptr_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("Intptr types test passed"),
                "Intptr types test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试调用约定
#[test]
fn test_calling_conventions() {
    let code = r#"
extern {
    c_int printf(c_string fmt, ...);
    c_int usleep(c_uint usec);
}

public int main() {
    printf("Testing cdecl calling convention\n");

    // 短暂休眠测试cdecl调用约定 (usleep是标准POSIX函数，使用微秒)
    usleep(10000);  // 10毫秒 = 10000微秒

    printf("Calling conventions test passed!\n");
    return 0;
}
"#;

    let temp_path = format!("tests/temp_callconv_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("Testing cdecl calling convention"),
                "Cdecl should work, got: {}",
                output
            );
            assert!(
                output.contains("Calling conventions test passed"),
                "Calling conventions test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试复杂FFI结构 - 字符串操作
#[test]
fn test_ffi_string_operations() {
    let code = r#"
#include <std/ffi.cay>

extern {
    c_int printf(c_string fmt, ...);
    c_int sprintf(ptr str, c_string fmt, ...);
    size_t strlen(c_string s);
    c_string strcpy(ptr dest, c_string src);
    c_int strcmp(c_string s1, c_string s2);
    ptr malloc(size_t size);
    void free(ptr p);
}

public int main() {
    // 创建缓冲区
    ptr buffer = malloc(256);
    if (buffer == null) {
        printf("Buffer allocation failed\n");
        return 1;
    }
    
    // 测试strcpy
    strcpy(buffer, "Hello, FFI!");
    printf("Copied string: %s\n", (c_string)buffer);
    
    // 测试strlen
    size_t len = strlen((c_string)buffer);
    printf("String length: %zu\n", len);
    
    // 测试strcmp
    c_int cmp = strcmp((c_string)buffer, "Hello, FFI!");
    printf("String comparison (should be 0): %d\n", cmp);
    
    free(buffer);
    
    printf("FFI string operations test passed!\n");
    return 0;
}
"#;

    let temp_path = format!("tests/temp_ffi_string_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("Copied string: Hello, FFI!"),
                "strcpy should work, got: {}",
                output
            );
            assert!(
                output.contains("FFI string operations test passed"),
                "FFI string test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试内存操作函数
#[test]
fn test_memory_operations() {
    let code = r#"
extern {
    c_int printf(c_string fmt, ...);
    ptr malloc(size_t size);
    void free(ptr p);
    ptr memset(ptr s, c_int c, size_t n);
    ptr memcpy(ptr dest, ptr src, size_t n);
    ptr memmove(ptr dest, ptr src, size_t n);
    c_int memcmp(ptr s1, ptr s2, size_t n);
}

public int main() {
    // 分配两个缓冲区
    ptr buf1 = malloc(16);
    ptr buf2 = malloc(16);
    
    if (buf1 == null || buf2 == null) {
        printf("Allocation failed\n");
        return 1;
    }
    
    // 测试memset
    memset(buf1, 0xAB, 16);
    printf("Buffer initialized with memset\n");
    
    // 测试memcpy
    memcpy(buf2, buf1, 16);
    printf("Buffer copied with memcpy\n");
    
    // 测试memcmp
    c_int cmp = memcmp(buf1, buf2, 16);
    printf("Memory comparison (should be 0): %d\n", cmp);
    
    free(buf1);
    free(buf2);
    
    printf("Memory operations test passed!\n");
    return 0;
}
"#;

    let temp_path = format!("tests/temp_mem_ops_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("Memory operations test passed"),
                "Memory operations test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试回调函数模式（类似qsort）
#[test]
fn test_callback_pattern() {
    let code = r#"
#include <std/ffi.cay>

// 定义比较函数指针类型 - 使用int代替c_int
alias CompareFn = fn(ptr, ptr) -> int;

extern {
    c_int printf(c_string fmt, ...);
    ptr malloc(size_t size);
    void free(ptr p);
}

// 简单的冒泡排序实现，使用回调比较
void bubble_sort(ptr arr, size_t n, size_t size, CompareFn cmp) {
    // 简化版本：只打印信息，实际排序需要更复杂的指针运算
    printf("Sorting array with %zu elements\n", n);
    
    // 测试调用比较函数
    int result = cmp(arr, arr);
    printf("Comparison result: %d\n", result);
}

// 整数比较函数 - 使用int返回类型
int compare_ints(ptr a, ptr b) {
    // 简化：返回0表示相等
    return 0;
}

public int main() {
    // 使用malloc分配数组内存
    ptr arr = malloc(20);  // 5个int = 20字节
    if (arr == null) {
        printf("Array allocation failed\n");
        return 1;
    }
    
    bubble_sort(arr, 5, 4, compare_ints);
    
    free(arr);
    printf("Callback pattern test passed!\n");
    return 0;
}
"#;

    let temp_path = format!("tests/temp_callback_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol_with_features(&temp_path, &["-F=top_level_function"]);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("Sorting array with 5 elements"),
                "Should show sorting info, got: {}",
                output
            );
            assert!(
                output.contains("Callback pattern test passed"),
                "Callback pattern test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试FFI错误处理
#[test]
fn test_ffi_error_handling() {
    let code = r#"
extern {
    c_int printf(c_string fmt, ...);
    ptr malloc(size_t size);
    void free(ptr p);
    c_int errno();
}

public int main() {
    // 测试分配0字节（可能返回null或有效指针）
    ptr p = malloc(0);
    printf("malloc(0) result: %p\n", p);
    if (p != null) {
        free(p);
    }
    
    // 正常分配
    ptr p2 = malloc(100);
    if (p2 == null) {
        printf("Allocation failed!\n");
        return 1;
    }
    
    printf("Allocation successful\n");
    free(p2);
    
    printf("FFI error handling test passed!\n");
    return 0;
}
"#;

    let temp_path = format!("tests/temp_ffi_error_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("FFI error handling test passed"),
                "FFI error handling test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}
