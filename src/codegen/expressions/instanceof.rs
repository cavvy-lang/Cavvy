//! instanceof 表达式代码生成
//!
//! 处理类型检查表达式。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::{cayResult, codegen_error};

impl IRGenerator {
    /// 生成 instanceof 表达式代码
    ///
    /// # Arguments
    /// * `instanceof` - instanceof 表达式
    pub fn generate_instanceof_expression(&mut self, instanceof: &InstanceOfExpr) -> cayResult<String> {
        let expr_result = self.generate_expression(&instanceof.expr)?;
        let (expr_type, expr_val) = self.parse_typed_value(&expr_result);

        let null_label = self.new_label("instanceof.null");
        let check_label = self.new_label("instanceof.check");
        let true_label = self.new_label("instanceof.true");
        let false_label = self.new_label("instanceof.false");
        let end_label = self.new_label("instanceof.end");

        let is_null = self.new_temp();
        if expr_type.ends_with("*") {
            self.emit_line(&format!("  {} = icmp eq {} {}, null", is_null, expr_type, expr_val));
        } else {
            self.emit_line(&format!("  {} = icmp eq i1 0, 1", is_null));
        }

        self.emit_line(&format!("  br i1 {}, label %{}, label %{}", is_null, null_label, check_label));

        self.emit_line(&format!("\n{}:", null_label));
        self.emit_line(&format!("  br label %{}", false_label));

        self.emit_line(&format!("\n{}:", check_label));

        let type_id_ptr = self.new_temp();
        self.emit_line(&format!("  {} = bitcast {} {} to i32*", type_id_ptr, expr_type, expr_val));

        let actual_type_id = self.new_temp();
        self.emit_line(&format!("  {} = load i32, i32* {}", actual_type_id, type_id_ptr));

        let target_type = &instanceof.target_type;
        let target_class = match target_type {
            crate::types::Type::Object(name) => name.clone(),
            _ => return Err(codegen_error("instanceof target must be an object type".to_string())),
        };

        let is_interface = self.type_registry.as_ref()
            .map(|r| r.get_interface(&target_class).is_some())
            .unwrap_or(false);

        if is_interface {
            self.generate_interface_check(&actual_type_id, &target_class, &true_label, &false_label)?;
        } else {
            self.generate_type_check(&actual_type_id, &target_class, &true_label, &false_label)?;
        }

        self.emit_line(&format!("\n{}:", true_label));
        self.emit_line(&format!("  br label %{}", end_label));

        self.emit_line(&format!("\n{}:", false_label));
        self.emit_line(&format!("  br label %{}", end_label));

        self.emit_line(&format!("\n{}:", end_label));
        let result_temp = self.new_temp();
        self.emit_line(&format!("  {} = phi i1 [ 1, %{} ], [ 0, %{} ]",
            result_temp, true_label, false_label));

        Ok(format!("i1 {}", result_temp))
    }

    /// 生成类型检查代码（用于类继承）
    fn generate_type_check(&mut self, actual_type_id: &str, target_class: &str, true_label: &str, false_label: &str) -> cayResult<()> {
        let target_type_id_value = self.get_type_id_value(target_class).unwrap_or(-1);

        let all_matching_type_ids: Vec<i32> = if let Some(ref registry) = self.type_registry {
            let mut result = vec![target_type_id_value];
            for class_info in registry.classes.values() {
                let mut current = class_info.parent.as_ref().map(|p| p.as_str());
                while let Some(parent_name) = current {
                    if parent_name == target_class {
                        if let Some(type_id) = self.get_type_id_value(&class_info.name) {
                            result.push(type_id);
                        }
                        break;
                    }
                    if let Some(parent_class) = registry.get_class(parent_name) {
                        current = parent_class.parent.as_ref().map(|p| p.as_str());
                    } else {
                        break;
                    }
                }
            }
            result
        } else {
            vec![target_type_id_value]
        };

        for (i, type_id_value) in all_matching_type_ids.iter().enumerate() {
            let is_match = self.new_temp();
            self.emit_line(&format!("  {} = icmp eq i32 {}, {}",
                is_match, actual_type_id, type_id_value));

            let next_check_label = if i < all_matching_type_ids.len() - 1 {
                self.new_label(&format!("typecheck.class{}", i))
            } else {
                false_label.to_string()
            };

            self.emit_line(&format!("  br i1 {}, label %{}, label %{}",
                is_match, true_label, next_check_label));

            if i < all_matching_type_ids.len() - 1 {
                self.emit_line(&format!("\n{}:", next_check_label));
            }
        }

        Ok(())
    }

    /// 生成接口检查代码
    fn generate_interface_check(&mut self, actual_type_id: &str, interface_name: &str, true_label: &str, false_label: &str) -> cayResult<()> {
        let implementing_type_ids: Vec<i32> = if let Some(ref registry) = self.type_registry {
            registry.classes.values()
                .filter(|c| {
                    if c.interfaces.contains(&interface_name.to_string()) {
                        return true;
                    }
                    let mut current = c.parent.as_ref().map(|p| p.as_str());
                    while let Some(parent_name) = current {
                        if let Some(parent_class) = registry.get_class(parent_name) {
                            if parent_class.interfaces.contains(&interface_name.to_string()) {
                                return true;
                            }
                            current = parent_class.parent.as_ref().map(|p| p.as_str());
                        } else {
                            break;
                        }
                    }
                    false
                })
                .filter_map(|c| self.get_type_id_value(&c.name))
                .collect()
        } else {
            Vec::new()
        };

        if implementing_type_ids.is_empty() {
            self.emit_line(&format!("  br label %{}", false_label));
            return Ok(());
        }

        for (i, type_id_value) in implementing_type_ids.iter().enumerate() {
            let is_match = self.new_temp();
            self.emit_line(&format!("  {} = icmp eq i32 {}, {}",
                is_match, actual_type_id, type_id_value));

            let next_check_label = if i < implementing_type_ids.len() - 1 {
                self.new_label(&format!("interface_check.class{}", i))
            } else {
                false_label.to_string()
            };

            self.emit_line(&format!("  br i1 {}, label %{}, label %{}",
                is_match, true_label, next_check_label));

            if i < implementing_type_ids.len() - 1 {
                self.emit_line(&format!("\n{}:", next_check_label));
            }
        }

        Ok(())
    }
}
