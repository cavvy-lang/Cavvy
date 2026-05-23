//! 语义分析器核心实现

use crate::ast::*;
use crate::types::{Type, ParameterInfo, ClassInfo, MethodInfo, FieldInfo, TypeRegistry};
use crate::error::{cayResult, semantic_error_with_file};
use super::symbol_table::{SemanticSymbolTable, SemanticSymbolInfo};

/// 语义分析错误信息（包含位置）
#[derive(Debug, Clone)]
pub struct SemanticErrorInfo {
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub file: Option<String>, // 错误所在的文件路径
}

/// 语义分析器
pub struct SemanticAnalyzer {
    pub(super) program: Option<std::rc::Rc<Program>>,  // 保存 AST 以供类型推断使用
    pub(super) type_registry: TypeRegistry,
    pub(super) symbol_table: SemanticSymbolTable,
    pub(super) current_class: Option<String>,
    pub(super) current_method: Option<String>,
    pub(super) current_method_is_static: bool,  // 当前方法是否是静态方法
    pub(super) current_method_is_constructor: bool,  // 当前是否是构造函数
    pub(super) errors: Vec<SemanticErrorInfo>,
    pub(super) current_file: Option<String>,  // 当前正在分析的文件路径
    /// 源映射表：输出行号 -> (原始文件, 原始行号)
    /// 用于根据AST中的原始行号反查对应的源文件
    pub(super) source_map: Option<std::collections::HashMap<usize, (String, usize)>>,
    /// 启用的语言特性
    pub(super) features: Vec<String>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self::with_features(Vec::new())
    }

    pub fn with_features(features: Vec<String>) -> Self {
        let mut analyzer = Self {
            program: None,
            type_registry: TypeRegistry::new(),
            symbol_table: SemanticSymbolTable::new(),
            current_class: None,
            current_method: None,
            current_method_is_static: false,
            current_method_is_constructor: false,
            errors: Vec::new(),
            current_file: None,
            source_map: None,
            features,
        };
        
        // 注册内置函数
        analyzer.register_builtin_functions();
        
        analyzer
    }

    fn register_builtin_functions(&mut self) {
        // 注册 print 函数 - 作为特殊处理
        // print 可以接受任意类型参数
    }

    pub fn analyze(&mut self, program: &Program) -> cayResult<()> {
        // 保存 program 引用以供类型推断使用
        self.program = Some(std::rc::Rc::new(program.clone()));

        // 第一遍：收集所有类定义
        self.collect_classes(program)?;

        // 注册运行时函数到 NetworkUtils 类
        self.register_runtime_functions();

        // 注册顶层函数到符号表
        self.register_top_level_functions(program)?;

        // 检查主类冲突（在收集类之后，类型检查之前）
        self.check_main_class_conflicts(program)?;

        // 第二遍：分析方法定义
        self.analyze_methods(program)?;

        // 第三遍：检查继承关系（包括 @Override 验证）
        self.check_inheritance(program)?;

        // 第四遍：类型检查
        self.type_check_program(program)?;

        if !self.errors.is_empty() {
            // 将所有错误转换为 cayError 并返回 MultipleErrors
            let mut cay_errors: Vec<crate::error::cayError> = Vec::new();
            for err in &self.errors {
                cay_errors.push(crate::error::cayError::Semantic {
                    file: err.file.clone(),
                    line: err.line,
                    column: err.column,
                    message: err.message.clone(),
                    suggestion: "请检查代码语义".to_string(),
                });
            }
            return Err(crate::error::cayError::MultipleErrors { errors: cay_errors });
        }

        Ok(())
    }

    /// 注册运行时函数到相应的类
    fn register_runtime_functions(&mut self) {
        // 向 NetworkUtils 类添加 __cay_buffer_to_string 方法
        if let Some(class_info) = self.type_registry.get_class_mut("NetworkUtils") {
            // 创建方法信息: String __cay_buffer_to_string(long buffer, int length)
            let method = MethodInfo {
                name: "__cay_buffer_to_string".to_string(),
                class_name: "NetworkUtils".to_string(),
                params: vec![
                    ParameterInfo::new("buffer".to_string(), Type::Int64),
                    ParameterInfo::new("length".to_string(), Type::Int32),
                ],
                return_type: Type::String,
                is_public: true,
                is_private: false,
                is_protected: false,
                is_static: true,
                is_native: true,
                is_override: false,
                is_final: false,
            };

            class_info.add_method(method);
        }
    }

    /// 注册顶层函数到符号表
    fn register_top_level_functions(&mut self, program: &Program) -> cayResult<()> {
        use crate::semantic::symbol_table::SemanticSymbolInfo;

        // 检查是否启用了顶层函数特性
        let top_level_enabled = self.features.contains(&"top_level_function".to_string());

        // Cavvy默认是面向对象语言，不允许顶层函数（除了main函数）
        // 除非启用了 top_level_function 特性
        if !top_level_enabled {
            for func in &program.top_level_functions {
                if func.name != "main" {
                    return Err(crate::error::semantic_error_with_file(
                        func.loc.file.clone(),
                        func.loc.line,
                        func.loc.column,
                        format!("Cavvy是面向对象语言，不允许顶层函数 '{}'。请将函数定义在类中，或使用 -F=top_level_function 启用该特性。", func.name)
                    ));
                }
            }
        }

        for func in &program.top_level_functions {
            // 检查函数名是否已存在（在当前作用域）
            if self.symbol_table.lookup_current(&func.name).is_some() {
                return Err(crate::error::semantic_error_with_file(
                    func.loc.file.clone(),
                    func.loc.line,
                    func.loc.column,
                    format!("顶层函数 '{}' 已定义", func.name)
                ));
            }

            // 将顶层函数添加到符号表
            // 使用函数类型作为符号类型，参数和返回类型编码在类型中
            let func_type = Type::Function(Box::new(crate::types::FunctionType {
                params: func.params.iter().map(|p| p.param_type.clone()).collect(),
                return_type: Box::new(func.return_type.clone()),
                is_static: true,
            }));
            let symbol_info = SemanticSymbolInfo {
                name: func.name.clone(),
                symbol_type: func_type,
                is_final: true,
                is_initialized: true,
            };
            self.symbol_table.declare(func.name.clone(), symbol_info);
        }

        Ok(())
    }

    /// 获取类型注册表（用于代码生成）
    pub fn get_type_registry(&self) -> &TypeRegistry {
        &self.type_registry
    }

    /// 设置当前文件路径（用于错误报告）
    pub fn set_current_file(&mut self, file: Option<String>) {
        self.current_file = file;
    }

    /// 设置源映射表（用于多文件include场景下的正确错误定位）
    pub fn set_source_map(&mut self, source_map: std::collections::HashMap<usize, (String, usize)>) {
        self.source_map = Some(source_map);
    }

    /// 根据行号解析对应的源文件路径
    ///
    /// 逻辑：
    /// 1. source_map 结构: 预处理后的行号 -> (原始文件, 原始行号)
    /// 2. 用预处理后的行号（line参数）查找对应的原始文件
    /// 3. 如果找到，返回原始文件路径
    /// 4. 如果未找到，回退到 current_file
    fn resolve_file_for_line(&self, line: usize) -> Option<String> {
        if let Some(ref map) = self.source_map {
            // 用预处理后的行号查找对应的原始文件
            if let Some((file, _original_line)) = map.get(&line) {
                return Some(file.clone());
            }
        }
        self.current_file.clone()
    }

    /// 报告语义错误（自动包含当前文件信息）
    pub fn report_error(&self, line: usize, column: usize, message: impl Into<String>) -> crate::error::cayError {
        let msg = message.into();
        semantic_error_with_file(self.current_file.clone(), line, column, msg)
    }

    /// 创建语义分析错误信息（自动解析文件路径）
    pub fn create_error_info(&self, line: usize, column: usize, message: impl Into<String>) -> SemanticErrorInfo {
        let (file, original_line) = self.resolve_file_and_line(line);
        SemanticErrorInfo {
            line: original_line,
            column,
            message: message.into(),
            file,
        }
    }
    
    /// 创建语义分析错误信息（带文件路径）
    /// 注意：line 已经是原始行号，不需要再通过 source_map 转换
    pub fn create_error_info_with_file(&self, file: Option<String>, line: usize, column: usize, message: impl Into<String>) -> SemanticErrorInfo {
        // line 已经是原始行号，直接使用
        // file 如果为 None，则使用 current_file
        let resolved_file = file.or_else(|| self.current_file.clone());
        SemanticErrorInfo {
            line,
            column,
            message: message.into(),
            file: resolved_file,
        }
    }

    /// 根据行号解析文件路径
    /// 注意：line 已经是原始行号（来自 token.loc.line），不需要再通过 source_map 转换
    pub(super) fn resolve_file_and_line(&self, _line: usize) -> (Option<String>, usize) {
        // line 已经是原始行号，直接返回 current_file
        // source_map 用于词法分析阶段将预处理后的行号转换为原始行号
        // 在语义分析阶段，AST 节点中的行号已经是原始行号
        (self.current_file.clone(), _line)
    }

    /// 从表达式中提取源代码位置
    pub fn get_expr_location(&self, expr: &Expr) -> (usize, usize) {
        match expr {
            Expr::Literal(_) => (0, 0),
            Expr::Identifier(e) => (e.loc.line, e.loc.column),
            Expr::Binary(e) => (e.loc.line, e.loc.column),
            Expr::Unary(e) => (e.loc.line, e.loc.column),
            Expr::Call(e) => (e.loc.line, e.loc.column),
            Expr::MemberAccess(e) => (e.loc.line, e.loc.column),
            Expr::ArrayAccess(e) => (e.loc.line, e.loc.column),
            Expr::ArrayInit(e) => (e.loc.line, e.loc.column),
            Expr::New(e) => (e.loc.line, e.loc.column),
            Expr::Cast(e) => (e.loc.line, e.loc.column),
            Expr::Assignment(e) => (e.loc.line, e.loc.column),
            Expr::Ternary(e) => (e.loc.line, e.loc.column),
            Expr::Lambda(e) => (e.loc.line, e.loc.column),
            Expr::InstanceOf(e) => (e.loc.line, e.loc.column),
            Expr::ArrayCreation(e) => (e.loc.line, e.loc.column),
            Expr::MethodRef(e) => (e.loc.line, e.loc.column),
            Expr::Alloc(e) => (e.loc.line, e.loc.column),
            Expr::Dealloc(e) => (e.loc.line, e.loc.column),
        }
    }

    /// 报告语义错误并返回默认类型（用于表达式类型推断）
    /// 这样可以让分析继续，收集更多错误
    pub fn report_semantic_error(&mut self, line: usize, column: usize, message: impl Into<String>) -> Type {
        self.errors.push(self.create_error_info(line, column, message));
        Type::Int32 // 返回默认类型继续分析
    }

    /// 报告语义错误并返回默认类型（带文件路径）
    pub fn report_semantic_error_with_file(&mut self, file: Option<String>, line: usize, column: usize, message: impl Into<String>) -> Type {
        self.errors.push(self.create_error_info_with_file(file, line, column, message));
        Type::Int32 // 返回默认类型继续分析
    }
}
