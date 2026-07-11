//! 数组表达式代码生成
//!
//! 处理数组创建、数组访问、数组初始化和多维数组。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::{CayResult, ErrorCodes, codegen_error_at};
use crate::types::Type;

impl IRGenerator {
    /// 生成数组创建表达式代码: new Type[size] 或 new Type[size1][size2]...
    ///
    /// # Arguments
    /// * `arr` - 数组创建表达式
    pub fn generate_array_creation(&mut self, arr: &ArrayCreationExpr) -> CayResult<String> {
        // 单态化上下文下，将泛型参数 T 替换为实际类型
        let element_type = self.resolve_type_arg_concrete(&arr.element_type);
        if arr.sizes.len() == 1 {
            // 一维数组
            self.generate_1d_array_creation(&element_type, &arr.sizes[0], &arr.loc)
        } else {
            // 多维数组
            self.generate_md_array_creation(&element_type, &arr.sizes, &arr.loc)
        }
    }

    /// 生成一维数组创建
    /// 内存布局: [长度:i32][填充:i32][元素0][元素1]...[元素N-1]
    /// 返回的指针指向元素0，长度存储在指针前8字节
    ///
    /// # Arguments
    /// * `element_type` - 元素类型
    /// * `size_expr` - 大小表达式
    /// * `loc` - 源码位置
    fn generate_1d_array_creation(
        &mut self,
        element_type: &Type,
        size_expr: &Expr,
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        // 生成数组大小表达式
        let size_val_expr = self.generate_expression(size_expr)?;
        let (size_type, size_val) = self.parse_typed_value(&size_val_expr);

        // 确保大小是整数类型
        if !size_type.starts_with("i") {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                format!("Array size must be integer, got {}", size_type),
            ));
        }

        // 将大小转换为 i64（用于内存分配）
        let size_i64 = if size_type != "i64" {
            let temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = sext {} {} to i64",
                temp, size_type, size_val
            ));
            temp
        } else {
            size_val.to_string()
        };

        // 同时保存为 i32 用于存储长度
        let size_i32 = if size_type != "i32" {
            let temp = self.new_temp();
            // 防御性检查：size_type 必须是纯整数类型
            if size_type.ends_with("*") {
                return Err(codegen_error_at(
                    ErrorCodes::CODEGEN_INVALID_OPERATION,
                    loc.clone(),
                    format!("Array size must be an integer type, got {}", size_type),
                ));
            }
            self.emit_line(&format!(
                "  {} = trunc {} {} to i32",
                temp, size_type, size_val
            ));
            temp
        } else {
            size_val.to_string()
        };

        // 获取元素类型
        let elem_type = self.type_to_llvm(element_type);

        // 计算元素大小
        let elem_size = match element_type {
            Type::Int32 => 4,
            Type::Int64 => 8,
            Type::Float32 => 4,
            Type::Float64 => 8,
            Type::Bool => 1,
            Type::Char => 1,
            Type::String => 8, // 指针大小
            Type::Object(name) => {
                // 检查是否是 enum 类型（struct { i32, i64 } = 16 bytes）
                if let Some(ref registry) = self.type_registry {
                    if registry.get_enum(name).is_some() {
                        16 // enum struct { i32, i64 }
                    } else {
                        8 // 普通对象指针大小
                    }
                } else {
                    8
                }
            }
            Type::Array(_) => 8, // 指针大小
            _ => 8,              // 默认
        };

        // 计算数据字节数 = 大小 * 元素大小
        let data_bytes_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = mul i64 {}, {}",
            data_bytes_temp, size_i64, elem_size
        ));

        // 额外分配 8 字节用于存储长度（i32 + 填充）
        let total_bytes_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = add i64 {}, 8",
            total_bytes_temp, data_bytes_temp
        ));

        // 调用 calloc 分配内存（自动零初始化）
        let calloc_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = call i8* @calloc(i64 1, i64 {})",
            calloc_temp, total_bytes_temp
        ));

        // calloc 失败保护：返回 null 而非崩溃
        let is_null = self.new_temp();
        self.emit_line(&format!(
            "  {} = icmp eq i8* {}, null",
            is_null, calloc_temp
        ));
        let ok_label = self.new_label("arr.alloc.ok");
        let fail_label = self.new_label("arr.alloc.fail");
        let merge_label = self.new_label("arr.alloc.merge");

        // 结果槽（alloca + store 模式，避免 phi 标签问题）
        // 注意：alloca 必须在 br 之前，位于当前基本块内
        let arr_ptr_type = format!("{}*", elem_type);
        let result_slot = self.new_temp();
        self.emit_line(&format!("  {} = alloca {}", result_slot, arr_ptr_type));

        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            is_null, fail_label, ok_label
        ));

        // calloc 失败：返回 null
        self.emit_line(&format!("\n{}:", fail_label));
        self.emit_line(&format!(
            "  store {} null, {}* {}",
            arr_ptr_type, arr_ptr_type, result_slot
        ));
        self.emit_line(&format!("  br label %{}", merge_label));

        // calloc 成功：正常分配
        self.emit_line(&format!("\n{}:", ok_label));
        // 存储长度（前4字节）
        let len_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i32*",
            len_ptr, calloc_temp
        ));
        self.emit_line(&format!(
            "  store i32 {}, i32* {}, align 4",
            size_i32, len_ptr
        ));

        // 计算数据起始地址（跳过8字节长度头）
        let data_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 8",
            data_ptr, calloc_temp
        ));

        // 将 i8* 转换为元素类型指针
        let cast_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to {}*",
            cast_temp, data_ptr, elem_type
        ));
        self.emit_line(&format!(
            "  store {}* {}, {}* {}",
            arr_ptr_type, cast_temp, arr_ptr_type, result_slot
        ));
        self.emit_line(&format!("  br label %{}", merge_label));

        // 合并点：加载结果
        self.emit_line(&format!("\n{}:", merge_label));
        let result_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = load {}*, {}* {}",
            result_temp, arr_ptr_type, arr_ptr_type, result_slot
        ));

        // 返回数组指针（指向数据，长度在指针前8字节；失败时返回 null）
        Ok(format!("{}* {}", elem_type, result_temp))
    }

    /// 生成分配器-backed 数组分配表达式代码
    /// __cay_alloc_array<T>(allocator, count)
    /// 内存布局与 new T[n] 相同: [长度:i32][填充:i32][元素0]...
    /// 通过 Allocator 接口 vtable slot 0 (allocate(long)) 申请内存。
    pub fn generate_alloc_array_expression(
        &mut self,
        alloc_array: &AllocArrayExpr,
    ) -> CayResult<String> {
        // 单态化后解析出具体元素类型
        let element_type = self.resolve_type_arg_concrete(&alloc_array.element_type);

        // 生成大小表达式
        let size_val_expr = self.generate_expression(&alloc_array.size)?;
        let (size_type, size_val) = self.parse_typed_value(&size_val_expr);

        if !size_type.starts_with("i") {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                alloc_array.loc.clone(),
                format!("Array size must be integer, got {}", size_type),
            ));
        }

        // 转换为 i64（用于分配）和 i32（用于存储长度）
        let size_i64 = if size_type != "i64" {
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = sext {} {} to i64", temp, size_type, size_val)
            );
            temp
        } else {
            size_val.to_string()
        };

        let size_i32 = if size_type != "i32" {
            let temp = self.new_temp();
            if size_type.ends_with("*") {
                return Err(codegen_error_at(
                    ErrorCodes::CODEGEN_INVALID_OPERATION,
                    alloc_array.loc.clone(),
                    format!("Array size must be an integer type, got {}", size_type),
                ));
            }
            self.emit_line(&format!("  {} = trunc {} {} to i32", temp, size_type, size_val)
            );
            temp
        } else {
            size_val.to_string()
        };

        let elem_type = self.type_to_llvm(&element_type);
        let elem_size = self.get_type_size(&elem_type);

        // 数据字节数 = count * elem_size
        let data_bytes_temp = self.new_temp();
        self.emit_line(&format!("  {} = mul i64 {}, {}", data_bytes_temp, size_i64, elem_size)
        );

        // 总字节数 = 8 (length header) + data
        let total_bytes_temp = self.new_temp();
        self.emit_line(&format!("  {} = add i64 {}, 8", total_bytes_temp, data_bytes_temp)
        );

        // 生成 allocator 表达式，得到 i8* 对象指针
        let allocator_result = self.generate_expression(&alloc_array.allocator)?;
        let (allocator_type, allocator_val) = self.parse_typed_value(&allocator_result);

        // 确保 allocator 是对象指针（i8*）
        let allocator_i8 = if allocator_type == "i8*" {
            allocator_val.to_string()
        } else if allocator_type.ends_with("*") {
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = bitcast {} {} to i8*", temp, allocator_type, allocator_val)
            );
            temp
        } else {
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = inttoptr {} {} to i8*", temp, allocator_type, allocator_val)
            );
            temp
        };

        // 通过 vtable 分派调用 Allocator.allocate(long)
        // vtable 指针位于对象指针偏移 8 字节处
        let vtable_ptr_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 8",
            vtable_ptr_temp, allocator_i8
        )
        );

        let vtable_temp = self.new_temp();
        self.emit_line(&format!("  {} = load i8*, i8* {}", vtable_temp, vtable_ptr_temp)
        );

        let vtable_array_temp = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to i8**", vtable_array_temp, vtable_temp)
        );

        // 获取 allocate(long) 在 Allocator 接口 vtable 中的槽位
        let interface_names = ["std::Allocator", "Allocator"];
        let mut slot = None;
        for interface_name in &interface_names {
            if self.interface_has_vtable_slot(interface_name, "allocate", &[Type::Int64]) {
                slot = Some(self.get_interface_vtable_slot(interface_name, "allocate", &[Type::Int64]));
                break;
            }
        }
        let slot = slot.unwrap_or(0);

        let slot_offset_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8*, i8** {}, i64 {}",
            slot_offset_temp, vtable_array_temp, slot
        )
        );

        let fn_ptr_temp = self.new_temp();
        self.emit_line(&format!("  {} = load i8*, i8** {}", fn_ptr_temp, slot_offset_temp)
        );

        // 函数指针类型: i64 (i8*, i64)*
        let fn_ptr_cast_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i64 (i8*, i64)*",
            fn_ptr_cast_temp, fn_ptr_temp
        )
        );

        let raw_ptr_i64 = self.new_temp();
        self.emit_line(&format!(
            "  {} = call i64 {}(i8* {}, i64 {})",
            raw_ptr_i64, fn_ptr_cast_temp, allocator_i8, total_bytes_temp
        )
        );

        // 将 long 转回 i8*
        let raw_ptr_i8 = self.new_temp();
        self.emit_line(&format!(
            "  {} = inttoptr i64 {} to i8*",
            raw_ptr_i8, raw_ptr_i64
        )
        );

        // 准备返回槽
        let arr_ptr_type = format!("{}*", elem_type);
        let result_slot = self.new_temp();
        self.emit_line(&format!("  {} = alloca {}", result_slot, arr_ptr_type)
        );

        // 检查分配是否失败（返回 0/null）
        let is_null = self.new_temp();
        self.emit_line(&format!(
            "  {} = icmp eq i8* {}, null",
            is_null, raw_ptr_i8
        )
        );
        let ok_label = self.new_label("arr.alloc.ok");
        let fail_label = self.new_label("arr.alloc.fail");
        let merge_label = self.new_label("arr.alloc.merge");

        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            is_null, fail_label, ok_label
        )
        );

        // 失败分支
        self.emit_line(&format!("\n{}:", fail_label)
        );
        self.emit_line(&format!(
            "  store {} null, {}* {}",
            arr_ptr_type, arr_ptr_type, result_slot
        )
        );
        self.emit_line(&format!("  br label %{}", merge_label)
        );

        // 成功分支：写长度头并返回数据指针
        self.emit_line(&format!("\n{}:", ok_label)
        );

        // 用 llvm.memset 清零整块内存（保持与 calloc 类似的零初始化语义）
        self.emit_line(&format!(
            "  call void @llvm.memset.p0i8.i64(i8* {}, i8 0, i64 {}, i1 false)",
            raw_ptr_i8, total_bytes_temp
        )
        );

        // 存储长度
        let len_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i32*",
            len_ptr, raw_ptr_i8
        )
        );
        self.emit_line(&format!(
            "  store i32 {}, i32* {}, align 4",
            size_i32, len_ptr
        )
        );

        // 计算数据起始地址（跳过 8 字节长度头）
        let data_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 8",
            data_ptr, raw_ptr_i8
        )
        );

        // 转换为元素类型指针
        let cast_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to {}*",
            cast_temp, data_ptr, elem_type
        )
        );
        self.emit_line(&format!(
            "  store {}* {}, {}* {}",
            arr_ptr_type, cast_temp, arr_ptr_type, result_slot
        )
        );
        self.emit_line(&format!("  br label %{}", merge_label)
        );

        // 合并点
        self.emit_line(&format!("\n{}:", merge_label)
        );
        let result_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = load {}*, {}* {}",
            result_temp, arr_ptr_type, arr_ptr_type, result_slot
        )
        );

        Ok(format!("{}* {}", elem_type, result_temp))
    }

    /// 生成多维数组创建: new Type[size1][size2]...[sizeN] 或 new Type[size][] (不规则数组)
    ///
    /// # Arguments
    /// * `element_type` - 元素类型
    /// * `sizes` - 各维度大小表达式列表
    /// * `loc` - 源码位置
    fn generate_md_array_creation(
        &mut self,
        element_type: &Type,
        sizes: &[Expr],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        // 多维数组实现：分配一个指针数组，每个指针指向子数组
        // 例如 new int[3][4][5]:
        // 1. 分配 3 个指针的数组 (int**)
        // 2. 循环 3 次，每次递归分配 [4][5] 的子数组
        // 3. 将子数组指针存入父数组
        //
        // 不规则数组 new int[3][]:
        // 1. 分配 3 个指针的数组，初始化为 null
        // 2. 不自动分配子数组，由用户后续手动分配

        if sizes.len() < 2 {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                "Multidimensional array needs at least 2 dimensions".to_string(),
            ));
        }

        // 检查是否有空维度（不规则数组）
        let has_empty_dimension = sizes.iter().any(|s| {
            if let Expr::Literal(lit_expr) = s {
                matches!(lit_expr.value, LiteralValue::Null)
            } else {
                false
            }
        });

        // 递归创建子数组类型（去掉第一维）
        let sub_sizes = &sizes[1..];

        // 生成第一维大小
        let first_size_expr = self.generate_expression(&sizes[0])?;
        let (first_size_type, first_size_val) = self.parse_typed_value(&first_size_expr);

        let first_size_i64 = if first_size_type != "i64" {
            let temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = sext {} {} to i64",
                temp, first_size_type, first_size_val
            ));
            temp
        } else {
            first_size_val.to_string()
        };

        // 获取元素类型的 LLVM 表示
        let elem_llvm_type = self.type_to_llvm(element_type);

        // 确定子数组的 LLVM 类型
        // 如果还有多个维度，子数组是指向更低维度的指针
        // 如果只剩一个维度，子数组是元素指针
        let sub_array_llvm_type = if sub_sizes.len() == 1 {
            format!("{}*", elem_llvm_type)
        } else {
            // 递归获取子数组类型
            format!("{}*", self.get_md_array_type(element_type, sub_sizes.len()))
        };

        // 分配指针数组 (elem_type** 用于存储子数组指针)
        let ptr_array_bytes = self.new_temp();
        self.emit_line(&format!(
            "  {} = mul i64 {}, 8",
            ptr_array_bytes, first_size_i64
        ));

        let calloc_ptr_array = self.new_temp();
        self.emit_line(&format!(
            "  {} = call i8* @calloc(i64 1, i64 {})",
            calloc_ptr_array, ptr_array_bytes
        ));

        // 转换为正确的指针类型
        let ptr_array = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to {}*",
            ptr_array, calloc_ptr_array, sub_array_llvm_type
        ));

        // 如果不规则数组（有空维度），不分配子数组，直接返回
        if has_empty_dimension {
            // 返回指针数组（所有子数组指针初始化为 null）
            return Ok(format!("{}* {}", sub_array_llvm_type, ptr_array));
        }

        // 生成循环来分配每个子数组
        let loop_label = self.new_label("md_loop");
        let body_label = self.new_label("md_body");
        let end_label = self.new_label("md_end");

        // 循环变量 - 使用临时变量名避免冲突
        let loop_var = self.new_temp();
        self.emit_line(&format!("  {} = alloca i64", loop_var));
        self.emit_line(&format!("  store i64 0, i64* {}", loop_var));

        // 跳转到循环条件
        self.emit_line(&format!("  br label %{}", loop_label));

        // 循环条件
        self.emit_line(&format!("\n{}:", loop_label));
        let current_idx = self.new_temp();
        self.emit_line(&format!("  {} = load i64, i64* {}", current_idx, loop_var));
        let cond = self.new_temp();
        self.emit_line(&format!(
            "  {} = icmp slt i64 {}, {}",
            cond, current_idx, first_size_i64
        ));
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            cond, body_label, end_label
        ));

        // 循环体
        self.emit_line(&format!("\n{}:", body_label));

        // 分配子数组
        let sub_array = if sub_sizes.len() == 1 {
            // 最后一维，创建一维数组
            self.generate_1d_array_creation(element_type, &sub_sizes[0], loc)?
        } else {
            // 还有多个维度，递归创建多维数组
            self.generate_md_array_creation(element_type, sub_sizes, loc)?
        };
        let (_, sub_array_val) = self.parse_typed_value(&sub_array);

        // 将子数组指针存入指针数组
        let elem_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr {}, {}* {}, i64 {}",
            elem_ptr, sub_array_llvm_type, sub_array_llvm_type, ptr_array, current_idx
        ));

        self.emit_line(&format!(
            "  store {} {}, {}* {}",
            sub_array_llvm_type, sub_array_val, sub_array_llvm_type, elem_ptr
        ));

        // 增加循环变量
        let next_idx = self.new_temp();
        self.emit_line(&format!("  {} = add i64 {}, 1", next_idx, current_idx));
        self.emit_line(&format!("  store i64 {}, i64* {}", next_idx, loop_var));

        // 跳回循环条件
        self.emit_line(&format!("  br label %{}", loop_label));

        // 循环结束
        self.emit_line(&format!("\n{}:", end_label));

        // 返回指针数组
        Ok(format!("{}* {}", sub_array_llvm_type, ptr_array))
    }

    /// 获取多维数组类型的 LLVM 表示
    ///
    /// # Arguments
    /// * `element_type` - 元素类型
    /// * `dimensions` - 维度数
    fn get_md_array_type(&self, element_type: &Type, dimensions: usize) -> String {
        let base = self.type_to_llvm(element_type);
        format!("{}{}", base, "*".repeat(dimensions))
    }

    /// 获取数组元素指针（用于赋值操作）
    ///
    /// # Arguments
    /// * `arr` - 数组访问表达式
    ///
    /// # Returns
    /// (元素类型, 元素指针, 索引值)
    pub fn get_array_element_ptr(
        &mut self,
        arr: &ArrayAccessExpr,
    ) -> CayResult<(String, String, String)> {
        // 生成数组表达式
        // 特殊处理：如果数组表达式是 MemberAccess（如 this.stack），我们需要获取指针而不是值
        // 使用 generate_expression 获取数组指针的值（已加载）
        // 对 MemberAccess（如 this.tokens、Fibonacci.memo）和 Identifier（如 tokens）
        // 都会正确返回加载后的数组数据指针值
        let (array_type, array_val) = {
            let array_expr = self.generate_expression(&arr.array)?;
            self.parse_typed_value(&array_expr)
        };

        // 生成索引表达式
        let index_expr = self.generate_expression(&arr.index)?;
        let (index_type, index_val) = self.parse_typed_value(&index_expr);

        // 确保索引是整数类型
        if !index_type.starts_with("i") {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                arr.loc.clone(),
                format!("Array index must be integer, got {}", index_type),
            ));
        }

        // 将索引转换为 i64
        let index_i64 = if index_type != "i64" {
            let temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = sext {} {} to i64",
                temp, index_type, index_val
            ));
            temp
        } else {
            index_val.to_string()
        };

        // 获取数组元素类型（去掉末尾的一个 *）
        // 例如: i32* -> i32, i32** -> i32*, i64* -> i64
        let elem_type = if array_type.ends_with("*") {
            // 找到最后一个 * 的位置，去掉它
            let len = array_type.len();
            array_type[..len - 1].to_string()
        } else {
            // 如果不是指针类型，假设是 i64*（向后兼容）
            "i64".to_string()
        };

        // 计算元素地址
        let elem_ptr_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr {}, {}* {}, i64 {}",
            elem_ptr_temp, elem_type, elem_type, array_val, index_i64
        ));

        Ok((elem_type, elem_ptr_temp, index_i64))
    }

    /// 生成数组访问表达式代码: arr[index]
    ///
    /// # Arguments
    /// * `arr` - 数组访问表达式
    pub fn generate_array_access(&mut self, arr: &ArrayAccessExpr) -> CayResult<String> {
        let (elem_type, elem_ptr_temp, _) = self.get_array_element_ptr(arr)?;

        // 加载元素值
        let elem_temp = self.new_temp();
        let align = self.get_type_align(&elem_type);
        self.emit_line(&format!(
            "  {} = load {}, {}* {}, align {}",
            elem_temp, elem_type, elem_type, elem_ptr_temp, align
        ));

        Ok(format!("{} {}", elem_type, elem_temp))
    }

    /// 生成数组初始化表达式代码: {1, 2, 3}
    /// 内存布局: [长度:i32][填充:i32][元素0][元素1]...[元素N-1]
    ///
    /// # Arguments
    /// * `init` - 数组初始化表达式
    pub fn generate_array_init(&mut self, init: &ArrayInitExpr) -> CayResult<String> {
        if init.elements.is_empty() {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                init.loc.clone(),
                "Cannot generate code for empty array initializer".to_string(),
            ));
        }

        // 推断元素类型（从第一个元素）
        let first_elem = self.generate_expression(&init.elements[0])?;
        let (elem_llvm_type, _) = self.parse_typed_value(&first_elem);

        // 获取元素大小
        let elem_size = match elem_llvm_type.as_str() {
            "i1" => 1,
            "i8" => 1,
            "i32" => 4,
            "i64" => 8,
            "float" => 4,
            "double" => 8,
            _ => 8, // 指针类型
        };

        let num_elements = init.elements.len() as i64;

        // 计算数据字节数
        let data_bytes = num_elements * elem_size;
        // 额外分配 8 字节用于存储长度
        let total_bytes = data_bytes + 8;

        // 分配内存（使用 calloc 自动零初始化）
        let calloc_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = call i8* @calloc(i64 1, i64 {})",
            calloc_temp, total_bytes
        ));

        // 存储长度（前4字节）- calloc 已零初始化，只需设置长度
        let len_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i32*",
            len_ptr, calloc_temp
        ));
        self.emit_line(&format!(
            "  store i32 {}, i32* {}, align 4",
            num_elements, len_ptr
        ));

        // 计算数据起始地址（跳过8字节长度头）
        let data_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 8",
            data_ptr, calloc_temp
        ));

        // 转换为元素类型指针
        let cast_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to {}*",
            cast_temp, data_ptr, elem_llvm_type
        ));

        // 存储每个元素
        for (i, elem) in init.elements.iter().enumerate() {
            let elem_val = self.generate_expression(elem)?;
            let (_, val) = self.parse_typed_value(&elem_val);

            // 获取元素地址
            let elem_ptr = self.new_temp();
            self.emit_line(&format!(
                "  {} = getelementptr {}, {}* {}, i64 {}",
                elem_ptr, elem_llvm_type, elem_llvm_type, cast_temp, i
            ));

            // 存储元素
            self.emit_line(&format!(
                "  store {} {}, {}* {}",
                elem_llvm_type, val, elem_llvm_type, elem_ptr
            ));
        }

        // 返回数组指针（指向数据，长度在指针前8字节）
        Ok(format!("{}* {}", elem_llvm_type, cast_temp))
    }

    /// 生成数组初始化表达式代码，使用指定的目标类型: {1, 2, 3}
    /// 内存布局: [长度:i32][填充:i32][元素0][元素1]...[元素N-1]
    ///
    /// # Arguments
    /// * `init` - 数组初始化表达式
    /// * `target_type` - 目标数组类型
    pub fn generate_array_init_with_type(
        &mut self,
        init: &ArrayInitExpr,
        target_type: &Type,
    ) -> CayResult<String> {
        if init.elements.is_empty() {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                init.loc.clone(),
                "Cannot generate code for empty array initializer".to_string(),
            ));
        }

        // 从目标类型获取元素类型
        let elem_llvm_type = if let Type::Array(elem_type) = target_type {
            self.type_to_llvm(elem_type)
        } else {
            // 如果目标类型不是数组，使用第一个元素的类型
            let first_elem = self.generate_expression(&init.elements[0])?;
            let (elem_type, _) = self.parse_typed_value(&first_elem);
            elem_type
        };

        // 获取元素大小
        let elem_size = match elem_llvm_type.as_str() {
            "i1" => 1,
            "i8" => 1,
            "i32" => 4,
            "i64" => 8,
            "float" => 4,
            "double" => 8,
            _ => 8, // 指针类型
        };

        let num_elements = init.elements.len() as i64;

        // 计算数据字节数
        let data_bytes = num_elements * elem_size;
        // 额外分配 8 字节用于存储长度
        let total_bytes = data_bytes + 8;

        // 分配内存（使用 calloc 自动零初始化）
        let calloc_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = call i8* @calloc(i64 1, i64 {})",
            calloc_temp, total_bytes
        ));

        // 存储长度（前4字节）- calloc 已零初始化，只需设置长度
        let len_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i32*",
            len_ptr, calloc_temp
        ));
        self.emit_line(&format!(
            "  store i32 {}, i32* {}, align 4",
            num_elements, len_ptr
        ));

        // 计算数据起始地址（跳过8字节长度头）
        let data_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 8",
            data_ptr, calloc_temp
        ));

        // 转换为元素类型指针
        let cast_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to {}*",
            cast_temp, data_ptr, elem_llvm_type
        ));

        // 存储每个元素
        for (i, elem) in init.elements.iter().enumerate() {
            let elem_val = self.generate_expression(elem)?;
            let (elem_value_type, val) = self.parse_typed_value(&elem_val);

            // 如果需要，进行类型转换
            let final_val = if elem_value_type != elem_llvm_type {
                let temp = self.new_temp();
                // 整数到浮点数转换
                if elem_value_type.starts_with("i")
                    && !elem_value_type.ends_with("*")
                    && (elem_llvm_type == "float" || elem_llvm_type == "double")
                {
                    self.emit_line(&format!(
                        "  {} = sitofp {} {} to {}",
                        temp, elem_value_type, val, elem_llvm_type
                    ));
                    temp.clone()
                }
                // 浮点数到整数转换
                else if (elem_value_type == "float" || elem_value_type == "double")
                    && elem_llvm_type.starts_with("i")
                {
                    self.emit_line(&format!(
                        "  {} = fptosi {} {} to {}",
                        temp, elem_value_type, val, elem_llvm_type
                    ));
                    temp.clone()
                }
                // 浮点数类型转换
                else if elem_value_type == "double" && elem_llvm_type == "float" {
                    self.emit_line(&format!("  {} = fptrunc double {} to float", temp, val));
                    temp.clone()
                } else if elem_value_type == "float" && elem_llvm_type == "double" {
                    self.emit_line(&format!("  {} = fpext float {} to double", temp, val));
                    temp.clone()
                }
                // 指针到整数转换 (ptrtoint)
                else if elem_value_type.ends_with("*")
                    && elem_llvm_type.starts_with("i")
                    && !elem_llvm_type.ends_with("*")
                {
                    self.emit_line(&format!(
                        "  {} = ptrtoint {} {} to {}",
                        temp, elem_value_type, val, elem_llvm_type
                    ));
                    temp.clone()
                }
                // 整数到指针转换 (inttoptr)
                else if elem_llvm_type.ends_with("*")
                    && elem_value_type.starts_with("i")
                    && !elem_value_type.ends_with("*")
                {
                    self.emit_line(&format!(
                        "  {} = inttoptr {} {} to {}",
                        temp, elem_value_type, val, elem_llvm_type
                    ));
                    temp.clone()
                }
                // 指针类型转换 (bitcast)
                else if elem_value_type.ends_with("*") && elem_llvm_type.ends_with("*") {
                    self.emit_line(&format!(
                        "  {} = bitcast {} {} to {}",
                        temp, elem_value_type, val, elem_llvm_type
                    ));
                    temp.clone()
                } else {
                    // 无法转换，使用原值（可能导致编译错误）
                    val.to_string()
                }
            } else {
                val.to_string()
            };

            // 获取元素地址
            let elem_ptr = self.new_temp();
            self.emit_line(&format!(
                "  {} = getelementptr {}, {}* {}, i64 {}",
                elem_ptr, elem_llvm_type, elem_llvm_type, cast_temp, i
            ));

            // 存储元素
            self.emit_line(&format!(
                "  store {} {}, {}* {}",
                elem_llvm_type, final_val, elem_llvm_type, elem_ptr
            ));
        }

        // 返回数组指针（指向数据，长度在指针前8字节）
        Ok(format!("{}* {}", elem_llvm_type, cast_temp))
    }
}
