//! 类型检查实现

use super::analyzer::SemanticAnalyzer;
use super::symbol_table::SemanticSymbolInfo;
use crate::ast::*;
use crate::error::cayResult;
use crate::types::{ParameterInfo, Type};

impl SemanticAnalyzer {
    /// 类型检查程序
    pub fn type_check_program(&mut self, program: &mut Program) -> cayResult<()> {
        // 先类型检查顶层函数，以便类方法调用时能看到已推断的返回类型
        self.type_registry.current_namespace.clear();
        for func in &mut program.top_level_functions {
            self.current_class = None; // 顶层函数不属于任何类
            self.current_method = Some(func.name.clone());
            self.current_method_is_static = true; // 顶层函数都是静态的
            self.current_method_is_constructor = false;
            self.symbol_table.enter_scope();

            // 添加参数到符号表
            for param in &func.params {
                self.symbol_table.declare(
                    param.name.clone(),
                    SemanticSymbolInfo {
                        name: param.name.clone(),
                        symbol_type: param.param_type.clone(),
                        is_final: false,
                        is_initialized: true,
                    },
                );
            }

            let is_auto = func.return_type == Type::Auto;
            if is_auto {
                self.current_inferring_return = Some(Type::Void);
            }

            // 类型检查函数体
            self.type_check_statement(&Stmt::Block(func.body.clone()), Some(&func.return_type))?;

            if is_auto {
                if let Some(inferred) = self.current_inferring_return.take() {
                    func.return_type = inferred;
                }
            }

            self.symbol_table.exit_scope();
            self.current_method = None;
            self.current_method_is_static = false;
        }

        // 更新 self.program 以反映顶层函数的最新返回类型
        self.program = Some(std::rc::Rc::new(program.clone()));

        for class in &mut program.classes {
            self.current_class = Some(class.name.clone());
            self.current_class_type_params = class.type_params.clone();
            self.type_registry.current_namespace = class.namespace_path.clone();

            for member in &mut class.members {
                match member {
                    ClassMember::Method(method) => {
                        self.current_method = Some(method.name.clone());
                        self.current_method_is_static =
                            method.modifiers.contains(&Modifier::Static);
                        self.current_method_is_constructor = false;
                        self.symbol_table.enter_scope();

                        // 非静态方法需要添加 this
                        if !self.current_method_is_static {
                            if let Some(current_class) = &self.current_class {
                                self.symbol_table.declare(
                                    "this".to_string(),
                                    SemanticSymbolInfo {
                                        name: "this".to_string(),
                                        symbol_type: Type::Object(current_class.clone()),
                                        is_final: true,
                                        is_initialized: true,
                                    },
                                );
                            }
                        }

                        // 添加参数到符号表
                        for param in &method.params {
                            // 对泛型类的参数类型进行参数替换
                            let param_type = if class.type_params.is_empty() {
                                param.param_type.clone()
                            } else {
                                self.replace_type_params(&param.param_type, &class.type_params)
                            };
                            self.symbol_table.declare(
                                param.name.clone(),
                                SemanticSymbolInfo {
                                    name: param.name.clone(),
                                    symbol_type: param_type,
                                    is_final: false,
                                    is_initialized: true,
                                },
                            );
                        }

                        // 类型检查方法体
                        if let Some(body) = &method.body {
                            // 对泛型类的返回类型进行参数替换
                            let mut return_type = if class.type_params.is_empty() {
                                method.return_type.clone()
                            } else {
                                self.replace_type_params(&method.return_type, &class.type_params)
                            };

                            let is_auto = return_type == Type::Auto;
                            if is_auto {
                                self.current_inferring_return = Some(Type::Void);
                            }

                            self.type_check_statement(
                                &Stmt::Block(body.clone()),
                                Some(&return_type),
                            )?;

                            if is_auto {
                                if let Some(inferred) = self.current_inferring_return.take() {
                                    method.return_type = inferred.clone();
                                    return_type = inferred.clone();
                                    // 同步更新 type_registry 中该方法的返回类型
                                    let _ = self.type_registry.update_method_return_type(
                                        &class.name,
                                        &method.name,
                                        &method.params,
                                        inferred,
                                    );
                                }
                            }
                        }

                        self.symbol_table.exit_scope();
                        self.current_method = None;
                        self.current_method_is_static = false;
                    }
                    ClassMember::Field(_) => {
                        // 字段类型检查暂不实现
                    }
                    ClassMember::Constructor(ctor) => {
                        // 构造函数类型检查
                        self.current_method_is_static = false;
                        self.current_method_is_constructor = true;
                        self.symbol_table.enter_scope();

                        // 添加 this 到符号表
                        self.symbol_table.declare(
                            "this".to_string(),
                            SemanticSymbolInfo {
                                name: "this".to_string(),
                                symbol_type: Type::Object(class.name.clone()),
                                is_final: true,
                                is_initialized: true,
                            },
                        );

                        // 添加参数到符号表
                        for param in &ctor.params {
                            self.symbol_table.declare(
                                param.name.clone(),
                                SemanticSymbolInfo {
                                    name: param.name.clone(),
                                    symbol_type: param.param_type.clone(),
                                    is_final: false,
                                    is_initialized: true,
                                },
                            );
                        }

                        // 类型检查构造函数体
                        self.type_check_statement(
                            &Stmt::Block(ctor.body.clone()),
                            Some(&Type::Void),
                        )?;

                        self.symbol_table.exit_scope();
                        self.current_method_is_constructor = false;
                    }
                    ClassMember::Destructor(dtor) => {
                        // 析构函数类型检查
                        self.current_method_is_static = false;
                        self.current_method_is_constructor = false;
                        self.symbol_table.enter_scope();

                        // 添加 this 到符号表
                        self.symbol_table.declare(
                            "this".to_string(),
                            SemanticSymbolInfo {
                                name: "this".to_string(),
                                symbol_type: Type::Object(class.name.clone()),
                                is_final: true,
                                is_initialized: true,
                            },
                        );

                        // 类型检查析构函数体
                        self.type_check_statement(
                            &Stmt::Block(dtor.body.clone()),
                            Some(&Type::Void),
                        )?;

                        self.symbol_table.exit_scope();
                    }
                    ClassMember::InstanceInitializer(block) => {
                        // 实例初始化块类型检查
                        self.current_method_is_static = false;
                        self.current_method_is_constructor = false;
                        self.symbol_table.enter_scope();
                        self.type_check_statement(&Stmt::Block(block.clone()), Some(&Type::Void))?;
                        self.symbol_table.exit_scope();
                    }
                    ClassMember::StaticInitializer(block) => {
                        // 静态初始化块类型检查
                        self.current_method_is_static = true;
                        self.current_method_is_constructor = false;
                        self.symbol_table.enter_scope();
                        self.type_check_statement(&Stmt::Block(block.clone()), Some(&Type::Void))?;
                        self.symbol_table.exit_scope();
                        self.current_method_is_static = false;
                    }
                }
            }

            self.current_class = None;
        }

        Ok(())
    }

    fn type_check_condition(&mut self, condition: &Expr, loc: &crate::error::SourceLocation) {
        let error_count_before = self.errors.len();
        let condition_type = self.infer_expr_type_collect_errors(condition);
        if self.errors.len() == error_count_before && condition_type != Type::Bool {
            self.errors.push(self.create_error_info_with_file(
                loc.file.clone(),
                loc.line,
                loc.column,
                format!("Condition expression must be bool, got {}", condition_type),
            ));
        }
    }

    /// 类型检查语句
    pub fn type_check_statement(
        &mut self,
        stmt: &Stmt,
        expected_return: Option<&Type>,
    ) -> cayResult<()> {
        match stmt {
            Stmt::Expr(expr) => {
                self.infer_expr_type_collect_errors(expr);
            }
            Stmt::VarDecl(var) => {
                // 检查当前作用域中是否已存在同名变量
                if self.symbol_table.lookup_current(&var.name).is_some() {
                    self.errors.push(self.create_error_info(
                        var.loc.line,
                        var.loc.column,
                        format!("Variable '{}' already defined in current scope", var.name),
                    ));
                    return Ok(());
                }

                // 对泛型类的变量类型进行参数替换
                let mut var_type = if self.current_class_type_params.is_empty() {
                    var.var_type.clone()
                } else {
                    self.replace_type_params(&var.var_type, &self.current_class_type_params)
                };
                let mut init_type_opt: Option<Type> = None;

                // 处理 auto 类型推断或类型检查（只分析初始化器一次）
                if let Some(init) = &var.initializer {
                    let init_type = self.infer_expr_type_collect_errors(init);
                    init_type_opt = Some(init_type.clone());

                    if var_type == Type::Auto {
                        // auto 类型推断：使用初始化器的类型
                        var_type = init_type;
                    } else {
                        // 非 auto：检查类型兼容性
                        if !self.types_compatible(&init_type, &var_type) {
                            self.errors.push(self.create_error_info_with_file(
                                var.loc.file.clone(),
                                var.loc.line,
                                var.loc.column,
                                format!("Cannot assign {} to {}", init_type, var_type),
                            ));
                        }
                    }
                } else if var_type == Type::Auto {
                    // auto 类型但没有初始化器
                    self.errors.push(self.create_error_info(
                        var.loc.line,
                        var.loc.column,
                        "'auto' variable declaration requires an initializer",
                    ));
                    var_type = Type::Int32; // 默认回退类型
                }

                self.symbol_table.declare(
                    var.name.clone(),
                    SemanticSymbolInfo {
                        name: var.name.clone(),
                        symbol_type: var_type,
                        is_final: var.is_final,
                        is_initialized: var.initializer.is_some(),
                    },
                );
            }
            Stmt::Return(expr) => {
                let return_type = if let Some(e) = expr {
                    self.infer_expr_type_collect_errors(e)
                } else {
                    Type::Void
                };

                // 自动推断返回类型（fn 关键字函数）
                let conflict_msg = if let Some(ref mut inferring) = self.current_inferring_return {
                    if *inferring == Type::Void {
                        // 第一次遇到 return，设置推断类型
                        *inferring = return_type.clone();
                        None
                    } else if *inferring != return_type {
                        let msg = format!(
                            "Conflicting return types: previous return was {}, but got {}",
                            inferring, return_type
                        );
                        *inferring = return_type.clone();
                        Some(msg)
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(msg) = conflict_msg {
                    let loc = if let Some(e) = expr {
                        self.get_expr_source_location(e)
                    } else {
                        crate::error::SourceLocation::new(self.current_file.clone(), 0, 0)
                    };
                    self.errors.push(self.create_error_info_with_file(
                        loc.file,
                        loc.line,
                        loc.column,
                        msg,
                    ));
                }

                if let Some(expected) = expected_return {
                    // fn 自动推断时，expected 为 Auto，跳过类型兼容性检查
                    if *expected != Type::Auto && !self.types_compatible(&return_type, expected) {
                        // 尝试从表达式获取位置信息
                        let loc = if let Some(e) = expr {
                            self.get_expr_source_location(e)
                        } else {
                            crate::error::SourceLocation::new(self.current_file.clone(), 0, 0)
                        };
                        self.errors.push(self.create_error_info_with_file(
                            loc.file,
                            loc.line,
                            loc.column,
                            format!(
                                "Return type mismatch: expected {}, got {}",
                                expected, return_type
                            ),
                        ));
                    }
                }
            }
            Stmt::Block(block) => {
                // 检查是否是多变量声明生成的块（只包含 VarDecl）
                let is_multi_var_decl = block
                    .statements
                    .iter()
                    .all(|s| matches!(s, Stmt::VarDecl(_)));
                if is_multi_var_decl {
                    // 多变量声明不创建新作用域，在当前作用域内声明所有变量
                    for stmt in &block.statements {
                        if let Stmt::VarDecl(var) = stmt {
                            self.type_check_statement(
                                &Stmt::VarDecl(var.clone()),
                                expected_return,
                            )?;
                        }
                    }
                } else {
                    self.symbol_table.enter_scope();
                    for stmt in &block.statements {
                        self.type_check_statement(stmt, expected_return)?;
                    }
                    self.symbol_table.exit_scope();
                }
            }
            Stmt::If(if_stmt) => {
                self.type_check_condition(&if_stmt.condition, &if_stmt.loc);
                self.type_check_statement(&if_stmt.then_branch, expected_return)?;
                if let Some(else_branch) = &if_stmt.else_branch {
                    self.type_check_statement(else_branch, expected_return)?;
                }
            }
            Stmt::While(while_stmt) => {
                self.type_check_condition(&while_stmt.condition, &while_stmt.loc);
                self.type_check_statement(&while_stmt.body, expected_return)?;
            }
            Stmt::For(for_stmt) => {
                self.symbol_table.enter_scope();
                if let Some(init) = &for_stmt.init {
                    self.type_check_for_init(init, expected_return)?;
                }
                if let Some(condition) = &for_stmt.condition {
                    self.type_check_condition(condition, &for_stmt.loc);
                }
                if let Some(update) = &for_stmt.update {
                    self.infer_expr_type_collect_errors(update);
                }
                self.type_check_statement(&for_stmt.body, expected_return)?;
                self.symbol_table.exit_scope();
            }
            Stmt::DoWhile(do_while_stmt) => {
                self.type_check_statement(&do_while_stmt.body, expected_return)?;
                self.type_check_condition(&do_while_stmt.condition, &do_while_stmt.loc);
            }
            Stmt::Switch(switch_stmt) => {
                self.infer_expr_type_collect_errors(&switch_stmt.expr);

                for case in &switch_stmt.cases {
                    self.symbol_table.enter_scope();
                    if let Some(binding) = &case.payload_binding {
                        self.symbol_table.declare(
                            binding.var_name.clone(),
                            SemanticSymbolInfo {
                                name: binding.var_name.clone(),
                                symbol_type: binding.var_type.clone(),
                                is_final: false,
                                is_initialized: true,
                            },
                        );
                    }
                    for stmt in &case.body {
                        self.type_check_statement(stmt, expected_return)?;
                    }
                    self.symbol_table.exit_scope();
                }

                if let Some(default_body) = &switch_stmt.default {
                    self.symbol_table.enter_scope();
                    for stmt in default_body {
                        self.type_check_statement(stmt, expected_return)?;
                    }
                    self.symbol_table.exit_scope();
                }
            }
            Stmt::Scope(scope_stmt) => {
                self.symbol_table.enter_scope();
                for stmt in &scope_stmt.body.statements {
                    self.type_check_statement(stmt, expected_return)?;
                }
                self.symbol_table.exit_scope();
            }
            _ => {}
        }

        Ok(())
    }

    /// 类型检查 for 循环初始化语句
    ///
    /// 与常规变量声明不同，for 初始化中的变量声明如果与外层作用域同名，
    /// 会复用外层变量而不是创建新变量（与代码生成行为一致）。
    fn type_check_for_init(
        &mut self,
        init: &Stmt,
        expected_return: Option<&Type>,
    ) -> cayResult<()> {
        match init {
            Stmt::VarDecl(var) => {
                // 先检查外层作用域是否已有同名变量
                let outer_exists = self.symbol_table.lookup(&var.name).is_some();

                if outer_exists {
                    // 复用外层变量：检查类型兼容性，然后更新符号表
                    let mut var_type = if self.current_class_type_params.is_empty() {
                        var.var_type.clone()
                    } else {
                        self.replace_type_params(&var.var_type, &self.current_class_type_params)
                    };

                    if let Some(init_expr) = &var.initializer {
                        let init_type = self.infer_expr_type_collect_errors(init_expr);

                        if var_type == Type::Auto {
                            var_type = init_type.clone();
                        } else if !self.types_compatible(&init_type, &var_type) {
                            self.errors.push(self.create_error_info_with_file(
                                var.loc.file.clone(),
                                var.loc.line,
                                var.loc.column,
                                format!("Cannot assign {} to {}", init_type, var_type),
                            ));
                        }
                    } else if var_type == Type::Auto {
                        self.errors.push(self.create_error_info(
                            var.loc.line,
                            var.loc.column,
                            "'auto' variable declaration requires an initializer",
                        ));
                        var_type = Type::Int32;
                    }

                    // 更新外层同名变量的类型和初始化状态
                    self.symbol_table.update(
                        &var.name,
                        SemanticSymbolInfo {
                            name: var.name.clone(),
                            symbol_type: var_type,
                            is_final: var.is_final,
                            is_initialized: var.initializer.is_some(),
                        },
                    );
                } else {
                    // 外层没有同名变量，按常规变量声明处理
                    // 但需要在 for 作用域中声明（当前已经在 for 作用域内）
                    if self.symbol_table.lookup_current(&var.name).is_some() {
                        self.errors.push(self.create_error_info(
                            var.loc.line,
                            var.loc.column,
                            format!("Variable '{}' already defined in current scope", var.name),
                        ));
                        return Ok(());
                    }

                    let mut var_type = if self.current_class_type_params.is_empty() {
                        var.var_type.clone()
                    } else {
                        self.replace_type_params(&var.var_type, &self.current_class_type_params)
                    };
                    let mut init_type_opt: Option<Type> = None;

                    if let Some(init_expr) = &var.initializer {
                        let init_type = self.infer_expr_type_collect_errors(init_expr);
                        init_type_opt = Some(init_type.clone());

                        if var_type == Type::Auto {
                            var_type = init_type;
                        } else if !self.types_compatible(&init_type, &var_type) {
                            self.errors.push(self.create_error_info_with_file(
                                var.loc.file.clone(),
                                var.loc.line,
                                var.loc.column,
                                format!("Cannot assign {} to {}", init_type, var_type),
                            ));
                        }
                    } else if var_type == Type::Auto {
                        self.errors.push(self.create_error_info(
                            var.loc.line,
                            var.loc.column,
                            "'auto' variable declaration requires an initializer",
                        ));
                        var_type = Type::Int32;
                    }

                    self.symbol_table.declare(
                        var.name.clone(),
                        SemanticSymbolInfo {
                            name: var.name.clone(),
                            symbol_type: var_type,
                            is_final: var.is_final,
                            is_initialized: var.initializer.is_some(),
                        },
                    );
                }
            }
            Stmt::Block(block) => {
                for stmt in &block.statements {
                    self.type_check_for_init(stmt, expected_return)?;
                }
            }
            _ => {
                self.type_check_statement(init, expected_return)?;
            }
        }
        Ok(())
    }
}
