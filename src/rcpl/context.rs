//! RCPL 上下文管理模块
//!
//! 管理持久化语句、静态字段、方法、类定义和预处理器指令

/// 上下文数据结构
#[derive(Debug, Clone)]
pub struct Context {
    /// 持久化语句（非static定义+赋值）
    persistent_stmts: Vec<String>,
    /// 用户显式写的 static 字段
    static_fields: Vec<String>,
    /// 方法定义
    methods: Vec<String>,
    /// 外部类/接口定义
    classes: Vec<String>,
    /// 预处理器指令（#include, #define 等）
    preprocessor_directives: Vec<String>,
}

impl Context {
    /// 创建新的空上下文
    pub fn new() -> Self {
        Context {
            persistent_stmts: Vec::new(),
            static_fields: Vec::new(),
            methods: Vec::new(),
            classes: Vec::new(),
            preprocessor_directives: Vec::new(),
        }
    }

    /// 添加持久化语句
    pub fn add_persistent_stmt(&mut self, stmt: String) {
        self.persistent_stmts.push(stmt);
    }

    /// 添加或更新赋值语句
    /// 如果存在相同左值的赋值，则替换；否则添加新语句
    /// 返回动作描述："已持久化赋值" 或 "更新赋值"
    pub fn add_or_update_assignment(&mut self, lval: String, code: String) -> &'static str {
        let pattern = format!("{} =", lval);

        let mut found = false;
        let mut new_stmts = Vec::with_capacity(self.persistent_stmts.len());

        for stmt in &self.persistent_stmts {
            // 检查是否以左值开头并后跟等号
            let trimmed = stmt.trim();
            if trimmed.starts_with(&pattern) {
                new_stmts.push(code.clone());
                found = true;
            } else {
                new_stmts.push(stmt.clone());
            }
        }

        if !found {
            new_stmts.push(code);
            self.persistent_stmts = new_stmts;
            "已持久化赋值"
        } else {
            self.persistent_stmts = new_stmts;
            "更新赋值"
        }
    }

    /// 添加显式 static 字段
    pub fn add_static_field(&mut self, field: String) {
        self.static_fields.push(field);
    }

    /// 添加预处理器指令
    /// 如果指令已存在则不会重复添加
    pub fn add_preprocessor_directive(&mut self, directive: String) {
        let trimmed = directive.trim().to_string();
        // 检查是否已存在相同的指令，避免重复
        if !self.preprocessor_directives.contains(&trimmed) {
            self.preprocessor_directives.push(trimmed);
        }
    }

    /// 添加方法
    pub fn add_method(&mut self, method: String) {
        self.methods.push(method);
    }

    /// 添加类/接口
    pub fn add_class(&mut self, class: String) {
        self.classes.push(class);
    }

    /// 移除最后一个持久化语句
    pub fn remove_last_persistent_stmt(&mut self) {
        self.persistent_stmts.pop();
    }

    /// 移除最后一个 static 字段
    pub fn remove_last_static_field(&mut self) {
        self.static_fields.pop();
    }

    /// 移除最后一个预处理器指令
    pub fn remove_last_preprocessor_directive(&mut self) {
        self.preprocessor_directives.pop();
    }

    /// 移除最后一个方法
    pub fn remove_last_method(&mut self) {
        self.methods.pop();
    }

    /// 移除最后一个类
    pub fn remove_last_class(&mut self) {
        self.classes.pop();
    }

    /// 清空上下文
    pub fn clear(&mut self) {
        self.persistent_stmts.clear();
        self.static_fields.clear();
        self.methods.clear();
        self.classes.clear();
        self.preprocessor_directives.clear();
    }

    /// 获取持久化语句列表
    pub fn persistent_stmts(&self) -> &[String] {
        &self.persistent_stmts
    }

    /// 获取 static 字段列表
    pub fn static_fields(&self) -> &[String] {
        &self.static_fields
    }

    /// 获取方法列表
    pub fn methods(&self) -> &[String] {
        &self.methods
    }

    /// 获取类列表
    pub fn classes(&self) -> &[String] {
        &self.classes
    }

    /// 获取预处理器指令列表
    pub fn preprocessor_directives(&self) -> &[String] {
        &self.preprocessor_directives
    }

    /// 显示当前上下文
    pub fn show(&self) {
        println!("=== 当前上下文 ===");

        if !self.preprocessor_directives.is_empty() {
            println!("预处理器指令:");
            for directive in &self.preprocessor_directives {
                println!("  {}", directive);
            }
        }

        if !self.persistent_stmts.is_empty() {
            println!("持久化语句:");
            for stmt in &self.persistent_stmts {
                println!("  {}", stmt);
            }
        }

        if !self.static_fields.is_empty() {
            println!("显式 static 字段:");
            for field in &self.static_fields {
                println!("  {}", field);
            }
        }

        if !self.methods.is_empty() {
            println!("方法:");
            for method in &self.methods {
                // 提取方法名
                let name = Self::extract_method_name(method);
                println!("  {}", name);
            }
        }

        if !self.classes.is_empty() {
            println!("外部类/接口:");
            for class in &self.classes {
                if let Some(name) = Self::extract_class_name(class) {
                    println!("  {}", name);
                }
            }
        }

        let total = self.persistent_stmts.len()
            + self.static_fields.len()
            + self.methods.len()
            + self.classes.len()
            + self.preprocessor_directives.len();
        if total == 0 {
            println!("(空)");
        }
    }

    /// 提取方法名
    fn extract_method_name(method: &str) -> String {
        // 简单解析：找到返回类型后的标识符
        let trimmed = method.trim();

        // 跳过修饰符
        let keywords = [
            "public",
            "private",
            "protected",
            "static",
            "final",
            "abstract",
            "native",
        ];
        let mut rest = trimmed;

        loop {
            let mut found = false;
            for kw in &keywords {
                if let Some(suffix) = rest.strip_prefix(kw) {
                    rest = suffix.trim_start();
                    found = true;
                    break;
                }
            }
            if !found {
                break;
            }
        }

        // 现在应该是返回类型
        if let Some(space_idx) = rest.find(' ') {
            let after_ret_type = &rest[space_idx + 1..].trim_start();
            // 找到方法名（到左括号为止）
            if let Some(paren_idx) = after_ret_type.find('(') {
                return after_ret_type[..paren_idx].trim().to_string();
            }
        }

        "<unknown>".to_string()
    }

    /// 提取类/接口名
    fn extract_class_name(class_def: &str) -> Option<String> {
        let trimmed = class_def.trim_start();

        // 跳过修饰符
        let keywords = ["public", "abstract", "final"];
        let mut rest = trimmed;

        loop {
            let mut found = false;
            for kw in &keywords {
                if let Some(suffix) = rest.strip_prefix(kw) {
                    rest = suffix.trim_start();
                    found = true;
                    break;
                }
            }
            if !found {
                break;
            }
        }

        // 检查是 class 还是 interface
        let kind = if rest.starts_with("class ") {
            "class"
        } else if rest.starts_with("interface ") {
            "interface"
        } else {
            return None;
        };

        // 提取名称
        let after_keyword = if kind == "class" {
            &rest[6..]
        } else {
            &rest[10..]
        };

        let name_end = after_keyword
            .find(|c: char| c.is_whitespace() || c == '{' || c == '<')
            .unwrap_or(after_keyword.len());

        let name = &after_keyword[..name_end];
        Some(format!("{} ({})", name, kind))
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}
