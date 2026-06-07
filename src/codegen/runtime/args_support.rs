//! 命令行参数支持运行时函数
//!
//! 提供 argc/argv 到 Cavvy String[] 的转换支持
//!
//! 注意：calloc, strlen 等 C 库函数已在 mod.rs 的 emit_header 中声明
//! Cavvy 数组布局：长度字段在数组指针前面 8 字节处
//! 布局: [长度:i32][padding:4 bytes][元素0:i8*][元素1:i8*]...
//! 返回指针指向元素0，长度在 -8 偏移处

use crate::codegen::context::IRGenerator;

impl IRGenerator {
    /// 生成命令行参数支持运行时函数
    pub(super) fn emit_args_support_runtime(&mut self) {
        // 声明额外的 C 库函数（使用签名检查避免重复声明）
        if !self.is_extern_emitted("strcpy@i8*@i8*@i8*") {
            self.emit_raw("declare i8* @strcpy(i8*, i8*)");
            self.mark_extern_emitted("strcpy@i8*@i8*@i8*".to_string());
        }
        self.emit_raw("");

        // __cay_create_string_array: 创建 String[] 数组
        // 参数: i32 %size - 数组大小
        // 返回: i8** - 数组对象指针（指向数组数据，长度在 -8 偏移处）
        // Cavvy 数组布局: [长度:i32][padding:4 bytes][元素0:i8*][元素1:i8*]...
        self.emit_raw("define i8** @__cay_create_string_array(i32 %size) {");
        self.emit_raw("entry:");
        // 计算数组对象大小: 长度头(8 bytes) + 元素数组(size * 8 bytes)
        self.emit_raw("  %size_i64 = sext i32 %size to i64");
        self.emit_raw("  %elem_size = mul i64 %size_i64, 8");
        self.emit_raw("  %total_size = add i64 8, %elem_size");
        // 分配内存
        self.emit_raw("  %arr_i8 = call i8* @calloc(i64 1, i64 %total_size)");
        // 设置长度字段 (offset 0, 4 bytes)
        self.emit_raw("  %len_ptr_i32 = bitcast i8* %arr_i8 to i32*");
        self.emit_raw("  store i32 %size, i32* %len_ptr_i32");
        // 元素数据从 offset 8 开始
        self.emit_raw("  %data_start = getelementptr i8, i8* %arr_i8, i64 8");
        self.emit_raw("  %data_start_ptr = bitcast i8* %data_start to i8**");
        // 返回指向数据区域的指针（Cavvy 数组指针指向数据，长度在 -8 处）
        self.emit_raw("  ret i8** %data_start_ptr");
        self.emit_raw("}");
        self.emit_raw("");

        // __cay_cstr_to_string: 将 C 字符串 (i8*) 转换为 Cavvy String 对象
        // 参数: i8* %cstr - C 字符串指针
        // 返回: i8* - Cavvy String 对象指针（指向字符串数据的 i8*）
        // Cavvy String 内部表示为 C 字符串（以 null 结尾的 i8*）
        self.emit_raw("define i8* @__cay_cstr_to_string(i8* %cstr) {");
        self.emit_raw("entry:");
        // 空指针检查
        self.emit_raw("  %is_null = icmp eq i8* %cstr, null");
        self.emit_raw("  br i1 %is_null, label %null_case, label %normal_case");
        self.emit_raw("");
        self.emit_raw("null_case:");
        self.emit_raw(
            "  ret i8* getelementptr ([1 x i8], [1 x i8]* @.cay_empty_str, i64 0, i64 0)",
        );
        self.emit_raw("");
        self.emit_raw("normal_case:");
        // 计算字符串长度
        self.emit_raw("  %len = call i64 @strlen(i8* %cstr)");
        // 分配内存：数据长度 + 1（null 终止符）
        self.emit_raw("  %data_size = add i64 %len, 1");
        self.emit_raw("  %total_size = add i64 16, %data_size");
        // 分配内存
        self.emit_raw("  %str_obj = call i8* @calloc(i64 1, i64 %total_size)");
        // 设置长度字段 (offset 0)
        self.emit_raw("  %len_i32 = trunc i64 %len to i32");
        self.emit_raw("  %len_field = bitcast i8* %str_obj to i32*");
        self.emit_raw("  store i32 %len_i32, i32* %len_field");
        // 设置数据指针 (offset 8)
        self.emit_raw("  %data_start = getelementptr i8, i8* %str_obj, i64 16");
        self.emit_raw("  %data_ptr_slot = getelementptr i8, i8* %str_obj, i64 8");
        self.emit_raw("  %data_ptr_slot_ptr = bitcast i8* %data_ptr_slot to i8**");
        self.emit_raw("  store i8* %data_start, i8** %data_ptr_slot_ptr");
        // 复制字符串数据
        self.emit_raw("  call i8* @strcpy(i8* %data_start, i8* %cstr)");
        // 返回指向数据区域的指针（Cavvy String 指针指向数据，长度在 -8 处）
        self.emit_raw("  ret i8* %data_start");
        self.emit_raw("}");
        self.emit_raw("");

        // __cay_array_set_ref: 设置数组元素（引用类型）
        // 参数: i8** %arr - 数组对象（指向数据区域）, i32 %idx - 索引, i8* %value - 值
        self.emit_raw("define void @__cay_array_set_ref(i8** %arr, i32 %idx, i8* %value) {");
        self.emit_raw("entry:");
        // 计算元素地址
        self.emit_raw("  %idx_i64 = sext i32 %idx to i64");
        self.emit_raw("  %elem_ptr = getelementptr i8*, i8** %arr, i64 %idx_i64");
        // 存储值
        self.emit_raw("  store i8* %value, i8** %elem_ptr");
        self.emit_raw("  ret void");
        self.emit_raw("}");
        self.emit_raw("");

        // __cay_array_get_ref: 获取数组元素（引用类型）
        // 参数: i8** %arr - 数组对象（指向数据区域）, i32 %idx - 索引
        // 返回: i8* - 元素值
        self.emit_raw("define i8* @__cay_array_get_ref(i8** %arr, i32 %idx) {");
        self.emit_raw("entry:");
        // 计算元素地址
        self.emit_raw("  %idx_i64 = sext i32 %idx to i64");
        self.emit_raw("  %elem_ptr = getelementptr i8*, i8** %arr, i64 %idx_i64");
        // 加载值
        self.emit_raw("  %value = load i8*, i8** %elem_ptr");
        self.emit_raw("  ret i8* %value");
        self.emit_raw("}");
        self.emit_raw("");

        // __cay_array_length: 获取数组长度
        // 参数: i8** %arr - 数组对象（指向数据区域）
        // 返回: i32 - 数组长度（长度存储在 arr - 8 处）
        // 数组布局: [长度:i32][padding:4][元素0:i8*][元素1:i8*]...
        // 返回指针指向元素0，所以长度在 -8 偏移处
        self.emit_raw("define i32 @__cay_array_length(i8** %arr) {");
        self.emit_raw("entry:");
        // 将数组指针转换为 i8*
        self.emit_raw("  %arr_i8 = bitcast i8** %arr to i8*");
        // 获取长度字段地址（arr - 8）
        self.emit_raw("  %len_ptr = getelementptr i8, i8* %arr_i8, i64 -8");
        self.emit_raw("  %len_ptr_i32 = bitcast i8* %len_ptr to i32*");
        self.emit_raw("  %len = load i32, i32* %len_ptr_i32");
        self.emit_raw("  ret i32 %len");
        self.emit_raw("}");
        self.emit_raw("");
    }
}
