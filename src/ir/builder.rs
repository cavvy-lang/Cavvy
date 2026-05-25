//! IR Builder - 从 AST 构建 IR 模块
//!
//! 这是 AST 到 IR 的主要转换入口。
//! Builder 遍历 AST 并逐步构建 IrFunction 和 IrModule。

use super::types::IrType;
use super::value::{
    IrValue, IrInstruction, IrTerminator,
    IrBinaryOp, IrCastKind, IrCmpOp,
};
use super::block::IrBasicBlock;
use super::function::{IrFunction, IrParam};
use super::module::{
    IrModule, IrExternDecl,
};
use crate::ast::*;
use crate::types::{Type, TypeRegistry};
use crate::error::cayResult;
use std::collections::HashMap;

/// 循环上下文（用于 break/continue）
#[derive(Debug, Clone)]
struct LoopContext {
    cond_label: String,
    end_label: String,
    label: Option<String>,
}

/// 作用域管理器
#[derive(Debug, Clone, Default)]
struct ScopeManager {
    scopes: Vec<HashMap<String, IrValue>>,
    cay_types: HashMap<String, crate::types::Type>,
}

impl ScopeManager {
    fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            cay_types: HashMap::new(),
        }
    }

    fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn exit_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    fn declare(&mut self, name: &str, value: IrValue, cay_type: crate::types::Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), value);
        }
        self.cay_types.insert(name.to_string(), cay_type);
    }

    fn lookup(&self, name: &str) -> Option<&IrValue> {
        for scope in self.scopes.iter().rev() {
            if let Some(val) = scope.get(name) {
                return Some(val);
            }
        }
        None
    }

    fn get_cay_type(&self, name: &str) -> Option<&crate::types::Type> {
        self.cay_types.get(name)
    }

    fn reset(&mut self) {
        self.scopes.clear();
        self.scopes.push(HashMap::new());
        self.cay_types.clear();
    }

    /// 获取所有可见变量
    fn get_all_variables(&self) -> Vec<(String, IrValue)> {
        let mut result = Vec::new();
        let mut seen = std::collections::HashSet::new();
        
        for scope in self.scopes.iter().rev() {
            for (name, value) in scope.iter() {
                if seen.insert(name.clone()) {
                    result.push((name.clone(), value.clone()));
                }
            }
        }
        
        result
    }
}

/// IR Builder
///
/// 负责将 AST 转换为 IR 模块。
pub struct IrBuilder {
    /// 正在构建的模块
    module: IrModule,
    /// 当前正在构建的函数
    current_function: Option<IrFunction>,
    /// 当前类名（用于方法名生成）
    current_class: String,
    /// 类型注册表
    type_registry: Option<TypeRegistry>,
    /// 作用域管理器
    scope_manager: ScopeManager,
    /// 循环上下文栈
    loop_stack: Vec<LoopContext>,
    /// 临时计数器
    temp_counter: u32,
    /// 标签计数器
    label_counter: u32,
    /// 类字段布局 (类名 -> (字段名 -> 偏移量))
    class_layouts: HashMap<String, HashMap<String, usize>>,
    /// 类总大小
    class_sizes: HashMap<String, usize>,
}

impl IrBuilder {
    /// 创建新的 IR Builder
    pub fn new() -> Self {
        Self {
            module: IrModule::new("module".to_string(), "x86_64-w64-mingw32".to_string()),
            current_function: None,
            current_class: String::new(),
            type_registry: None,
            scope_manager: ScopeManager::new(),
            loop_stack: Vec::new(),
            temp_counter: 0,
            label_counter: 0,
            class_layouts: HashMap::new(),
            class_sizes: HashMap::new(),
        }
    }

    /// 设置类型注册表
    pub fn set_type_registry(&mut self, registry: TypeRegistry) {
        self.type_registry = Some(registry);
    }

    /// 从 AST 构建 IR 模块
    pub fn build_from_ast(&mut self, program: &Program) -> cayResult<IrModule> {
        // 重置状态
        self.module = IrModule::new("module".to_string(), "x86_64-w64-mingw32".to_string());

        // 计算类布局
        self.compute_class_layouts(program);

        // 生成外部声明
        for extern_decl in &program.extern_declarations {
            self.build_extern_decl(extern_decl);
        }

        // 生成顶层函数
        for func in &program.top_level_functions {
            self.build_top_level_function(func)?;
        }

        // 生成类和接口
        for class in &program.classes {
            self.build_class(class)?;
        }

        Ok(self.module.clone())
    }

    /// 计算所有类的实例布局
    fn compute_class_layouts(&mut self, program: &Program) {
        let mut computed = std::collections::HashSet::new();
        let classes: HashMap<String, &ClassDecl> = program.classes.iter()
            .map(|c| (c.name.clone(), c))
            .collect();

        fn compute_recursive(
            class: &ClassDecl,
            classes: &HashMap<String, &ClassDecl>,
            computed: &mut std::collections::HashSet<String>,
            layouts: &mut HashMap<String, HashMap<String, usize>>,
            sizes: &mut HashMap<String, usize>,
            type_registry: &Option<TypeRegistry>,
        ) {
            if computed.contains(&class.name) {
                return;
            }

            // 先计算父类
            if let Some(ref parent_name) = class.parent {
                if let Some(parent_class) = classes.get(parent_name) {
                    compute_recursive(parent_class, classes, computed, layouts, sizes, type_registry);
                }
            }

            let header_size = 8usize; // type_id (4) + padding (4)
            let mut offset = header_size;

            // 从父类继承字段偏移
            if let Some(ref parent_name) = class.parent {
                if let Some(_parent_layout) = layouts.get(parent_name) {
                    if let Some(&parent_size) = sizes.get(parent_name) {
                        offset = parent_size;
                    }
                }
            }

            let mut field_map = if let Some(ref parent_name) = class.parent {
                layouts.get(parent_name).cloned().unwrap_or_default()
            } else {
                HashMap::new()
            };

            for member in &class.members {
                if let ClassMember::Field(field) = member {
                    if field.modifiers.contains(&Modifier::Static) {
                        continue;
                    }
                    let size = field.field_type.size_in_bytes();
                    let ir_ty = IrType::from(&field.field_type);
                    let align = ir_ty.alignment();
                    offset = (offset + align - 1) & !(align - 1);
                    field_map.insert(field.name.clone(), offset);
                    offset += size;
                }
            }

            let total_size = (offset + 7) & !7;
            layouts.insert(class.name.clone(), field_map);
            sizes.insert(class.name.clone(), total_size);
            computed.insert(class.name.clone());
        }

        for class in &program.classes {
            compute_recursive(
                class, &classes, &mut computed,
                &mut self.class_layouts, &mut self.class_sizes,
                &self.type_registry,
            );
        }
    }

    /// 生成新的临时寄存器
    fn new_temp(&mut self, ty: IrType) -> IrValue {
        let name = format!("%t{}", self.temp_counter);
        self.temp_counter += 1;
        IrValue::Register(name, ty)
    }

    /// 生成新的标签
    fn new_label(&mut self, prefix: &str) -> String {
        let label = format!("{}.{}", prefix, self.label_counter);
        self.label_counter += 1;
        label
    }

    /// 获取当前基本块（可变引用）
    fn current_block_mut(&mut self) -> Option<&mut IrBasicBlock> {
        self.current_function.as_mut()?.current_block_mut()
    }

    fn current_block(&self) -> Option<&IrBasicBlock> {
        self.current_function.as_ref()?.current_block()
    }

    /// 在当前基本块中添加指令
    fn emit(&mut self, inst: IrInstruction) -> cayResult<()> {
        let block = self.current_block_mut()
            .ok_or_else(|| crate::error::codegen_error("No current block".to_string()))?;
        block.push(inst);
        Ok(())
    }

    /// 设置当前块的终止指令
    fn set_terminator(&mut self, term: IrTerminator) -> cayResult<()> {
        let block = self.current_block_mut()
            .ok_or_else(|| crate::error::codegen_error("No current block".to_string()))?;
        block.set_terminator(term);
        Ok(())
    }

    /// 创建新基本块并设置为当前块
    fn new_block(&mut self, label: String) -> cayResult<()> {
        let func = self.current_function.as_mut()
            .ok_or_else(|| crate::error::codegen_error("No current function".to_string()))?;
        func.add_block(IrBasicBlock::new(label));
        Ok(())
    }

    // ============================================================
    // 外部声明
    // ============================================================

    fn build_extern_decl(&mut self, decl: &ExternDecl) {
        for func in &decl.functions {
            let return_type = IrType::from(&func.return_type);
            let params: Vec<(String, IrType)> = func.params.iter()
                .map(|p| (p.name.clone(), IrType::from(&p.param_type)))
                .collect();
            self.module.add_extern(IrExternDecl {
                name: func.name.clone(),
                return_type,
                params,
                calling_convention: None,
                is_varargs: func.params.iter().any(|p| p.is_varargs),
            });
        }
    }

    // ============================================================
    // 顶层函数
    // ============================================================

    fn build_top_level_function(&mut self, func: &TopLevelFunction) -> cayResult<()> {
        let fn_name = format!("__toplevel_{}", func.name);
        let return_type = IrType::from(&func.return_type);
        let params: Vec<IrParam> = func.params.iter()
            .map(|p| IrParam {
                name: p.name.clone(),
                ty: IrType::from(&p.param_type),
            })
            .collect();

        self.current_function = Some(IrFunction::new(fn_name, return_type, params));
        self.current_class = String::new();
        self.scope_manager.reset();
        self.loop_stack.clear();
        self.temp_counter = 0;
        self.label_counter = 0;

        // 在入口块为参数创建 alloca
        for param in &func.params {
            let ir_ty = IrType::from(&param.param_type);
            let alloca = self.new_temp(IrType::Pointer(Box::new(ir_ty.clone())));
            self.emit(IrInstruction::Alloca {
                result: alloca.clone(),
                ty: ir_ty.clone(),
                align: ir_ty.alignment() as u32,
            })?;
            let param_val = IrValue::Param(param.name.clone(), ir_ty.clone());
            self.emit(IrInstruction::Store {
                value: param_val,
                ptr: alloca.clone(),
                ty: ir_ty.clone(),
            })?;
            self.scope_manager.declare(&param.name, alloca, param.param_type.clone());
        }

        // 生成函数体
        self.build_block(&func.body)?;

        // 确保函数以 return 结束
        if func.return_type == Type::Void {
            self.set_terminator(IrTerminator::Return { value: None })?;
        }

        let func_ir = self.current_function.take().unwrap();
        self.module.add_function(func_ir);
        Ok(())
    }

    // ============================================================
    // 类
    // ============================================================

    fn build_class(&mut self, class: &ClassDecl) -> cayResult<()> {
        self.current_class = class.name.clone();

        for member in &class.members {
            match member {
                ClassMember::Method(method) => {
                    if !method.modifiers.contains(&Modifier::Native)
                        && !method.modifiers.contains(&Modifier::Abstract)
                    {
                        self.build_method(&class.name, method)?;
                    }
                }
                ClassMember::Constructor(ctor) => {
                    self.build_constructor(&class.name, ctor)?;
                }
                ClassMember::Destructor(dtor) => {
                    self.build_destructor(&class.name, dtor)?;
                }
                ClassMember::StaticInitializer(block) => {
                    self.build_static_init(&class.name, block)?;
                }
                _ => {}
            }
        }

        // 如果没有显式构造函数，生成默认构造函数
        let has_ctor = class.members.iter().any(|m| matches!(m, ClassMember::Constructor(_)));
        if !has_ctor {
            self.build_default_constructor(&class.name)?;
        }

        Ok(())
    }

    fn build_method(&mut self, class_name: &str, method: &MethodDecl) -> cayResult<()> {
        let fn_name = self.generate_method_name(class_name, method);
        let return_type = IrType::from(&method.return_type);
        let is_static = method.modifiers.contains(&Modifier::Static);

        let mut params: Vec<IrParam> = Vec::new();

        // 实例方法添加 this 参数
        if !is_static {
            params.push(IrParam {
                name: "this".to_string(),
                ty: IrType::Pointer(Box::new(IrType::I8)),
            });
        }

        for param in &method.params {
            let ir_ty = if param.is_varargs {
                IrType::Pointer(Box::new(IrType::I8))
            } else {
                IrType::from(&param.param_type)
            };
            params.push(IrParam {
                name: param.name.clone(),
                ty: ir_ty,
            });
        }

        self.current_function = Some(IrFunction::new(fn_name, return_type, params));
        self.current_function.as_mut().unwrap().is_static = is_static;
        self.scope_manager.reset();
        self.loop_stack.clear();
        self.temp_counter = 0;
        self.label_counter = 0;

        // 为参数创建 alloca
        if !is_static {
            let alloca = self.new_temp(IrType::Pointer(Box::new(IrType::Pointer(Box::new(IrType::I8)))));
            self.emit(IrInstruction::Alloca {
                result: alloca.clone(),
                ty: IrType::Pointer(Box::new(IrType::I8)),
                align: 8,
            })?;
            let this_val = IrValue::Param("this".to_string(), IrType::Pointer(Box::new(IrType::I8)));
            self.emit(IrInstruction::Store {
                value: this_val,
                ptr: alloca.clone(),
                ty: IrType::Pointer(Box::new(IrType::I8)),
            })?;
            self.scope_manager.declare("this", alloca, Type::Object(class_name.to_string()));
        }

        for param in &method.params {
            let ir_ty = if param.is_varargs {
                IrType::Pointer(Box::new(IrType::I8))
            } else {
                IrType::from(&param.param_type)
            };
            let alloca = self.new_temp(IrType::Pointer(Box::new(ir_ty.clone())));
            self.emit(IrInstruction::Alloca {
                result: alloca.clone(),
                ty: ir_ty.clone(),
                align: ir_ty.alignment() as u32,
            })?;
            let param_val = IrValue::Param(param.name.clone(), ir_ty.clone());
            self.emit(IrInstruction::Store {
                value: param_val,
                ptr: alloca.clone(),
                ty: ir_ty.clone(),
            })?;
            self.scope_manager.declare(&param.name, alloca, param.param_type.clone());
        }

        // 生成方法体
        if let Some(body) = method.body.as_ref() {
            self.build_block(body)?;
        }

        // 确保函数以 return 结束
        if method.return_type == Type::Void {
            self.set_terminator(IrTerminator::Return { value: None })?;
        }

        let func_ir = self.current_function.take().unwrap();
        self.module.add_function(func_ir);
        Ok(())
    }

    fn build_constructor(&mut self, class_name: &str, ctor: &ConstructorDecl) -> cayResult<()> {
        let fn_name = self.generate_constructor_name(class_name, ctor);

        let mut params: Vec<IrParam> = vec![
            IrParam { name: "this".to_string(), ty: IrType::Pointer(Box::new(IrType::I8)) }
        ];
        for param in &ctor.params {
            params.push(IrParam {
                name: param.name.clone(),
                ty: IrType::from(&param.param_type),
            });
        }

        self.current_function = Some(IrFunction::new(fn_name, IrType::Void, params));
        self.scope_manager.reset();
        self.loop_stack.clear();
        self.temp_counter = 0;
        self.label_counter = 0;

        // this 参数
        let alloca = self.new_temp(IrType::Pointer(Box::new(IrType::Pointer(Box::new(IrType::I8)))));
        self.emit(IrInstruction::Alloca {
            result: alloca.clone(),
            ty: IrType::Pointer(Box::new(IrType::I8)),
            align: 8,
        })?;
        let this_val = IrValue::Param("this".to_string(), IrType::Pointer(Box::new(IrType::I8)));
        self.emit(IrInstruction::Store {
            value: this_val,
            ptr: alloca.clone(),
            ty: IrType::Pointer(Box::new(IrType::I8)),
        })?;
        self.scope_manager.declare("this", alloca, Type::Object(class_name.to_string()));

        for param in &ctor.params {
            let ir_ty = IrType::from(&param.param_type);
            let alloca = self.new_temp(IrType::Pointer(Box::new(ir_ty.clone())));
            self.emit(IrInstruction::Alloca {
                result: alloca.clone(),
                ty: ir_ty.clone(),
                align: ir_ty.alignment() as u32,
            })?;
            let param_val = IrValue::Param(param.name.clone(), ir_ty.clone());
            self.emit(IrInstruction::Store {
                value: param_val,
                ptr: alloca.clone(),
                ty: ir_ty.clone(),
            })?;
            self.scope_manager.declare(&param.name, alloca, param.param_type.clone());
        }

        // 处理 constructor_call (this()/super())
        if let Some(ref _call) = ctor.constructor_call {
            // TODO: 实现构造函数委托调用
        }

        self.build_block(&ctor.body)?;
        self.set_terminator(IrTerminator::Return { value: None })?;

        let func_ir = self.current_function.take().unwrap();
        self.module.add_function(func_ir);
        Ok(())
    }

    fn build_destructor(&mut self, class_name: &str, dtor: &DestructorDecl) -> cayResult<()> {
        let fn_name = format!("{}.__dtor", class_name);

        let params = vec![
            IrParam { name: "this".to_string(), ty: IrType::Pointer(Box::new(IrType::I8)) }
        ];

        self.current_function = Some(IrFunction::new(fn_name, IrType::Void, params));
        self.scope_manager.reset();
        self.loop_stack.clear();
        self.temp_counter = 0;
        self.label_counter = 0;

        let alloca = self.new_temp(IrType::Pointer(Box::new(IrType::Pointer(Box::new(IrType::I8)))));
        self.emit(IrInstruction::Alloca {
            result: alloca.clone(),
            ty: IrType::Pointer(Box::new(IrType::I8)),
            align: 8,
        })?;
        self.emit(IrInstruction::Store {
            value: IrValue::Param("this".to_string(), IrType::Pointer(Box::new(IrType::I8))),
            ptr: alloca.clone(),
            ty: IrType::Pointer(Box::new(IrType::I8)),
        })?;
        self.scope_manager.declare("this", alloca, Type::Object(class_name.to_string()));

        self.build_block(&dtor.body)?;
        self.set_terminator(IrTerminator::Return { value: None })?;

        let func_ir = self.current_function.take().unwrap();
        self.module.add_function(func_ir);
        Ok(())
    }

    fn build_default_constructor(&mut self, class_name: &str) -> cayResult<()> {
        let fn_name = format!("{}.__ctor", class_name);

        let params = vec![
            IrParam { name: "this".to_string(), ty: IrType::Pointer(Box::new(IrType::I8)) }
        ];

        self.current_function = Some(IrFunction::new(fn_name, IrType::Void, params));
        self.scope_manager.reset();
        self.loop_stack.clear();
        self.temp_counter = 0;
        self.label_counter = 0;

        let alloca = self.new_temp(IrType::Pointer(Box::new(IrType::Pointer(Box::new(IrType::I8)))));
        self.emit(IrInstruction::Alloca {
            result: alloca.clone(),
            ty: IrType::Pointer(Box::new(IrType::I8)),
            align: 8,
        })?;
        self.emit(IrInstruction::Store {
            value: IrValue::Param("this".to_string(), IrType::Pointer(Box::new(IrType::I8))),
            ptr: alloca.clone(),
            ty: IrType::Pointer(Box::new(IrType::I8)),
        })?;
        self.scope_manager.declare("this", alloca, Type::Object(class_name.to_string()));

        self.set_terminator(IrTerminator::Return { value: None })?;

        let func_ir = self.current_function.take().unwrap();
        self.module.add_function(func_ir);
        Ok(())
    }

    fn build_static_init(&mut self, class_name: &str, block: &Block) -> cayResult<()> {
        let fn_name = format!("{}.__static_init", class_name);

        self.current_function = Some(IrFunction::new(fn_name, IrType::Void, Vec::new()));
        self.scope_manager.reset();
        self.loop_stack.clear();
        self.temp_counter = 0;
        self.label_counter = 0;

        self.build_block(block)?;
        self.set_terminator(IrTerminator::Return { value: None })?;

        let func_ir = self.current_function.take().unwrap();
        self.module.add_function(func_ir);
        Ok(())
    }

    // ============================================================
    // 语句块
    // ============================================================

    fn build_block(&mut self, block: &Block) -> cayResult<()> {
        self.scope_manager.enter_scope();
        // eprintln!("DEBUG: build_block with {} statements", block.statements.len());
        for (i, stmt) in block.statements.iter().enumerate() {
            // eprintln!("DEBUG:  Statement {}: {:?}", i, std::mem::discriminant(stmt));
            self.build_statement(stmt)?;
        }
        self.scope_manager.exit_scope();
        Ok(())
    }

    fn build_statement(&mut self, stmt: &Stmt) -> cayResult<()> {
        match stmt {
            Stmt::Expr(expr) => { self.build_expression(expr)?; }
            Stmt::VarDecl(var) => self.build_var_decl(var)?,
            Stmt::Return(expr) => self.build_return(expr)?,
            Stmt::Block(block) => {
                let is_multi_var = block.statements.iter().all(|s| matches!(s, Stmt::VarDecl(_)));
                if is_multi_var {
                    for s in &block.statements {
                        if let Stmt::VarDecl(v) = s {
                            self.build_var_decl(v)?;
                        }
                    }
                } else {
                    self.build_block(block)?;
                }
            }
            Stmt::If(if_stmt) => self.build_if(if_stmt)?,
            Stmt::While(while_stmt) => self.build_while(while_stmt)?,
            Stmt::For(for_stmt) => self.build_for(for_stmt)?,
            Stmt::DoWhile(do_while) => self.build_do_while(do_while)?,
            Stmt::Switch(switch) => self.build_switch(switch)?,
            Stmt::Scope(scope) => {
                self.scope_manager.enter_scope();
                for s in &scope.body.statements {
                    self.build_statement(s)?;
                }
                self.scope_manager.exit_scope();
            }
            Stmt::Break(label) => self.build_break(label)?,
            Stmt::Continue(label) => self.build_continue(label)?,
            Stmt::InlineIr(inline_ir) => self.build_inline_ir(inline_ir)?,
        }
        Ok(())
    }

    /// 构建内联IR语句
    fn build_inline_ir(&mut self, inline_ir: &InlineIrStmt) -> cayResult<()> {
        use super::inline_ir::InlineIrParser;
        
        // 调试：检查raw_lines
        // eprintln!("DEBUG: Inline IR raw_lines count: {}", inline_ir.raw_lines.len());
        for (i, line) in inline_ir.raw_lines.iter().enumerate() {
            // eprintln!("DEBUG: Line {}: '{}'", i, line);
        }
        
        if inline_ir.raw_lines.is_empty() {
            return Err(crate::error::cayError::CodeGen {
                code: "E5004".to_string(),
                file: None,
                line: 0,
                column: 0,
                message: "Inline IR block has no lines".to_string(),
                suggestion: "Check parser implementation".to_string(),
            });
        }
        
        // 创建内联IR解析器
        let parser = InlineIrParser::new();
        
        // 收集可用的输入变量
        let mut inputs = Vec::new();
        for (name, value) in self.scope_manager.get_all_variables() {
            inputs.push((name, value));
        }
        
        // 解析IR文本
        let raw_text = inline_ir.raw_lines.join("\n");
        let block = parser.parse(&raw_text, &inputs, &[])
            .map_err(|e| crate::error::cayError::CodeGen { 
                code: "E5004".to_string(),
                file: None,
                line: 0,
                column: 0,
                message: format!("Inline IR error: {}", e),
                suggestion: "Check your inline IR syntax".to_string(),
            })?;
        
        // 将内联IR块转换为IR指令
        let inst = parser.to_instruction(&block);
        self.emit(inst)?;
        
        Ok(())
    }

    // ============================================================
    // 变量声明
    // ============================================================

    fn build_var_decl(&mut self, var: &VarDecl) -> cayResult<()> {
        let actual_type = if var.var_type == Type::Auto {
            self.infer_type_from_expr(var.initializer.as_ref())?
        } else {
            var.var_type.clone()
        };

        let ir_ty = IrType::from(&actual_type);
        let alloca = self.new_temp(IrType::Pointer(Box::new(ir_ty.clone())));
        self.emit(IrInstruction::Alloca {
            result: alloca.clone(),
            ty: ir_ty.clone(),
            align: ir_ty.alignment() as u32,
        })?;

        if let Some(init) = var.initializer.as_ref() {
            let value = self.build_expression(init)?;
            self.emit(IrInstruction::Store {
                value: value.clone(),
                ptr: alloca.clone(),
                ty: ir_ty.clone(),
            })?;
        }

        self.scope_manager.declare(&var.name, alloca, actual_type);
        Ok(())
    }

    // ============================================================
    // 控制流
    // ============================================================

    fn build_if(&mut self, if_stmt: &IfStmt) -> cayResult<()> {
        let then_label = self.new_label("then");
        let else_label = self.new_label("else");
        let merge_label = self.new_label("ifmerge");
        let has_else = if_stmt.else_branch.is_some();

        // 生成条件
        let cond = self.build_expression(&if_stmt.condition)?;
        let cond_val = self.new_temp(IrType::I1);
        self.emit(IrInstruction::Compare {
            result: cond_val.clone(),
            op: IrCmpOp::Ne,
            left: cond.clone(),
            right: IrValue::IntConst(0, IrType::I32),
        })?;

        if has_else {
            self.set_terminator(IrTerminator::ConditionalBranch {
                condition: cond_val,
                true_target: then_label.clone(),
                false_target: else_label.clone(),
            })?;
        } else {
            self.set_terminator(IrTerminator::ConditionalBranch {
                condition: cond_val,
                true_target: then_label.clone(),
                false_target: merge_label.clone(),
            })?;
        }

        // then 块
        self.new_block(then_label)?;
        self.build_statement(&if_stmt.then_branch)?;
        // 仅当块没有 terminator 时才添加分支
        let then_returns = self.current_block_is_complete();
        if !then_returns {
            self.set_terminator(IrTerminator::Branch { target: merge_label.clone() })?;
        }

        // else 块
        let mut else_returns = false;
        if has_else {
            self.new_block(else_label)?;
            self.build_statement(if_stmt.else_branch.as_ref().unwrap())?;
            else_returns = self.current_block_is_complete();
            if !else_returns {
                self.set_terminator(IrTerminator::Branch { target: merge_label.clone() })?;
            }
        }

        // merge 块（仅当至少有一个分支会跳转到它时才创建）
        if !then_returns || (has_else && !else_returns) || !has_else {
            self.new_block(merge_label)?;
        }
        Ok(())
    }

    /// 检查当前块是否已有 terminator
    fn current_block_is_complete(&self) -> bool {
        self.current_block()
            .map(|b| b.is_complete())
            .unwrap_or(true)
    }

    /// 检查块是否存在
    fn block_exists(&self, label: &str) -> bool {
        self.current_function
            .as_ref()
            .map(|f| f.blocks.iter().any(|b| b.label == label))
            .unwrap_or(false)
    }

    fn build_while(&mut self, while_stmt: &WhileStmt) -> cayResult<()> {
        let cond_label = self.new_label("while.cond");
        let body_label = self.new_label("while.body");
        let end_label = self.new_label("while.end");

        self.loop_stack.push(LoopContext {
            cond_label: cond_label.clone(),
            end_label: end_label.clone(),
            label: while_stmt.label.clone(),
        });

        self.set_terminator(IrTerminator::Branch { target: cond_label.clone() })?;

        // 条件块
        self.new_block(cond_label.clone())?;
        let cond = self.build_expression(&while_stmt.condition)?;
        let cond_val = self.new_temp(IrType::I1);
        self.emit(IrInstruction::Compare {
            result: cond_val.clone(),
            op: IrCmpOp::Ne,
            left: cond,
            right: IrValue::IntConst(0, IrType::I32),
        })?;
        self.set_terminator(IrTerminator::ConditionalBranch {
            condition: cond_val,
            true_target: body_label.clone(),
            false_target: end_label.clone(),
        })?;

        // 循环体
        self.new_block(body_label)?;
        self.build_statement(&while_stmt.body)?;
        self.set_terminator(IrTerminator::Branch { target: cond_label })?;

        // 结束块
        self.new_block(end_label)?;
        self.loop_stack.pop();
        Ok(())
    }

    fn build_for(&mut self, for_stmt: &ForStmt) -> cayResult<()> {
        let cond_label = self.new_label("for.cond");
        let body_label = self.new_label("for.body");
        let update_label = self.new_label("for.update");
        let end_label = self.new_label("for.end");

        // 初始化
        if let Some(init) = for_stmt.init.as_ref() {
            self.build_statement(init)?;
        }

        self.loop_stack.push(LoopContext {
            cond_label: update_label.clone(),
            end_label: end_label.clone(),
            label: for_stmt.label.clone(),
        });

        self.set_terminator(IrTerminator::Branch { target: cond_label.clone() })?;

        // 条件块
        self.new_block(cond_label.clone())?;
        if let Some(condition) = for_stmt.condition.as_ref() {
            let cond = self.build_expression(condition)?;
            let cond_val = self.new_temp(IrType::I1);
            self.emit(IrInstruction::Compare {
                result: cond_val.clone(),
                op: IrCmpOp::Ne,
                left: cond,
                right: IrValue::IntConst(0, IrType::I32),
            })?;
            self.set_terminator(IrTerminator::ConditionalBranch {
                condition: cond_val,
                true_target: body_label.clone(),
                false_target: end_label.clone(),
            })?;
        } else {
            self.set_terminator(IrTerminator::Branch { target: body_label.clone() })?;
        }

        // 循环体
        self.new_block(body_label)?;
        self.build_statement(&for_stmt.body)?;
        self.set_terminator(IrTerminator::Branch { target: update_label.clone() })?;

        // 更新块
        self.new_block(update_label)?;
        if let Some(update) = for_stmt.update.as_ref() {
            self.build_expression(update)?;
        }
        self.set_terminator(IrTerminator::Branch { target: cond_label })?;

        // 结束块
        self.new_block(end_label)?;
        self.loop_stack.pop();
        Ok(())
    }

    fn build_do_while(&mut self, do_while: &DoWhileStmt) -> cayResult<()> {
        let body_label = self.new_label("dowhile.body");
        let cond_label = self.new_label("dowhile.cond");
        let end_label = self.new_label("dowhile.end");

        self.loop_stack.push(LoopContext {
            cond_label: cond_label.clone(),
            end_label: end_label.clone(),
            label: do_while.label.clone(),
        });

        self.set_terminator(IrTerminator::Branch { target: body_label.clone() })?;

        // 循环体
        self.new_block(body_label.clone())?;
        self.build_statement(&do_while.body)?;
        self.set_terminator(IrTerminator::Branch { target: cond_label.clone() })?;

        // 条件
        self.new_block(cond_label)?;
        let cond = self.build_expression(&do_while.condition)?;
        let cond_val = self.new_temp(IrType::I1);
        self.emit(IrInstruction::Compare {
            result: cond_val.clone(),
            op: IrCmpOp::Ne,
            left: cond,
            right: IrValue::IntConst(0, IrType::I32),
        })?;
        self.set_terminator(IrTerminator::ConditionalBranch {
            condition: cond_val,
            true_target: body_label.clone(),
            false_target: end_label.clone(),
        })?;

        self.new_block(end_label)?;
        self.loop_stack.pop();
        Ok(())
    }

    fn build_switch(&mut self, switch: &SwitchStmt) -> cayResult<()> {
        let end_label = self.new_label("switch.end");
        let default_label = self.new_label("switch.default");
        let mut case_labels: Vec<(i64, String)> = Vec::new();

        for (i, case) in switch.cases.iter().enumerate() {
            case_labels.push((case.value, self.new_label(&format!("switch.case{}", i))));
        }

        // 将 switch 结束标签压入栈，支持 break
        self.loop_stack.push(LoopContext {
            cond_label: end_label.clone(),
            end_label: end_label.clone(),
            label: None,
        });

        let value = self.build_expression(&switch.expr)?;

        // 生成 switch 跳转（使用条件跳转链）
        // 从第一个 case 开始，依次比较
        if case_labels.is_empty() {
            // 没有 case，直接跳转到 default
            self.set_terminator(IrTerminator::Branch { target: default_label.clone() })?;
        } else {
            // 创建第一个比较块
            let first_cmp_label = self.new_label("switch.cmp");
            self.set_terminator(IrTerminator::Branch { target: first_cmp_label.clone() })?;
            self.new_block(first_cmp_label.clone())?;

            // 生成比较链
            for (i, (case_val, case_label)) in case_labels.iter().enumerate() {
                let eq_val = self.new_temp(IrType::I1);
                self.emit(IrInstruction::Compare {
                    result: eq_val.clone(),
                    op: IrCmpOp::Eq,
                    left: value.clone(),
                    right: IrValue::IntConst(*case_val, IrType::I32),
                })?;

                // 确定 false 目标（下一个比较或 default）
                let false_target = if i + 1 < case_labels.len() {
                    self.new_label(&format!("switch.cmp{}", i + 1))
                } else {
                    default_label.clone()
                };

                self.set_terminator(IrTerminator::ConditionalBranch {
                    condition: eq_val,
                    true_target: case_label.clone(),
                    false_target: false_target.clone(),
                })?;

                // 创建下一个比较块（如果不是最后一个）
                if i + 1 < case_labels.len() {
                    self.new_block(false_target)?;
                }
            }
        }

        // default 块
        self.new_block(default_label)?;
        if let Some(default_stmts) = &switch.default {
            for stmt in default_stmts {
                self.build_statement(stmt)?;
            }
        }
        if !self.current_block_is_complete() {
            self.set_terminator(IrTerminator::Branch { target: end_label.clone() })?;
        }

        // 各 case 块
        for (i, case) in switch.cases.iter().enumerate() {
            let label = &case_labels[i].1;
            self.new_block(label.clone())?;
            for stmt in &case.body {
                self.build_statement(stmt)?;
            }
            if !self.current_block_is_complete() {
                self.set_terminator(IrTerminator::Branch { target: end_label.clone() })?;
            }
        }

        self.new_block(end_label)?;
        self.loop_stack.pop(); // 弹出 switch 上下文
        Ok(())
    }

    fn build_break(&mut self, label: &Option<String>) -> cayResult<()> {
        let target = if let Some(l) = label {
            self.loop_stack.iter().rev()
                .find(|ctx| ctx.label.as_deref() == Some(l.as_str()))
                .map(|ctx| ctx.end_label.clone())
                .ok_or_else(|| crate::error::codegen_error(format!("break label '{}' not found", l)))?
        } else {
            self.loop_stack.last()
                .map(|ctx| ctx.end_label.clone())
                .ok_or_else(|| crate::error::codegen_error("break outside loop".to_string()))?
        };
        self.set_terminator(IrTerminator::Branch { target })?;
        Ok(())
    }

    fn build_continue(&mut self, label: &Option<String>) -> cayResult<()> {
        let target = if let Some(l) = label {
            self.loop_stack.iter().rev()
                .find(|ctx| ctx.label.as_deref() == Some(l.as_str()))
                .map(|ctx| ctx.cond_label.clone())
                .ok_or_else(|| crate::error::codegen_error(format!("continue label '{}' not found", l)))?
        } else {
            self.loop_stack.last()
                .map(|ctx| ctx.cond_label.clone())
                .ok_or_else(|| crate::error::codegen_error("continue outside loop".to_string()))?
        };
        self.set_terminator(IrTerminator::Branch { target })?;
        Ok(())
    }

    fn build_return(&mut self, expr: &Option<Expr>) -> cayResult<()> {
        if let Some(e) = expr.as_ref() {
            let value = self.build_expression(e)?;
            self.set_terminator(IrTerminator::Return { value: Some(value) })?;
        } else {
            self.set_terminator(IrTerminator::Return { value: None })?;
        }
        Ok(())
    }

    // ============================================================
    // 表达式构建
    // ============================================================

    fn build_expression(&mut self, expr: &Expr) -> cayResult<IrValue> {
        match expr {
            Expr::Literal(lit_expr) => self.build_literal(&lit_expr.value),
            Expr::Identifier(ident) => self.build_identifier(&ident.name),
            Expr::Binary(bin) => self.build_binary(bin),
            Expr::Unary(unary) => self.build_unary(unary),
            Expr::Call(call) => self.build_call(call),
            Expr::Assignment(assign) => self.build_assignment(assign),
            Expr::MemberAccess(member) => self.build_member_access(member),
            Expr::Cast(cast) => self.build_cast(cast),
            Expr::New(new_expr) => self.build_new(new_expr),
            Expr::Ternary(tern) => self.build_ternary(tern),
            Expr::ArrayCreation(arr) => self.build_array_creation(arr),
            Expr::ArrayAccess(arr) => self.build_array_access(arr),
            Expr::ArrayInit(init) => self.build_array_init(init),
            _ => Err(crate::error::codegen_error(
                format!("Expression type not yet implemented in IR builder")
            )),
        }
    }

    fn build_literal(&mut self, lit: &LiteralValue) -> cayResult<IrValue> {
        match lit {
            LiteralValue::Int32(v) => Ok(IrValue::IntConst(*v as i64, IrType::I32)),
            LiteralValue::Int64(v) => Ok(IrValue::IntConst(*v, IrType::I64)),
            LiteralValue::Float32(v) => Ok(IrValue::FloatConst(*v as f64, IrType::F32)),
            LiteralValue::Float64(v) => Ok(IrValue::FloatConst(*v, IrType::F64)),
            LiteralValue::Bool(v) => Ok(IrValue::BoolConst(*v)),
            LiteralValue::Char(v) => Ok(IrValue::IntConst(*v as i64, IrType::I8)),
            LiteralValue::String(s) => {
                let name = self.module.add_string(s);
                Ok(IrValue::GlobalRef(name, IrType::Pointer(Box::new(IrType::I8))))
            }
            LiteralValue::Null => Ok(IrValue::NullConst(IrType::Pointer(Box::new(IrType::I8)))),
        }
    }

    fn build_identifier(&mut self, name: &str) -> cayResult<IrValue> {
        // 查找变量
        let alloca_opt = self.scope_manager.lookup(name).cloned();
        if let Some(alloca) = alloca_opt {
            let ty = alloca.ir_type();
            // 指针类型 → 加载
            if let IrType::Pointer(inner) = ty {
                let result = self.new_temp(*inner.clone());
                self.emit(IrInstruction::Load {
                    result: result.clone(),
                    ptr: alloca.clone(),
                    ty: *inner.clone(),
                })?;
                return Ok(result);
            }
            return Ok(alloca);
        }

        // 查找静态字段
        let full_name = format!("@{}.{}_s", self.current_class, name);
        Ok(IrValue::GlobalRef(full_name, IrType::I32)) // 默认类型
    }

    fn build_binary(&mut self, bin: &BinaryExpr) -> cayResult<IrValue> {
        let left = self.build_expression(&bin.left)?;
        let right = self.build_expression(&bin.right)?;

        let ty = left.ir_type();
        let result = self.new_temp(ty.clone());

        let op = match bin.op {
            BinaryOp::Add => if ty.is_float() { IrBinaryOp::FAdd } else { IrBinaryOp::Add },
            BinaryOp::Sub => if ty.is_float() { IrBinaryOp::FSub } else { IrBinaryOp::Sub },
            BinaryOp::Mul => if ty.is_float() { IrBinaryOp::FMul } else { IrBinaryOp::Mul },
            BinaryOp::Div => if ty.is_float() { IrBinaryOp::FDiv } else { IrBinaryOp::Div },
            BinaryOp::Mod => if ty.is_float() { IrBinaryOp::FRem } else { IrBinaryOp::Mod },
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le |
            BinaryOp::Gt | BinaryOp::Ge | BinaryOp::And | BinaryOp::Or => {
                // 比较和逻辑运算在 build_compare 中处理
                return self.build_compare(bin);
            }
            BinaryOp::BitAnd => IrBinaryOp::And,
            BinaryOp::BitOr => IrBinaryOp::Or,
            BinaryOp::BitXor => IrBinaryOp::Xor,
            BinaryOp::Shl => IrBinaryOp::Shl,
            BinaryOp::Shr => IrBinaryOp::Shr,
            BinaryOp::UnsignedShr => IrBinaryOp::LShr,
        };

        self.emit(IrInstruction::BinaryOp {
            result: result.clone(),
            op,
            left,
            right,
        })?;
        Ok(result)
    }

    fn build_compare(&mut self, bin: &BinaryExpr) -> cayResult<IrValue> {
        let left = self.build_expression(&bin.left)?;
        let right = self.build_expression(&bin.right)?;
        let ty = left.ir_type();
        let is_float = ty.is_float();

        let cmp_op = match bin.op {
            BinaryOp::Eq => if is_float { IrCmpOp::FEq } else { IrCmpOp::Eq },
            BinaryOp::Ne => if is_float { IrCmpOp::FNe } else { IrCmpOp::Ne },
            BinaryOp::Lt => if is_float { IrCmpOp::FLt } else { IrCmpOp::Slt },
            BinaryOp::Le => if is_float { IrCmpOp::FLe } else { IrCmpOp::Sle },
            BinaryOp::Gt => if is_float { IrCmpOp::FGt } else { IrCmpOp::Sgt },
            BinaryOp::Ge => if is_float { IrCmpOp::FGe } else { IrCmpOp::Sge },
            BinaryOp::And | BinaryOp::Or => {
                // 逻辑运算：先比较，然后 AND/OR
                let left_cmp = self.new_temp(IrType::I1);
                self.emit(IrInstruction::Compare {
                    result: left_cmp.clone(),
                    op: IrCmpOp::Ne,
                    left: left.clone(),
                    right: IrValue::IntConst(0, left.ir_type()),
                })?;
                let right_cmp = self.new_temp(IrType::I1);
                self.emit(IrInstruction::Compare {
                    result: right_cmp.clone(),
                    op: IrCmpOp::Ne,
                    left: right.clone(),
                    right: IrValue::IntConst(0, right.ir_type()),
                })?;
                let result = self.new_temp(IrType::I1);
                let op = if bin.op == BinaryOp::And { IrBinaryOp::And } else { IrBinaryOp::Or };
                self.emit(IrInstruction::BinaryOp {
                    result: result.clone(),
                    op,
                    left: left_cmp,
                    right: right_cmp,
                })?;
                return Ok(result);
            }
            _ => unreachable!(),
        };

        let result = self.new_temp(IrType::I1);
        self.emit(IrInstruction::Compare {
            result: result.clone(),
            op: cmp_op,
            left,
            right,
        })?;
        Ok(result)
    }

    fn build_unary(&mut self, unary: &UnaryExpr) -> cayResult<IrValue> {
        let operand = self.build_expression(&unary.operand)?;
        let ty = operand.ir_type();
        let result = self.new_temp(ty.clone());

        match unary.op {
            UnaryOp::Neg => {
                let op = if ty.is_float() { IrBinaryOp::FSub } else { IrBinaryOp::Sub };
                let zero = if ty.is_float() {
                    IrValue::FloatConst(0.0, ty.clone())
                } else {
                    IrValue::IntConst(0, ty.clone())
                };
                self.emit(IrInstruction::BinaryOp {
                    result: result.clone(),
                    op,
                    left: zero,
                    right: operand,
                })?;
            }
            UnaryOp::Not => {
                self.emit(IrInstruction::Compare {
                    result: result.clone(),
                    op: IrCmpOp::Eq,
                    left: operand,
                    right: IrValue::IntConst(0, ty),
                })?;
            }
            UnaryOp::BitNot => {
                let neg_one = IrValue::IntConst(-1, ty.clone());
                self.emit(IrInstruction::BinaryOp {
                    result: result.clone(),
                    op: IrBinaryOp::Xor,
                    left: operand,
                    right: neg_one,
                })?;
            }
            UnaryOp::PostInc | UnaryOp::PreInc => {
                // x++ 或 ++x: 增加1
                let one = IrValue::IntConst(1, ty.clone());
                self.emit(IrInstruction::BinaryOp {
                    result: result.clone(),
                    op: IrBinaryOp::Add,
                    left: operand.clone(),
                    right: one,
                })?;
                // 存储回变量
                self.build_assignment_to_expr(&unary.operand, result.clone())?;
                // PostInc 返回原值，PreInc 返回新值
                if unary.op == UnaryOp::PostInc {
                    return Ok(operand);
                }
            }
            UnaryOp::PostDec | UnaryOp::PreDec => {
                // x-- 或 --x: 减少1
                let one = IrValue::IntConst(1, ty.clone());
                self.emit(IrInstruction::BinaryOp {
                    result: result.clone(),
                    op: IrBinaryOp::Sub,
                    left: operand.clone(),
                    right: one,
                })?;
                // 存储回变量
                self.build_assignment_to_expr(&unary.operand, result.clone())?;
                // PostDec 返回原值，PreDec 返回新值
                if unary.op == UnaryOp::PostDec {
                    return Ok(operand);
                }
            }
            _ => {
                return Err(crate::error::codegen_error(
                    format!("Unary operator {:?} not yet implemented in IR builder", unary.op)
                ));
            }
        }
        Ok(result)
    }

    /// 将值赋给表达式（用于自增/自减）
    fn build_assignment_to_expr(&mut self, target: &Expr, value: IrValue) -> cayResult<()> {
        match target {
            Expr::Identifier(ident) => {
                let alloca_opt = self.scope_manager.lookup(&ident.name).cloned();
                if let Some(alloca) = alloca_opt {
                    let ty = value.ir_type();
                    self.emit(IrInstruction::Store {
                        value,
                        ptr: alloca,
                        ty,
                    })?;
                }
                Ok(())
            }
            Expr::ArrayAccess(arr) => {
                self.build_array_assignment(arr, value)
            }
            _ => {
                Err(crate::error::codegen_error(
                    "Complex assignment target not yet implemented in IR builder".to_string()
                ))
            }
        }
    }

    fn build_array_assignment(&mut self, arr: &crate::ast::ArrayAccessExpr, value: IrValue) -> cayResult<()> {
        // 构建数组和索引
        let array_val = self.build_expression(&arr.array)?;
        let index_val = self.build_expression(&arr.index)?;

        // 获取元素类型
        let (elem_ty, base_ty) = match array_val.ir_type() {
            IrType::Array(elem, _) => (*elem.clone(), *elem),
            IrType::Pointer(elem) => (*elem.clone(), *elem),
            _ => {
                let ty = value.ir_type();
                (ty.clone(), ty)
            }
        };

        // 计算元素地址
        let ptr = self.new_temp(IrType::Pointer(Box::new(elem_ty.clone())));
        self.emit(IrInstruction::GetElementPtr {
            result: ptr.clone(),
            ptr: array_val,
            indices: vec![index_val],
            base_ty,
        })?;

        // 存储值到元素地址
        self.emit(IrInstruction::Store {
            value,
            ptr,
            ty: elem_ty,
        })?;

        Ok(())
    }

    fn build_call(&mut self, call: &CallExpr) -> cayResult<IrValue> {
        let func_name = match call.callee.as_ref() {
            Expr::Identifier(ident) => ident.name.clone(),
            Expr::MemberAccess(member) => {
                // obj.method() - 需要虚调用分派
                format!("{}.{}", self.current_class, member.member)
            }
            _ => return Err(crate::error::codegen_error("Complex callee not yet supported in IR builder".to_string())),
        };

        let mut args: Vec<IrValue> = Vec::new();
        for arg in &call.args {
            args.push(self.build_expression(arg)?);
        }

        // 查找函数返回类型
        let return_ty = self.lookup_function_return_type(&func_name);

        // 创建结果寄存器
        let result = if return_ty == IrType::Void {
            IrValue::Undef(IrType::Void)
        } else {
            self.new_temp(return_ty.clone())
        };

        self.emit(IrInstruction::Call {
            result: if return_ty == IrType::Void { None } else { Some(result.clone()) },
            func_name,
            args,
            return_ty,
        })?;
        Ok(result)
    }

    /// 查找函数返回类型
    fn lookup_function_return_type(&self, func_name: &str) -> IrType {
        // 1. 查找已定义的函数
        if let Some(func) = self.module.find_function(func_name) {
            return func.return_type.clone();
        }

        // 2. 查找外部声明
        if let Some(extern_decl) = self.module.find_extern(func_name) {
            return extern_decl.return_type.clone();
        }

        // 3. 根据命名约定推断
        if func_name.starts_with("@") {
            // 全局函数，尝试查找
            let clean_name = func_name.trim_start_matches("@");
            if let Some(func) = self.module.find_function(clean_name) {
                return func.return_type.clone();
            }
            if let Some(extern_decl) = self.module.find_extern(clean_name) {
                return extern_decl.return_type.clone();
            }
        }

        // 4. 默认返回类型为 Void
        IrType::Void
    }

    fn build_assignment(&mut self, assign: &AssignmentExpr) -> cayResult<IrValue> {
        let value = self.build_expression(&assign.value)?;
        let ty = value.ir_type();

        // 处理复合赋值: +=, -=, *=, /=, %=
        let final_value = if assign.op != crate::ast::AssignOp::Assign {
            self.build_compound_op(&assign.target, &assign.op, value)?
        } else {
            value
        };

        match assign.target.as_ref() {
            Expr::Identifier(ident) => {
                let alloca_opt = self.scope_manager.lookup(&ident.name).cloned();
                if let Some(alloca) = alloca_opt {
                    self.emit(IrInstruction::Store {
                        value: final_value.clone(),
                        ptr: alloca.clone(),
                        ty: final_value.ir_type(),
                    })?;
                }
            }
            Expr::ArrayAccess(arr) => {
                self.build_array_assignment(arr, final_value.clone())?;
            }
            _ => {
                return Err(crate::error::codegen_error(
                    "Complex assignment target not yet implemented in IR builder".to_string()
                ));
            }
        }
        Ok(final_value)
    }

    /// 构建复合赋值操作 (+=, -=, *=, /=, %=)
    fn build_compound_op(&mut self, target: &Expr, op: &crate::ast::AssignOp, value: IrValue) -> cayResult<IrValue> {
        // 获取当前值
        let current_val = self.build_expression(target)?;
        let ty = current_val.ir_type();

        // 映射赋值操作到二元操作
        let binary_op = match op {
            crate::ast::AssignOp::AddAssign => IrBinaryOp::Add,
            crate::ast::AssignOp::SubAssign => IrBinaryOp::Sub,
            crate::ast::AssignOp::MulAssign => IrBinaryOp::Mul,
            crate::ast::AssignOp::DivAssign => IrBinaryOp::Div,
            crate::ast::AssignOp::ModAssign => IrBinaryOp::Mod,
            _ => return Ok(value), // 普通赋值，不处理
        };

        // 执行二元操作
        let result = self.new_temp(ty.clone());
        self.emit(IrInstruction::BinaryOp {
            result: result.clone(),
            op: binary_op,
            left: current_val,
            right: value,
        })?;

        Ok(result)
    }

    fn build_member_access(&mut self, member: &MemberAccessExpr) -> cayResult<IrValue> {
        let obj = self.build_expression(&member.object)?;
        let obj_ty = obj.ir_type();

        // 处理数组的 length 属性
        if member.member == "length" {
            if let IrType::Array(_, len) = obj_ty {
                return Ok(IrValue::IntConst(len as i64, IrType::I32));
            }
            // 对于其他类型，尝试从类型注册表获取
            if let Some(ref registry) = self.type_registry {
                let class_name = self.extract_class_name(&obj_ty);
                if let Some(class_info) = registry.get_class(&class_name) {
                    // 查找 length 字段或方法
                    if let Some(field_info) = class_info.fields.get("length") {
                        return Ok(IrValue::IntConst(0, IrType::from(&field_info.field_type)));
                    }
                }
            }
            // 默认返回 0 作为 length
            return Ok(IrValue::IntConst(0, IrType::I32));
        }

        // 获取对象类型名称（从指针类型中提取）
        let class_name = match &obj_ty {
            IrType::Pointer(inner) => self.extract_class_name(inner),
            _ => self.current_class.clone(),
        };

        // 查找字段偏移
        if let Some(layout) = self.class_layouts.get(&class_name) {
            if let Some(&offset) = layout.get(&member.member) {
                // 获取字段类型
                let field_ty = self.lookup_field_type(&class_name, &member.member);

                // 计算字段地址: obj + offset
                let offset_val = IrValue::IntConst(offset as i64, IrType::I64);
                let field_ptr = self.new_temp(IrType::Pointer(Box::new(field_ty.clone())));
                self.emit(IrInstruction::GetElementPtr {
                    result: field_ptr.clone(),
                    ptr: obj,
                    indices: vec![offset_val],
                    base_ty: IrType::I8,
                })?;

                // 加载字段值
                let result = self.new_temp(field_ty.clone());
                self.emit(IrInstruction::Load {
                    result: result.clone(),
                    ty: field_ty,
                    ptr: field_ptr,
                })?;
                return Ok(result);
            }
        }

        // 检查是否是静态字段
        let static_name = format!("@{}.{}_s", class_name, member.member);
        if self.module.find_function(&static_name).is_some() || self.module.find_extern(&static_name).is_some() {
            return Ok(IrValue::GlobalRef(static_name, IrType::I32));
        }

        // 如果找不到字段，返回错误
        Err(crate::error::codegen_error(
            format!("Field '{}' not found in class '{}'", member.member, class_name)
        ))
    }

    /// 从类型中提取类名
    fn extract_class_name(&self, ty: &IrType) -> String {
        match ty {
            IrType::Struct { name, .. } => name.clone(),
            IrType::Pointer(inner) => self.extract_class_name(inner),
            _ => self.current_class.clone(),
        }
    }

    /// 查找字段类型
    fn lookup_field_type(&self, class_name: &str, field_name: &str) -> IrType {
        // 从类型注册表查找类信息
        if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                // 尝试从类字段中查找字段类型
                if let Some(field_info) = class_info.fields.get(field_name) {
                    return IrType::from(&field_info.field_type);
                }
            }
        }
        IrType::I32 // 默认类型
    }

    fn build_cast(&mut self, cast: &CastExpr) -> cayResult<IrValue> {
        let value = self.build_expression(&cast.expr)?;
        let from_ty = value.ir_type();
        let to_ty = IrType::from(&cast.target_type);

        if from_ty == to_ty {
            return Ok(value);
        }

        let result = self.new_temp(to_ty.clone());
        let kind = self.determine_cast_kind(&from_ty, &to_ty)?;

        self.emit(IrInstruction::Cast {
            result: result.clone(),
            kind,
            value,
            to_ty,
        })?;
        Ok(result)
    }

    fn build_new(&mut self, new_expr: &NewExpr) -> cayResult<IrValue> {
        let class_name = &new_expr.class_name;

        // 获取类大小
        let size = self.class_sizes.get(class_name).copied().unwrap_or(8);
        let size_val = IrValue::IntConst(size as i64, IrType::I64);

        // 调用 GC 分配函数 (cavvy_gc_alloc)
        let alloc_result = self.new_temp(IrType::Pointer(Box::new(IrType::I8)));
        self.emit(IrInstruction::Call {
            result: Some(alloc_result.clone()),
            func_name: "cavvy_gc_alloc".to_string(),
            args: vec![size_val],
            return_ty: IrType::Pointer(Box::new(IrType::I8)),
        })?;

        // 转换为对象指针类型
        let obj_ptr_ty = IrType::Pointer(Box::new(IrType::Struct {
            name: class_name.clone(),
            fields: vec![],
        }));
        let obj_ptr = self.new_temp(obj_ptr_ty.clone());
        self.emit(IrInstruction::Cast {
            result: obj_ptr.clone(),
            kind: IrCastKind::BitCast,
            value: alloc_result,
            to_ty: obj_ptr_ty.clone(),
        })?;

        // 调用构造函数
        let ctor_name = format!("{}.__ctor", class_name);
        self.emit(IrInstruction::Call {
            result: None,
            func_name: ctor_name,
            args: vec![obj_ptr.clone()],
            return_ty: IrType::Void,
        })?;

        Ok(obj_ptr)
    }

    fn build_ternary(&mut self, tern: &TernaryExpr) -> cayResult<IrValue> {
        let cond = self.build_expression(&tern.condition)?;
        let cond_val = self.new_temp(IrType::I1);
        self.emit(IrInstruction::Compare {
            result: cond_val.clone(),
            op: IrCmpOp::Ne,
            left: cond,
            right: IrValue::IntConst(0, IrType::I32),
        })?;

        let true_val = self.build_expression(&tern.true_branch)?;
        let false_val = self.build_expression(&tern.false_branch)?;
        let ty = true_val.ir_type();
        let result = self.new_temp(ty.clone());

        self.emit(IrInstruction::Select {
            result: result.clone(),
            condition: cond_val,
            true_val,
            false_val,
        })?;
        Ok(result)
    }

    fn build_array_creation(&mut self, arr: &crate::ast::ArrayCreationExpr) -> cayResult<IrValue> {
        // 计算第一维数组大小（支持多维数组时取第一个）
        let size_val = arr.sizes.first()
            .map(|s| self.build_expression(s))
            .transpose()?;

        let elem_ty = IrType::from(&arr.element_type);

        // 确定数组大小（常量或默认）
        let arr_size = if let Some(IrValue::IntConst(n, _)) = size_val {
            n as usize
        } else {
            10 // 默认大小
        };

        // 创建固定大小数组类型
        let arr_ty = IrType::Array(Box::new(elem_ty), arr_size.max(1));
        let alloca_result = self.new_temp(arr_ty.clone());
        self.emit(IrInstruction::Alloca {
            result: alloca_result.clone(),
            ty: arr_ty,
            align: 8,
        })?;

        Ok(alloca_result)
    }

    fn build_array_access(&mut self, arr: &crate::ast::ArrayAccessExpr) -> cayResult<IrValue> {
        // 构建数组和索引
        let array_val = self.build_expression(&arr.array)?;
        let index_val = self.build_expression(&arr.index)?;

        // 获取元素类型
        let (elem_ty, base_ty) = match array_val.ir_type() {
            IrType::Array(elem, _) => (*elem.clone(), *elem),
            IrType::Pointer(elem) => (*elem.clone(), *elem),
            _ => (IrType::I32, IrType::I32),
        };

        // 计算元素地址
        let ptr = self.new_temp(IrType::Pointer(Box::new(elem_ty.clone())));
        self.emit(IrInstruction::GetElementPtr {
            result: ptr.clone(),
            ptr: array_val,
            indices: vec![index_val],
            base_ty,
        })?;

        // 加载元素值
        let result = self.new_temp(elem_ty.clone());
        self.emit(IrInstruction::Load {
            result: result.clone(),
            ty: elem_ty,
            ptr,
        })?;

        Ok(result)
    }

    fn build_array_init(&mut self, init: &crate::ast::ArrayInitExpr) -> cayResult<IrValue> {
        // 数组初始化: {1, 2, 3}
        // 创建固定大小的数组并初始化元素
        let elem_count = init.elements.len();
        if elem_count == 0 {
            return Ok(IrValue::NullConst(IrType::Pointer(Box::new(IrType::I8))));
        }

        // 推断元素类型（从第一个元素）
        let first_val = self.build_expression(&init.elements[0])?;
        let elem_ty = first_val.ir_type();

        // 创建数组类型
        let arr_ty = IrType::Array(Box::new(elem_ty.clone()), elem_count);
        let arr_result = self.new_temp(arr_ty.clone());

        // 分配数组内存
        self.emit(IrInstruction::Alloca {
            result: arr_result.clone(),
            ty: arr_ty.clone(),
            align: 8,
        })?;

        // 存储每个元素
        for (idx, elem) in init.elements.iter().enumerate() {
            let val = if idx == 0 {
                first_val.clone()
            } else {
                self.build_expression(elem)?
            };

            // 计算元素地址
            let index_val = IrValue::IntConst(idx as i64, IrType::I32);
            let elem_ptr = self.new_temp(IrType::Pointer(Box::new(elem_ty.clone())));
            self.emit(IrInstruction::GetElementPtr {
                result: elem_ptr.clone(),
                ptr: arr_result.clone(),
                indices: vec![index_val],
                base_ty: elem_ty.clone(),
            })?;

            // 存储元素值
            self.emit(IrInstruction::Store {
                value: val,
                ptr: elem_ptr,
                ty: elem_ty.clone(),
            })?;
        }

        Ok(arr_result)
    }

    // ============================================================
    // 辅助方法
    // ============================================================

    fn determine_cast_kind(&self, from: &IrType, to: &IrType) -> cayResult<IrCastKind> {
        match (from, to) {
            // 整数扩展/截断
            (IrType::I1, IrType::I8 | IrType::I16 | IrType::I32 | IrType::I64) => Ok(IrCastKind::ZeroExt),
            (IrType::I8, IrType::I16 | IrType::I32 | IrType::I64) => Ok(IrCastKind::SignExt),
            (IrType::I16, IrType::I32 | IrType::I64) => Ok(IrCastKind::SignExt),
            (IrType::I32, IrType::I64) => Ok(IrCastKind::SignExt),
            (IrType::I64, IrType::I32) => Ok(IrCastKind::Trunc),
            (IrType::I64, IrType::I16 | IrType::I8 | IrType::I1) => Ok(IrCastKind::Trunc),
            (IrType::I32, IrType::I16 | IrType::I8 | IrType::I1) => Ok(IrCastKind::Trunc),

            // 整数 ↔ 浮点
            (IrType::I32 | IrType::I64, IrType::F32 | IrType::F64) => Ok(IrCastKind::IntToFloat),
            (IrType::F32 | IrType::F64, IrType::I32 | IrType::I64) => Ok(IrCastKind::FloatToInt),

            // 浮点扩展/截断
            (IrType::F32, IrType::F64) => Ok(IrCastKind::FloatExt),
            (IrType::F64, IrType::F32) => Ok(IrCastKind::FloatTrunc),

            // 指针转换
            (IrType::Pointer(_), IrType::Pointer(_)) => Ok(IrCastKind::BitCast),
            (IrType::Pointer(_), _) if to.is_integer() => Ok(IrCastKind::PtrToInt),
            (_, IrType::Pointer(_)) if from.is_integer() => Ok(IrCastKind::IntToPtr),

            _ => Err(crate::error::codegen_error(
                format!("Cannot cast from {} to {}", from.to_llvm_str(), to.to_llvm_str())
            )),
        }
    }

    fn infer_type_from_expr(&self, expr: Option<&Expr>) -> cayResult<Type> {
        match expr {
            Some(Expr::Literal(lit_expr)) => match &lit_expr.value {
                LiteralValue::Int32(_) => Ok(Type::Int32),
                LiteralValue::Int64(_) => Ok(Type::Int64),
                LiteralValue::Float32(_) => Ok(Type::Float32),
                LiteralValue::Float64(_) => Ok(Type::Float64),
                LiteralValue::Bool(_) => Ok(Type::Bool),
                LiteralValue::Char(_) => Ok(Type::Char),
                LiteralValue::String(_) => Ok(Type::String),
                LiteralValue::Null => Ok(Type::Object("Object".to_string())),
            }
            _ => Ok(Type::Int32), // 默认
        }
    }

    fn generate_method_name(&self, class_name: &str, method: &MethodDecl) -> String {
        if method.params.is_empty() {
            format!("{}.{}", class_name, method.name)
        } else {
            let param_types: Vec<String> = method.params.iter()
                .map(|p| self.type_to_signature(&p.param_type))
                .collect();
            format!("{}.__{}_{}", class_name, method.name, param_types.join("_"))
        }
    }

    fn generate_constructor_name(&self, class_name: &str, ctor: &ConstructorDecl) -> String {
        if ctor.params.is_empty() {
            format!("{}.__ctor", class_name)
        } else {
            let param_types: Vec<String> = ctor.params.iter()
                .map(|p| self.type_to_signature(&p.param_type))
                .collect();
            format!("{}.__ctor_{}", class_name, param_types.join("_"))
        }
    }

    fn type_to_signature(&self, ty: &Type) -> String {
        match ty {
            Type::Void => "v".to_string(),
            Type::Int32 => "i".to_string(),
            Type::Int64 => "l".to_string(),
            Type::Float32 => "f".to_string(),
            Type::Float64 => "d".to_string(),
            Type::Bool => "b".to_string(),
            Type::String => "s".to_string(),
            Type::Char => "c".to_string(),
            Type::Object(name) => format!("o{}", name),
            Type::Array(inner) => format!("a{}", self.type_to_signature(inner)),
            _ => "x".to_string(),
        }
    }

    /// 消耗 builder，返回构建的模块
    pub fn finish(self) -> IrModule {
        self.module
    }
}

impl Default for IrBuilder {
    fn default() -> Self {
        Self::new()
    }
}
