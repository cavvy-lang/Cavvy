//! IR生成上下文和状态管理
use std::collections::HashMap;

/// 循环上下文，用于支持 break/continue
#[derive(Debug, Clone)]
pub struct LoopContext {
    pub cond_label: String,  // continue 跳转的目标（条件检查）
    pub end_label: String,   // break 跳转的目标（循环结束）
}

/// IR生成器核心上下文
pub struct IRGenerator {
    pub output: String,
    pub indent: usize,
    pub label_counter: usize,
    pub temp_counter: usize,
    pub global_strings: HashMap<String, String>,
    pub global_counter: usize,
    pub current_function: String,
    pub current_class: String,
    pub current_return_type: String,   // 当前函数的返回类型
    pub var_types: HashMap<String, String>,
    pub var_class_map: HashMap<String, String>,
    pub loop_stack: Vec<LoopContext>,  // 循环上下文栈
}

impl IRGenerator {
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
            label_counter: 0,
            temp_counter: 0,
            global_strings: HashMap::new(),
            global_counter: 0,
            current_function: String::new(),
            current_class: String::new(),
            current_return_type: String::new(),
            var_types: HashMap::new(),
            var_class_map: HashMap::new(),
            loop_stack: Vec::new(),
        }
    }

    /// 发射一行代码
    pub fn emit_line(&mut self, line: &str) {
        if !line.is_empty() {
            self.output.push_str(&"  ".repeat(self.indent));
        }
        self.output.push_str(line);
        self.output.push('\n');
    }

    /// 发射代码但不添加缩进（用于全局声明）
    pub fn emit_raw(&mut self, line: &str) {
        self.output.push_str(line);
        self.output.push('\n');
    }

    /// 创建新标签
    pub fn new_label(&mut self, prefix: &str) -> String {
        let label = format!("{}.{}", prefix, self.label_counter);
        self.label_counter += 1;
        label
    }

    /// 创建新的临时变量
    pub fn new_temp(&mut self) -> String {
        let temp = format!("%t{}", self.temp_counter);
        self.temp_counter += 1;
        temp
    }

    /// 进入循环上下文
    pub fn enter_loop(&mut self, cond_label: String, end_label: String) {
        self.loop_stack.push(LoopContext { cond_label, end_label });
    }

    /// 退出循环上下文
    pub fn exit_loop(&mut self) {
        self.loop_stack.pop();
    }

    /// 获取当前循环上下文（用于 break/continue）
    pub fn current_loop(&self) -> Option<&LoopContext> {
        self.loop_stack.last()
    }

    /// 获取或创建字符串常量
    pub fn get_or_create_string_constant(&mut self, s: &str) -> String {
        if let Some(name) = self.global_strings.get(s) {
            return name.clone();
        }

        let name = format!("@.str.{}", self.global_counter);
        self.global_counter += 1;

        // 转义字符串
        let escaped = s.replace("\\", "\\\\")
            .replace("\"", "\\\"")
            .replace("\n", "\\0A")
            .replace("\r", "\\0D")
            .replace("\t", "\\09");

        // 存储以便稍后输出到全局区
        self.global_strings.insert(s.to_string(), name.clone());

        name
    }

    /// 获取字符串常量的声明
    pub fn get_string_declarations(&self) -> String {
        let mut result = String::new();
        for (s, name) in &self.global_strings {
            let escaped = s.replace("\\", "\\\\")
                .replace("\"", "\\\"")
                .replace("\n", "\\0A")
                .replace("\r", "\\0D")
                .replace("\t", "\\09");
            let len = s.len() + 1; // +1 for null terminator
            result.push_str(&format!(
                "{} = private unnamed_addr constant [{} x i8] c\"{}\\00\", align 1\n",
                name, len, escaped
            ));
        }
        result
    }

    /// 获取全局字符串映射（用于后处理）
    pub fn get_global_strings(&self) -> &std::collections::HashMap<String, String> {
        &self.global_strings
    }
}
