//! FFI void*指针专门测试
//!
//! 重点测试void*指针的各种使用场景

mod common;
use common::compile_and_run_eol;

/// 测试基本的void*分配和释放
#[test]
fn test_void_ptr_basic() {
    let code = r#"
extern {
    c_int printf(c_string fmt, ...);
    ptr malloc(size_t size);
    void free(ptr p);
}

public int main() {
    // void* 可以指向任何类型
    ptr data = malloc(32);
    
    if (data == null) {
        printf("Failed to allocate void*\n");
        return 1;
    }
    
    printf("void* allocated at %p\n", data);
    
    free(data);
    printf("void* freed successfully\n");
    
    return 0;
}
"#;

    let temp_path = format!("tests/temp_void_basic_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("void* allocated"),
                "Should allocate void*, got: {}",
                output
            );
            assert!(
                output.contains("void* freed successfully"),
                "Should free void*, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试void*与memset/memcpy
#[test]
fn test_void_ptr_memset_memcpy() {
    let code = r#"
extern {
    c_int printf(c_string fmt, ...);
    ptr malloc(size_t size);
    void free(ptr p);
    ptr memset(ptr s, c_int c, size_t n);
    ptr memcpy(ptr dest, ptr src, size_t n);
}

public int main() {
    ptr buf1 = malloc(16);
    ptr buf2 = malloc(16);
    
    if (buf1 == null || buf2 == null) {
        printf("Allocation failed\n");
        return 1;
    }
    
    // 使用memset初始化void*
    memset(buf1, 0x42, 16);
    printf("Buffer 1 initialized with 0x42\n");
    
    // 使用memcpy复制void*
    memcpy(buf2, buf1, 16);
    printf("Buffer 2 copied from buffer 1\n");
    
    free(buf1);
    free(buf2);
    
    printf("void* memset/memcpy test passed!\n");
    return 0;
}
"#;

    let temp_path = format!("tests/temp_void_mem_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("void* memset/memcpy test passed"),
                "void* memset/memcpy test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试void*作为函数参数和返回值
#[test]
fn test_void_ptr_as_param_and_return() {
    let code = r#"
extern {
    c_int printf(c_string fmt, ...);
    ptr malloc(size_t size);
    void free(ptr p);
}

class BufferManager {
    // 返回void*的函数
    public static ptr allocate_buffer(size_t size) {
        return malloc(size);
    }

    // 接受void*参数的函数
    public static void process_buffer(ptr buffer, size_t size) {
        if (buffer != null) {
            printf("Processing buffer at %p, size %zu\n", buffer, size);
        }
    }
}

public int main() {
    ptr buf = BufferManager.allocate_buffer(64);

    if (buf == null) {
        printf("Allocation failed\n");
        return 1;
    }

    BufferManager.process_buffer(buf, 64);

    free(buf);
    printf("void* as param and return test passed!\n");
    return 0;
}
"#;

    let temp_path = format!("tests/temp_void_param_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("Processing buffer"),
                "Should process buffer, got: {}",
                output
            );
            assert!(
                output.contains("void* as param and return test passed"),
                "Test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试void*类型别名
#[test]
fn test_void_ptr_type_alias() {
    let code = r#"
// 定义void*类型别名
alias VoidPtr = ptr;
alias ConstVoidPtr = ptr;  // 简化：Cavvy不区分const

extern {
    c_int printf(c_string fmt, ...);
    VoidPtr malloc(size_t size);
    void free(VoidPtr p);
}

public int main() {
    // 使用类型别名
    VoidPtr data = malloc(32);
    
    if (data == null) {
        printf("Allocation failed\n");
        return 1;
    }
    
    printf("Allocated using VoidPtr alias: %p\n", data);
    
    free(data);
    printf("void* type alias test passed!\n");
    return 0;
}
"#;

    let temp_path = format!("tests/temp_void_alias_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("Allocated using VoidPtr alias"),
                "VoidPtr alias should work, got: {}",
                output
            );
            assert!(
                output.contains("void* type alias test passed"),
                "Test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试void*与函数指针结合
#[test]
fn test_void_ptr_with_callback() {
    let code = r#"
// 定义使用void*的回调类型 - 支持参数名
alias VisitorFn = fn(data: ptr, user_data: ptr) -> void;

extern {
    c_int printf(c_string fmt, ...);
    ptr malloc(size_t size);
    void free(ptr p);
}

class ArrayIterator {
    // 模拟遍历函数
    public static void foreach_element(ptr arr, size_t count, VisitorFn visitor, ptr user_data) {
        printf("Iterating %zu elements\n", count);
        // 简化：只调用一次visitor
        visitor(arr, user_data);
    }
}

class ElementPrinter {
    // 访问者函数
    public static void print_element(ptr data, ptr user_data) {
        printf("Visiting element at %p, user data at %p\n", data, user_data);
    }
}

public int main() {
    ptr arr = malloc(16);
    ptr user_data = malloc(8);

    if (arr == null || user_data == null) {
        printf("Allocation failed\n");
        return 1;
    }

    ArrayIterator.foreach_element(arr, 4, ElementPrinter.print_element, user_data);

    free(arr);
    free(user_data);

    printf("void* with callback test passed!\n");
    return 0;
}
"#;

    let temp_path = format!("tests/temp_void_callback_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("Iterating 4 elements"),
                "Should iterate elements, got: {}",
                output
            );
            assert!(
                output.contains("void* with callback test passed"),
                "Test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试void*数组
#[test]
fn test_void_ptr_array() {
    let code = r#"
extern {
    c_int printf(c_string fmt, ...);
    ptr malloc(size_t size);
    void free(ptr p);
}

public int main() {
    // 分配void*数组
    ptr arr = malloc(8 * 4);  // 4个指针的空间
    
    if (arr == null) {
        printf("Allocation failed\n");
        return 1;
    }
    
    printf("void* array allocated at %p\n", arr);
    
    // 分配一些数据块并存储在数组中
    for (int i = 0; i < 4; i = i + 1) {
        ptr data = malloc(16);
        printf("Data %d allocated at %p\n", i, data);
        free(data);
    }
    
    free(arr);
    printf("void* array test passed!\n");
    return 0;
}
"#;

    let temp_path = format!("tests/temp_void_array_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("void* array allocated"),
                "void* array should be allocated, got: {}",
                output
            );
            assert!(
                output.contains("void* array test passed"),
                "Test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试void*的null检查
#[test]
fn test_void_ptr_null_check() {
    let code = r#"
extern {
    c_int printf(c_string fmt, ...);
    ptr malloc(size_t size);
    void free(ptr p);
}

public int main() {
    ptr p1 = null;
    ptr p2 = malloc(16);
    
    // 测试null检查
    if (p1 == null) {
        printf("p1 is null (correct)\n");
    }
    
    if (p2 != null) {
        printf("p2 is not null (correct)\n");
    }
    
    // 测试null比较
    if (p1 == p2) {
        printf("p1 == p2 (unexpected)\n");
    } else {
        printf("p1 != p2 (correct)\n");
    }
    
    free(p2);
    
    printf("void* null check test passed!\n");
    return 0;
}
"#;

    let temp_path = format!("tests/temp_void_null_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("p1 is null (correct)"),
                "Should detect p1 as null, got: {}",
                output
            );
            assert!(
                output.contains("p2 is not null (correct)"),
                "Should detect p2 as not null, got: {}",
                output
            );
            assert!(
                output.contains("void* null check test passed"),
                "Test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

/// 测试realloc风格的void*操作
#[test]
fn test_void_ptr_realloc_pattern() {
    let code = r#"
extern {
    c_int printf(c_string fmt, ...);
    ptr malloc(size_t size);
    ptr realloc(ptr p, size_t size);
    void free(ptr p);
}

public int main() {
    // 初始分配
    ptr buf = malloc(16);
    if (buf == null) {
        printf("Initial allocation failed\n");
        return 1;
    }
    printf("Initial buffer: %p (size 16)\n", buf);
    
    // 重新分配
    ptr new_buf = realloc(buf, 32);
    if (new_buf == null) {
        printf("Realloc failed\n");
        free(buf);
        return 1;
    }
    printf("Resized buffer: %p (size 32)\n", new_buf);
    
    free(new_buf);
    printf("void* realloc pattern test passed!\n");
    return 0;
}
"#;

    let temp_path = format!("tests/temp_void_realloc_{}.cay", std::process::id());
    std::fs::write(&temp_path, code).expect("Failed to write temp file");

    let result = compile_and_run_eol(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    match result {
        Ok(output) => {
            assert!(
                output.contains("Initial buffer:"),
                "Should show initial buffer, got: {}",
                output
            );
            assert!(
                output.contains("void* realloc pattern test passed"),
                "Test should pass, got: {}",
                output
            );
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}
