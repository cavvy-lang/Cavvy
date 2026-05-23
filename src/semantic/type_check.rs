//! 类型检查实现

use crate::ast::*;
use crate::types::{Type, ParameterInfo};
use crate::error::cayResult;
use super::analyzer::SemanticAnalyzer;
use super::symbol_table::SemanticSymbolInfo;

impl SemanticAnalyzer {
    /// 类型检查程序
    pub fn type_check_program(&mut self, program: &Program) -> cayResult<()> {
        for class in &program.classes {
            self.current_class = Some(class.name.clone());
            
            for member in &class.members {
                match member {
                    ClassMember::Method(method) => {
                        self.current_method = Some(method.name.clone());
                        self.current_method_is_static = method.modifiers.contains(&Modifier::Static);
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
                                    }
                                );
                            }
                        }
                        
                        // 添加参数到符号表
                        for param in &method.params {
                            self.symbol_table.declare(
                                param.name.clone(),
                                SemanticSymbolInfo {
                                    name: param.name.clone(),
                                    symbol_type: param.param_type.clone(),
                                    is_final: false,
                                    is_initialized: true,
                                }
                            );
                        }
                        
                        // 类型检查方法体
                        if let Some(body) = &method.body {
                            self.type_check_statement(&Stmt::Block(body.clone()), Some(&method.return_type))?;
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
                            }
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
                                }
                            );
                        }
                        
                        // 类型检查构造函数体
                        self.type_check_statement(&Stmt::Block(ctor.body.clone()), Some(&Type::Void))?;
                        
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
                            }
                        );
                        
                        // 类型检查析构函数体
                        self.type_check_statement(&Stmt::Block(dtor.body.clone()), Some(&Type::Void))?;
                        
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

        // 类型检查顶层函数
        for func in &program.top_level_functions {
            self.current_class = None;  // 顶层函数不属于任何类
            self.current_method = Some(func.name.clone());
            self.current_method_is_static = true;  // 顶层函数都是静态的
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
                    }
                );
            }

            // 类型检查函数体
            self.type_check_statement(&Stmt::Block(func.body.clone()), Some(&func.return_type))?;

            self.symbol_table.exit_scope();
            self.current_method = None;
            self.current_method_is_static = false;
        }

        Ok(())
    }

    /// 类型检查语句
    pub fn type_check_statement(&mut self, stmt: &Stmt, expected_return: Option<&Type>) -> cayResult<()> {
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
                        format!(
                            "Variable '{}' already defined in current scope",
                            var.name
                        ),
                    ));
                    return Ok(());
                }

                let mut var_type = var.var_type.clone();
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
                                format!(
                                    "Cannot assign {} to {}",
                                    init_type, var_type
                                ),
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
                    }
                );
            }
            Stmt::Return(expr) => {
                let return_type = if let Some(e) = expr {
                    self.infer_expr_type_collect_errors(e)
                } else {
                    Type::Void
                };
                
                if let Some(expected) = expected_return {
                    if !self.types_compatible(&return_type, expected) {
                        // 尝试从表达式获取位置信息
                        let (line, column) = if let Some(e) = expr {
                            self.get_expr_location(e)
                        } else {
                            (0, 0)
                        };
                        self.errors.push(self.create_error_info(
                            line,
                            column,
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
                let is_multi_var_decl = block.statements.iter().all(|s| matches!(s, Stmt::VarDecl(_)));
                if is_multi_var_decl {
                    // 多变量声明不创建新作用域，在当前作用域内声明所有变量
                    for stmt in &block.statements {
                        if let Stmt::VarDecl(var) = stmt {
                            self.type_check_statement(&Stmt::VarDecl(var.clone()), expected_return)?;
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
            _ => {}
        }
        
        Ok(())
    }
}
