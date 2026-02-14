//! 成员访问表达式代码生成
//!
//! 处理静态字段访问、对象成员访问和数组 length 属性。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::cayResult;

impl IRGenerator {
    /// 生成成员访问表达式代码
    ///
    /// # Arguments
    /// * `member` - 成员访问表达式
    pub fn generate_member_access(&mut self, member: &MemberAccessExpr) -> cayResult<String> {
        // 检查是否是静态字段访问: ClassName.fieldName
        if let Expr::Identifier(class_name) = &*member.object {
            let static_key = format!("{}.{}", class_name, member.member);
            if let Some(field_info) = self.static_field_map.get(&static_key).cloned() {
                // 静态字段访问 - 返回全局变量的指针
                let temp = self.new_temp();
                self.emit_line(&format!("  {} = load {}, {}* {}, align {}", 
                    temp, field_info.llvm_type, field_info.llvm_type, field_info.name, 
                    self.get_type_align(&field_info.llvm_type)));
                return Ok(format!("{} {}", field_info.llvm_type, temp));
            }
        }
        
        // 特殊处理数组的 .length 属性
        if member.member == "length" {
            let obj = self.generate_expression(&member.object)?;
            let (obj_type, obj_val) = self.parse_typed_value(&obj);
            
            // 检查是否是数组类型（以 * 结尾）
            if obj_type.ends_with("*") {
                // 首先将数组指针转换为 i8*
                let obj_i8 = self.new_temp();
                self.emit_line(&format!("  {} = bitcast {} {} to i8*", obj_i8, obj_type, obj_val));
                
                // 数组长度存储在数组指针前面的 8 字节中
                // 计算长度地址：array_ptr - 8
                let len_ptr_i8 = self.new_temp();
                self.emit_line(&format!("  {} = getelementptr i8, i8* {}, i64 -8", len_ptr_i8, obj_i8));
                
                // 将长度指针转换为 i32*
                let len_ptr = self.new_temp();
                self.emit_line(&format!("  {} = bitcast i8* {} to i32*", len_ptr, len_ptr_i8));
                
                // 加载长度（作为 i32）
                let len_val = self.new_temp();
                self.emit_line(&format!("  {} = load i32, i32* {}, align 4", len_val, len_ptr));
                
                return Ok(format!("i32 {}", len_val));
            }
        }
        
        // 目前仅支持将成员访问视为对象指针的占位符（返回 i8* ptr）
        // 生成对象表达式并返回其指针值
        let obj = self.generate_expression(&member.object)?;
        let (_t, val) = self.parse_typed_value(&obj);
        Ok(format!("i8* {}", val))
    }
}
