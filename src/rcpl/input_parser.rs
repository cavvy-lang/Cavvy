//! RCPL 输入解析模块
//!
//! 解析用户输入，识别表达式、语句、变量定义、函数定义、类定义等类型

/// 输入类型枚举
#[derive(Debug, Clone, PartialEq)]
pub enum InputType {
    /// 空输入
    Empty,
    /// 表达式（无分号结尾，需要自动打印）
    Expression {
        code: String,
    },
    /// 语句（带分号结尾）
    Statement {
        code: String,
    },
    /// 变量定义：类型 名字 = 值;
    VarDecl {
        is_final: bool,
        name: String,
        code: String,
    },
    /// 赋值语句：左值 = 右值;
    Assignment {
        lval: String,
        code: String,
    },
    /// 显式 static 字段
    StaticField {
        code: String,
    },
    /// 方法定义
    Method {
        name: String,
        ret_type: String,
        modifiers: String,
        code: String,
    },
    /// 类定义
    Class {
        name: String,
        code: String,
    },
    /// 接口定义
    Interface {
        name: String,
        code: String,
    },
    /// 预处理器指令
    Preprocessor {
        code: String,
    },
    /// 控制流语句
    For {
        code: String,
    },
    While {
        code: String,
    },
    If {
        code: String,
    },
    DoWhile {
        code: String,
    },
    Switch {
        code: String,
    },
}

/// 输入解析器
pub struct InputParser;

impl InputParser {
    /// 创建新的解析器
    pub fn new() -> Self {
        InputParser
    }

    /// 解析输入代码，返回输入类型
    pub fn parse(&self, code: &str) -> InputType {
        let trimmed = code.trim();

        if trimmed.is_empty() {
            return InputType::Empty;
        }

        // 预处理器指令（不以分号结尾，需要优先检测）
        if trimmed.starts_with('#') {
            return InputType::Preprocessor {
                code: trimmed.to_string(),
            };
        }

        // 检测分号或右花括号结尾 - 视为定义或语句
        if trimmed.ends_with(';') || trimmed.ends_with('}') {
            // 类/接口定义
            if let Some(cap) = self.try_parse_class(trimmed) {
                return cap;
            }

            // 显式 static 字段
            if self.is_static_field(trimmed) {
                return InputType::StaticField {
                    code: trimmed.to_string(),
                };
            }

            // 函数定义
            if let Some(method) = self.try_parse_method(trimmed) {
                return method;
            }

            // 变量定义
            if let Some(var_decl) = self.try_parse_var_decl(trimmed) {
                return var_decl;
            }

            // 赋值语句
            if let Some(assignment) = self.try_parse_assignment(trimmed) {
                return assignment;
            }

            // 控制流语句
            if let Some(control) = self.try_parse_control_flow(trimmed) {
                return control;
            }

            // 其他带分号的视为语句
            InputType::Statement {
                code: trimmed.to_string(),
            }
        } else {
            // 无分号结尾 - 视为表达式，自动包装
            InputType::Expression {
                code: trimmed.to_string(),
            }
        }
    }

    /// 尝试解析类/接口定义
    fn try_parse_class(&self, code: &str) -> Option<InputType> {
        let trimmed = code.trim_start();

        // 跳过修饰符
        let keywords = ["public", "pub", "abstract", "final"];
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
        if rest.starts_with("class ") {
            let after_keyword = &rest[6..].trim_start();
            let name_end = after_keyword
                .find(|c: char| c.is_whitespace() || c == '{' || c == '<')
                .unwrap_or(after_keyword.len());
            let name = &after_keyword[..name_end];
            return Some(InputType::Class {
                name: name.to_string(),
                code: code.to_string(),
            });
        }

        if rest.starts_with("interface ") {
            let after_keyword = &rest[10..].trim_start();
            let name_end = after_keyword
                .find(|c: char| c.is_whitespace() || c == '{' || c == '<')
                .unwrap_or(after_keyword.len());
            let name = &after_keyword[..name_end];
            return Some(InputType::Interface {
                name: name.to_string(),
                code: code.to_string(),
            });
        }

        None
    }

    /// 检查是否为显式 static 字段
    fn is_static_field(&self, code: &str) -> bool {
        code.trim_start().starts_with("static ")
    }

    /// 尝试解析方法定义
    fn try_parse_method(&self, code: &str) -> Option<InputType> {
        let trimmed = code.trim_start();

        // 收集修饰符
        let keywords = [
            "public",
            "pub",
            "private",
            "priv",
            "protected",
            "static",
            "final",
            "abstract",
            "native",
        ];
        let mut rest = trimmed;
        let mut modifiers = Vec::new();

        loop {
            let mut found = false;
            for kw in &keywords {
                if let Some(suffix) = rest.strip_prefix(kw) {
                    modifiers.push(*kw);
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
        let ret_type_end = rest.find(|c: char| c.is_whitespace())?;
        let ret_type = &rest[..ret_type_end];

        // 检查是否为 fn 关键字开头的方法（类型后置式）
        // 格式: [modifiers] fn name(params) [-> ret] { body }
        if ret_type == "fn" {
            let after_fn = &rest[ret_type_end..].trim_start();
            let name_end = after_fn.find('(')?;
            let name = after_fn[..name_end].trim();
            return Some(InputType::Method {
                name: name.to_string(),
                ret_type: "auto".to_string(),
                modifiers: modifiers.join(" "),
                code: code.to_string(),
            });
        }

        // 检查是否为有效的返回类型
        let valid_types = [
            "int", "long", "float", "double", "bool", "string", "char", "void",
        ];
        if !valid_types.contains(&ret_type) && !ret_type.ends_with("[]") {
            return None;
        }

        let after_ret_type = &rest[ret_type_end..].trim_start();

        // 找到方法名（到左括号为止）
        let name_end = after_ret_type.find('(')?;
        let name = after_ret_type[..name_end].trim();

        // 排除 main 函数
        if name == "main" {
            return None;
        }

        Some(InputType::Method {
            name: name.to_string(),
            ret_type: ret_type.to_string(),
            modifiers: modifiers.join(" "),
            code: code.to_string(),
        })
    }

    /// 尝试解析变量定义
    fn try_parse_var_decl(&self, code: &str) -> Option<InputType> {
        let trimmed = code.trim_start();

        // 检查 final 修饰符
        let (is_final, rest): (bool, &str) = if trimmed.starts_with("final ") {
            (true, trimmed[6..].trim_start())
        } else {
            (false, trimmed)
        };

        // 检查类型关键字
        let types = [
            "auto", "var", "let", "int", "long", "float", "double", "bool", "string", "char",
        ];
        let mut var_type = None;
        let mut after_type = rest;

        for t in &types {
            if let Some(suffix) = rest.strip_prefix(t) {
                // 检查后面是否跟着数组括号或其他空白
                let next = suffix.chars().next();
                if next.is_none() || next.unwrap().is_whitespace() || next.unwrap() == '[' {
                    var_type = Some(*t);
                    after_type = suffix;
                    break;
                }
            }
        }

        var_type?;

        // 跳过数组括号
        let mut after_brackets = after_type;
        while after_brackets.starts_with('[') {
            if let Some(end) = after_brackets.find(']') {
                after_brackets = &after_brackets[end + 1..];
            } else {
                return None;
            }
        }

        let after_ws = after_brackets.trim_start();

        // 提取变量名
        let name_end = after_ws
            .find(|c: char| c.is_whitespace() || c == '=' || c == ';')
            .unwrap_or(after_ws.len());
        let name = &after_ws[..name_end];

        // 变量名应该有效
        if name.is_empty() || !name.chars().next().unwrap().is_ascii_alphabetic() {
            return None;
        }

        Some(InputType::VarDecl {
            is_final,
            name: name.to_string(),
            code: code.to_string(),
        })
    }

    /// 尝试解析赋值语句
    fn try_parse_assignment(&self, code: &str) -> Option<InputType> {
        let trimmed = code.trim_start();

        // 找到等号位置
        let eq_pos = trimmed.find('=')?;
        let left = &trimmed[..eq_pos].trim_end();

        // 确保左边不是类型定义
        let type_keywords = [
            "int", "long", "float", "double", "bool", "string", "char", "auto", "var", "let",
            "final",
        ];
        let first_word = left.split_whitespace().next().unwrap_or("");

        if type_keywords.contains(&first_word) {
            return None;
        }

        // 提取左值（支持数组访问和字段访问的简单形式）
        let lval = left.to_string();

        Some(InputType::Assignment {
            lval,
            code: code.to_string(),
        })
    }

    /// 尝试解析控制流语句
    fn try_parse_control_flow(&self, code: &str) -> Option<InputType> {
        let trimmed = code.trim_start();

        if trimmed.starts_with("for") {
            return Some(InputType::For {
                code: code.to_string(),
            });
        }

        if trimmed.starts_with("while") {
            return Some(InputType::While {
                code: code.to_string(),
            });
        }

        if trimmed.starts_with("if") {
            return Some(InputType::If {
                code: code.to_string(),
            });
        }

        if trimmed.starts_with("do") {
            return Some(InputType::DoWhile {
                code: code.to_string(),
            });
        }

        if trimmed.starts_with("switch") {
            return Some(InputType::Switch {
                code: code.to_string(),
            });
        }

        None
    }
}

impl Default for InputParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_fn_method() {
        let parser = InputParser::new();
        let input = "public static fn add(int a, int b) {\nreturn a+b;\n}";
        let result = parser.parse(input);
        assert!(
            matches!(result, InputType::Method { ref name, .. } if name == "add"),
            "expected Method, got {:?}",
            result
        );
    }

    #[test]
    fn test_parse_fn_method_pub_alias() {
        let parser = InputParser::new();
        let input = "pub static fn add(int a, int b) {\nreturn a+b;\n}";
        let result = parser.parse(input);
        assert!(
            matches!(result, InputType::Method { ref name, .. } if name == "add"),
            "expected Method, got {:?}",
            result
        );
    }
}
