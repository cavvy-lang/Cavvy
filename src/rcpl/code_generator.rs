//! RCPL 代码生成模块
//!
//! 根据上下文和当前输入生成完整的 Cavvy 程序

use super::context::Context;
use super::input_parser::InputType;

/// 代码生成器
pub struct CodeGenerator;

impl CodeGenerator {
    /// 创建新的代码生成器
    pub fn new() -> Self {
        CodeGenerator
    }

    /// 生成完整程序
    pub fn generate(&self, context: &Context, input_type: &InputType) -> String {
        let mut parts: Vec<String> = Vec::new();

        // 1. 添加预处理器指令（放在文件最开头）
        let preprocessor_directives = context.preprocessor_directives();
        if !preprocessor_directives.is_empty() {
            parts.push(preprocessor_directives.join("\n"));
        }

        // 2. 添加累积的外部类定义
        let classes = context.classes();
        if !classes.is_empty() {
            parts.push(classes.join("\n\n"));
        }

        // 3. 获取静态字段和方法
        let static_fields = context.static_fields().join("\n");
        let methods = context.methods().join("\n\n");

        // 4. 构建持久化语句块
        let persistent_block = context
            .persistent_stmts()
            .iter()
            .map(|s| format!("    {}", s))
            .collect::<Vec<_>>()
            .join("\n");

        // 5. 确定 main 方法体内容
        let main_body = self.build_main_body(input_type, &persistent_block);

        // 6. 组装 @main 类
        let repl_main = format!(
            r#"@main
class __ReplMain {{
    // Static Fields (explicit)
{}

    // Methods
{}

    public static int main() {{
{}
        return 0;
    }}
}}"#,
            Self::indent_if_not_empty(&static_fields, 4),
            Self::indent_if_not_empty(&methods, 4),
            main_body
        );

        parts.push(repl_main);

        // 7. 组装最终程序
        parts.join("\n\n")
    }

    /// 构建 main 方法体
    fn build_main_body(&self, input_type: &InputType, persistent_block: &str) -> String {
        match input_type {
            InputType::Expression { code } => {
                // 自动包装为 print(表达式);
                let expr = code.trim_end_matches(';').trim();
                if persistent_block.is_empty() {
                    format!("        print({});", expr)
                } else {
                    format!("{}\n        print({});", persistent_block, expr)
                }
            }
            InputType::Statement { code }
            | InputType::For { code }
            | InputType::While { code }
            | InputType::If { code }
            | InputType::DoWhile { code }
            | InputType::Switch { code } => {
                // 语句类型，需要执行持久化块和当前语句
                let stmt = code.trim();
                if persistent_block.is_empty() {
                    format!("        {}", stmt)
                } else {
                    format!("{}\n        {}", persistent_block, stmt)
                }
            }
            _ => {
                // 定义类型（VarDecl, Assignment, StaticField, Method, Class等）
                // 只执行持久化块
                if persistent_block.is_empty() {
                    "        // definitions only".to_string()
                } else {
                    persistent_block.to_string()
                }
            }
        }
    }

    /// 如果内容非空，添加缩进
    fn indent_if_not_empty(content: &str, indent: usize) -> String {
        if content.is_empty() {
            return String::new();
        }

        let spaces = " ".repeat(indent);
        content
            .lines()
            .map(|line| {
                if line.trim().is_empty() {
                    line.to_string()
                } else {
                    format!("{}{}", spaces, line)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 保护字符串（防止正则误处理）
    /// 返回 (保护后的代码, 字符串列表)
    pub fn protect_strings(&self, code: &str) -> (String, Vec<String>) {
        let mut strings: Vec<String> = Vec::new();
        let mut result = String::with_capacity(code.len());
        let mut chars = code.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '"' {
                // 开始收集字符串
                let mut s = String::new();
                s.push(c);

                while let Some(ch) = chars.next() {
                    s.push(ch);
                    if ch == '\\' {
                        // 转义字符，包含下一个字符
                        if let Some(next) = chars.next() {
                            s.push(next);
                        }
                    } else if ch == '"' {
                        // 字符串结束
                        break;
                    }
                }

                strings.push(s);
                // 使用占位符替换
                result.push('\u{FFFE}');
                result.push_str(&(strings.len() - 1).to_string());
                result.push('\u{FFFF}');
            } else {
                result.push(c);
            }
        }

        (result, strings)
    }

    /// 恢复字符串
    pub fn restore_strings(&self, code: &str, strings: &[String]) -> String {
        let mut result = String::with_capacity(code.len() * 2);
        let mut chars = code.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\u{FFFE}' {
                // 收集数字索引
                let mut idx_str = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_ascii_digit() {
                        idx_str.push(ch);
                        chars.next();
                    } else {
                        break;
                    }
                }

                // 跳过结束标记
                if chars.peek() == Some(&'\u{FFFF}') {
                    chars.next();
                }

                // 恢复字符串
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if idx < strings.len() {
                        result.push_str(&strings[idx]);
                    }
                }
            } else {
                result.push(c);
            }
        }

        result
    }
}

impl Default for CodeGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protect_restore_strings() {
        let generator = CodeGenerator::new();
        let code = r#"print("hello world"); int x = 5;"#;

        let (protected, strings) = generator.protect_strings(code);
        assert_eq!(strings.len(), 1);
        assert_eq!(strings[0], "\"hello world\"");

        let restored = generator.restore_strings(&protected, &strings);
        assert_eq!(restored, code);
    }

    #[test]
    fn test_generate_with_expression() {
        let generator = CodeGenerator::new();
        let ctx = Context::new();
        let input = InputType::Expression {
            code: "1 + 2".to_string(),
        };

        let program = generator.generate(&ctx, &input);
        assert!(program.contains("@main"));
        assert!(program.contains("print(1 + 2)"));
    }

    #[test]
    fn test_generate_with_statement() {
        let generator = CodeGenerator::new();
        let ctx = Context::new();
        let input = InputType::Statement {
            code: "int x = 5;".to_string(),
        };

        let program = generator.generate(&ctx, &input);
        assert!(program.contains("@main"));
        assert!(program.contains("int x = 5;"));
    }
}
