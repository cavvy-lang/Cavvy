//! 表达式类型推断辅助函数

use super::super::analyzer::SemanticAnalyzer;
use crate::error::{semantic_error_with_file, SourceLocation};
use crate::error::cayError;

/// 辅助函数：根据SourceLocation创建语义错误
pub fn semantic_error_at_loc(
    loc: &SourceLocation,
    message: impl Into<String>,
) -> cayError {
    semantic_error_with_file(loc.file.clone(), loc.line, loc.column, message)
}

/// 检查成员访问权限
///
/// # Arguments
/// * `member_name` - 成员名称
/// * `is_public` - 成员是否公开
/// * `is_protected` - 成员是否受保护
/// * `is_private` - 成员是否私有
/// * `current_class` - 当前类名
/// * `target_class` - 目标成员所属类名
/// * `type_registry` - 类型注册表（用于检查继承关系）
/// * `loc` - 源代码位置
///
/// # Returns
/// 如果访问被拒绝，返回 Err；否则返回 Ok(())
pub fn check_member_access(
    member_name: &str,
    is_public: bool,
    is_protected: bool,
    is_private: bool,
    current_class: &Option<String>,
    target_class: &str,
    type_registry: &crate::types::TypeRegistry,
    loc: &SourceLocation,
) -> crate::error::cayResult<()> {
    // 公开成员总是可以访问
    if is_public {
        return Ok(());
    }

    // 获取当前类名
    let current_class_name = match current_class {
        Some(name) => name,
        None => {
            // 没有当前类上下文（如顶层函数），不能访问非公开成员
            return Err(semantic_error_at_loc(
                loc,
                format!("{} has private access in {}", member_name, target_class),
            ));
        }
    };

    // 同一个类可以访问所有成员
    if current_class_name == target_class {
        return Ok(());
    }

    // 如果是 protected，检查是否是子类
    if is_protected {
        // 检查当前类是否是目标类的子类
        if is_subclass(current_class_name, target_class, type_registry) {
            return Ok(());
        }
    }

    // 私有或 protected 但不是子类
    let access_type = if is_private { "private" } else { "protected" };
    Err(semantic_error_at_loc(
        loc,
        format!(
            "{} has {} access in {}",
            member_name, access_type, target_class
        ),
    ))
}

/// 检查 child_class 是否是 parent_class 的子类（包括直接和间接继承）
pub fn is_subclass(
    child_class: &str,
    parent_class: &str,
    type_registry: &crate::types::TypeRegistry,
) -> bool {
    let mut current = Some(child_class.to_string());

    while let Some(class_name) = current {
        if class_name == parent_class {
            return true;
        }

        // 获取父类
        if let Some(class_info) = type_registry.get_class(&class_name) {
            current = class_info.parent.clone();
        } else {
            break;
        }
    }

    false
}

pub fn edit_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b_chars.len()).collect();
    let mut curr = vec![0; b_chars.len() + 1];

    for (i, a_ch) in a_chars.iter().enumerate() {
        curr[0] = i + 1;
        for (j, b_ch) in b_chars.iter().enumerate() {
            let cost = if a_ch == b_ch { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b_chars.len()]
}
