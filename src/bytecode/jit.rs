/// JIT/AOT 编译器
/// 将Cavvy字节码编译为LLVM IR，然后编译为机器码
use super::*;

/// JIT编译错误
#[derive(Debug)]
pub enum JitError {
    IoError(std::io::Error),
    CompilationError(String),
    LinkingError(String),
    InvalidBytecode(String),
}

impl std::fmt::Display for JitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JitError::IoError(e) => write!(f, "IO error: {}", e),
            JitError::CompilationError(e) => write!(f, "Compilation error: {}", e),
            JitError::LinkingError(e) => write!(f, "Linking error: {}", e),
            JitError::InvalidBytecode(e) => write!(f, "Invalid bytecode: {}", e),
        }
    }
}

impl std::error::Error for JitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            JitError::IoError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for JitError {
    fn from(e: std::io::Error) -> Self {
        JitError::IoError(e)
    }
}

/// JIT编译选项
#[derive(Debug, Clone)]
pub struct JitOptions {
    /// 优化级别
    pub optimization: String,
    /// 目标平台
    pub target: String,
    /// 保留中间文件
    pub keep_intermediate: bool,
    /// 输出目录
    pub output_dir: Option<String>,
    /// 链接的库
    pub link_libs: Vec<String>,
    /// 库搜索路径
    pub lib_paths: Vec<String>,
}

impl Default for JitOptions {
    fn default() -> Self {
        Self {
            optimization: "-O2".to_string(),
            target: get_default_target(),
            keep_intermediate: false,
            output_dir: None,
            link_libs: Vec::new(),
            lib_paths: Vec::new(),
        }
    }
}

/// 获取默认目标平台
fn get_default_target() -> String {
    if cfg!(target_os = "windows") {
        "x86_64-w64-mingw32".to_string()
    } else if cfg!(target_os = "linux") {
        "x86_64-unknown-linux-gnu".to_string()
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin".to_string()
    } else {
        "x86_64-unknown-linux-gnu".to_string()
    }
}

/// JIT编译器
pub struct JitCompiler {
    options: JitOptions,
}

/// 指令生成上下文
struct InstructionContext<'a> {
    /// 常量池引用
    pool: &'a ConstantPool,
    /// 临时变量计数器
    temp_counter: &'a mut u32,
    /// 局部变量映射（索引 -> LLVM值名）
    local_vars: &'a mut std::collections::HashMap<u16, String>,
    /// 操作数栈
    operand_stack: &'a mut Vec<String>,
    /// 当前基本块标签
    current_block: &'a mut String,
    /// 标签计数器
    label_counter: &'a mut u32,
    /// 字节码位置到标签的映射（用于跳转目标）
    position_labels: &'a mut std::collections::HashMap<usize, String>,
}

impl<'a> InstructionContext<'a> {
    /// 获取下一个临时变量名
    fn next_temp(&mut self) -> String {
        let temp = format!("%t{}", self.temp_counter);
        *self.temp_counter += 1;
        temp
    }

    /// 获取下一个标签名
    fn next_label(&mut self) -> String {
        let label = format!("label{}", self.label_counter);
        *self.label_counter += 1;
        label
    }

    /// 获取或创建指定位置的标签
    ///
    /// 如果该位置已有标签则返回已有的，否则创建新标签。
    /// 用于跳转指令的目标标签映射。
    fn get_or_create_label(&mut self, position: usize) -> String {
        self.position_labels
            .get(&position)
            .cloned()
            .unwrap_or_else(|| {
                let label = self.next_label();
                self.position_labels.insert(position, label.clone());
                label
            })
    }

    /// 从栈顶弹出一个值
    fn pop(&mut self) -> Option<String> {
        self.operand_stack.pop()
    }

    /// 向栈顶压入一个值
    fn push(&mut self, value: String) {
        self.operand_stack.push(value);
    }

    /// 获取栈顶值（不弹出）
    fn peek(&self) -> Option<&String> {
        self.operand_stack.last()
    }

    /// 获取局部变量名
    fn get_local(&self, index: u16) -> String {
        self.local_vars
            .get(&index)
            .cloned()
            .unwrap_or_else(|| format!("%local{}", index))
    }

    /// 设置局部变量名
    fn set_local(&mut self, index: u16, name: String) {
        self.local_vars.insert(index, name);
    }
}

impl JitCompiler {
    /// 创建新的JIT编译器
    pub fn new(options: JitOptions) -> Self {
        Self { options }
    }

    /// 编译字节码模块为可执行文件
    pub fn compile_to_executable(
        &self,
        module: &BytecodeModule,
        output_path: &str,
    ) -> Result<(), JitError> {
        // 1. 将字节码转换为LLVM IR
        let ir_code = self.bytecode_to_ir(module)?;

        // 2. 确定输出目录
        let output_dir = self
            .options
            .output_dir
            .as_ref()
            .map(|s| std::path::PathBuf::from(s))
            .unwrap_or_else(|| std::env::temp_dir().join("cavvy-jit"));

        std::fs::create_dir_all(&output_dir)?;

        // 3. 写入IR文件
        let ir_file = output_dir.join("output.ll");
        std::fs::write(&ir_file, &ir_code)?;

        // 4. 编译IR为对象文件
        let obj_file = output_dir.join("output.o");
        self.compile_ir_to_object(&ir_file, &obj_file)?;

        // 5. 链接对象文件为可执行文件
        self.link_executable(&obj_file, output_path)?;

        // 6. 清理中间文件
        if !self.options.keep_intermediate {
            let _ = std::fs::remove_file(&ir_file);
            let _ = std::fs::remove_file(&obj_file);
        }

        Ok(())
    }

    /// 将字节码转换为LLVM IR
    pub fn bytecode_to_ir(&self, module: &BytecodeModule) -> Result<String, JitError> {
        let mut ir = String::new();

        // 添加IR头部
        ir.push_str("; Cavvy Bytecode Compiled IR\n");
        ir.push_str(&format!("; Module: {}\n", module.header.name));
        ir.push_str(&format!("; Target: {}\n", module.header.target_platform));
        ir.push_str(&format!("; Obfuscated: {}\n\n", module.header.obfuscated));

        // 添加目标平台声明
        ir.push_str(self.generate_target_declarations());

        // 添加运行时声明
        ir.push_str(self.generate_runtime_declarations());

        // 添加外部库声明
        ir.push_str(&self.generate_external_lib_declarations(&module.header.external_libs));

        // 添加字符串常量
        ir.push_str(&self.generate_string_constants(module)?);

        // 添加类型定义
        for type_def in &module.type_definitions {
            ir.push_str(&self.generate_type_definition(type_def, &module.constant_pool)?);
        }

        // 添加全局变量
        for global in &module.global_variables {
            ir.push_str(&self.generate_global_variable(global, &module.constant_pool)?);
        }

        // 添加函数定义
        for func in &module.functions {
            ir.push_str(&self.generate_function(func, &module.constant_pool)?);
        }

        // 添加类方法定义
        for type_def in &module.type_definitions {
            for method in &type_def.methods {
                if let Some(ref body) = method.body {
                    ir.push_str(&self.generate_method(
                        type_def,
                        method,
                        body,
                        &module.constant_pool,
                    )?);
                }
            }
        }

        Ok(ir)
    }

    /// 生成目标平台声明
    fn generate_target_declarations(&self) -> &'static str {
        r#"; Target declarations
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-windows-gnu"

"#
    }

    /// 生成运行时声明
    fn generate_runtime_declarations(&self) -> &'static str {
        r#"; Runtime function declarations
declare i32 @printf(i8*, ...)
declare i32 @scanf(i8*, ...)
declare i8* @malloc(i64)
declare void @free(i8*)
declare i8* @memcpy(i8*, i8*, i64)
declare i8* @memset(i8*, i32, i64)

; String runtime functions
declare i8* @cavvy_string_concat(i8*, i8*)
declare i8* @cavvy_string_substring(i8*, i32, i32)
declare i32 @cavvy_string_length(i8*)
declare i32 @cavvy_string_indexof(i8*, i8*)
declare i8* @cavvy_string_replace(i8*, i8*, i8*)
declare i8 @cavvy_string_charat(i8*, i32)

; Array runtime functions
declare i8* @cavvy_array_new(i32, i32)
declare i32 @cavvy_array_length(i8*)
declare i8* @cavvy_array_get(i8*, i32)
declare void @cavvy_array_set(i8*, i32, i8*)

"#
    }

    /// 生成外部库声明
    fn generate_external_lib_declarations(&self, libs: &[String]) -> String {
        let mut decls = String::new();
        for lib in libs {
            decls.push_str(&format!("; External library: {}\n", lib));
            match lib.as_str() {
                "user32" => {
                    decls.push_str("declare i32 @MessageBoxA(i8*, i8*, i8*, i32)\n");
                }
                "kernel32" => {
                    decls.push_str("declare i32 @GetLastError()\n");
                }
                "m" => {
                    decls.push_str("declare double @sqrt(double)\n");
                    decls.push_str("declare double @pow(double, double)\n");
                    decls.push_str("declare double @sin(double)\n");
                    decls.push_str("declare double @cos(double)\n");
                }
                _ => {}
            }
        }
        decls.push('\n');
        decls
    }

    /// 生成字符串常量
    fn generate_string_constants(&self, module: &BytecodeModule) -> Result<String, JitError> {
        use super::constant_pool::Constant;

        let mut ir = String::new();
        ir.push_str("; String constants\n");

        // 遍历常量池中的所有字符串常量
        // 注意：我们需要通过索引来遍历，因为entries是私有的
        let pool_size = module.constant_pool.size() as u16;
        for index in 1..pool_size {
            if let Some(s) = module.constant_pool.get_string(index) {
                let escaped = s
                    .replace("\\", "\\\\")
                    .replace("\"", "\\\"")
                    .replace("\n", "\\0A")
                    .replace("\r", "\\0D")
                    .replace("\t", "\\09");
                ir.push_str(&format!(
                    "@str_{} = private unnamed_addr constant [{} x i8] c\"{}\\00\"\n",
                    index,
                    s.len() + 1,
                    escaped
                ));
            }
        }

        ir.push('\n');
        Ok(ir)
    }

    /// 生成类型定义
    fn generate_type_definition(
        &self,
        type_def: &TypeDefinition,
        pool: &ConstantPool,
    ) -> Result<String, JitError> {
        let name = pool
            .get_string(type_def.name_index)
            .ok_or_else(|| JitError::InvalidBytecode("Invalid type name index".to_string()))?;

        let mut ir = format!("; Type definition: {}\n", name);

        // 生成结构体类型
        ir.push_str(&format!("%struct.{} = type {{\n", name));

        // vtable指针（如果是类）
        if !type_def.modifiers.is_interface {
            ir.push_str("  %struct.vtable*,\n");
        }

        // 字段
        for field in &type_def.fields {
            let field_type = self.get_llvm_type_string(field.type_index, pool)?;
            ir.push_str(&format!("  {},\n", field_type));
        }

        ir.push_str("}\n\n");

        // 生成vtable类型
        if !type_def.modifiers.is_interface {
            ir.push_str(&format!("%struct.{}.vtable = type {{\n", name));
            for method in &type_def.methods {
                let method_sig = self.get_method_signature(method, pool)?;
                ir.push_str(&format!("  {},\n", method_sig));
            }
            ir.push_str("}\n\n");
        }

        Ok(ir)
    }

    /// 生成全局变量
    fn generate_global_variable(
        &self,
        global: &GlobalVariable,
        pool: &ConstantPool,
    ) -> Result<String, JitError> {
        let name = pool
            .get_string(global.name_index)
            .ok_or_else(|| JitError::InvalidBytecode("Invalid global name index".to_string()))?;
        let llvm_type = self.get_llvm_type_string(global.type_index, pool)?;

        let mut ir = format!("@{} = ", name);

        if global.modifiers.is_static {
            ir.push_str("internal ");
        }

        ir.push_str("global ");
        ir.push_str(&llvm_type);

        // 初始值
        if let Some(init_index) = global.initial_value {
            let init_val = self.get_constant_value(init_index, pool)?;
            ir.push_str(&format!(" {}", init_val));
        } else {
            ir.push_str(" zeroinitializer");
        }

        ir.push_str(", align 8\n");
        Ok(ir)
    }

    /// 生成函数
    fn generate_function(
        &self,
        func: &FunctionDefinition,
        pool: &ConstantPool,
    ) -> Result<String, JitError> {
        let name = pool
            .get_string(func.name_index)
            .ok_or_else(|| JitError::InvalidBytecode("Invalid function name index".to_string()))?;
        let ret_type = self.get_llvm_type_string(func.return_type_index, pool)?;

        let mut ir = format!("; Function: {}\n", name);

        // 确定链接类型
        let linkage = if func.modifiers.is_static && !func.modifiers.is_public {
            "internal"
        } else {
            "define"
        };

        ir.push_str(&format!("{} {} @{}(", linkage, ret_type, name));

        // 参数
        for (i, param_type_idx) in func.param_type_indices.iter().enumerate() {
            if i > 0 {
                ir.push_str(", ");
            }
            let param_type = self.get_llvm_type_string(*param_type_idx, pool)?;
            ir.push_str(&format!("{} %arg{}", param_type, i));
        }

        ir.push_str(") {\n");

        // 函数体
        ir.push_str(&self.generate_code_body(&func.body, pool, &ret_type)?);

        ir.push_str("}\n\n");
        Ok(ir)
    }

    /// 生成方法
    fn generate_method(
        &self,
        type_def: &TypeDefinition,
        method: &MethodDefinition,
        body: &CodeBody,
        pool: &ConstantPool,
    ) -> Result<String, JitError> {
        let type_name = pool
            .get_string(type_def.name_index)
            .ok_or_else(|| JitError::InvalidBytecode("Invalid type name index".to_string()))?;
        let method_name = pool
            .get_string(method.name_index)
            .ok_or_else(|| JitError::InvalidBytecode("Invalid method name index".to_string()))?;
        let ret_type = self.get_llvm_type_string(method.return_type_index, pool)?;

        let mut ir = format!("; Method: {}.{}\n", type_name, method_name);

        let linkage = if method.modifiers.is_static {
            "internal"
        } else {
            "define"
        };

        let mangled_name = format!("{}_{}", type_name, method_name);
        ir.push_str(&format!("{} {} @{}(", linkage, ret_type, mangled_name));

        // this指针（如果不是静态方法）
        if !method.modifiers.is_static {
            ir.push_str(&format!("%struct.{}* %this", type_name));
            if !method.param_type_indices.is_empty() {
                ir.push_str(", ");
            }
        }

        // 参数
        for (i, param_type_idx) in method.param_type_indices.iter().enumerate() {
            if i > 0 {
                ir.push_str(", ");
            }
            let param_type = self.get_llvm_type_string(*param_type_idx, pool)?;
            ir.push_str(&format!("{} %arg{}", param_type, i));
        }

        ir.push_str(") {\n");

        // 函数体
        ir.push_str(&self.generate_code_body(body, pool, &ret_type)?);

        ir.push_str("}\n\n");
        Ok(ir)
    }

    /// 生成代码体
    fn generate_code_body(
        &self,
        body: &CodeBody,
        pool: &ConstantPool,
        ret_type: &str,
    ) -> Result<String, JitError> {
        let mut ir = String::new();
        ir.push_str("entry:\n");

        // 初始化上下文
        let mut temp_counter: u32 = 0;
        let mut local_vars: std::collections::HashMap<u16, String> =
            std::collections::HashMap::new();
        let mut operand_stack: Vec<String> = Vec::new();
        let mut current_block = "entry".to_string();
        let mut label_counter: u32 = 0;
        let mut position_labels: std::collections::HashMap<usize, String> =
            std::collections::HashMap::new();

        let mut ctx = InstructionContext {
            pool,
            temp_counter: &mut temp_counter,
            local_vars: &mut local_vars,
            operand_stack: &mut operand_stack,
            current_block: &mut current_block,
            label_counter: &mut label_counter,
            position_labels: &mut position_labels,
        };

        // 生成每条指令
        for instr in &body.instructions {
            let instr_ir = self.generate_instruction(instr, &mut ctx)?;
            ir.push_str(&instr_ir);
        }

        // 确保有返回指令
        if !ir.contains("ret ") {
            if ret_type == "void" {
                ir.push_str("  ret void\n");
            } else if ret_type == "i32" {
                ir.push_str("  ret i32 0\n");
            } else {
                ir.push_str(&format!("  ret {} zeroinitializer\n", ret_type));
            }
        }

        Ok(ir)
    }

    /// 生成单条指令 - 完整实现所有指令
    fn generate_instruction(
        &self,
        instr: &Instruction,
        ctx: &mut InstructionContext,
    ) -> Result<String, JitError> {
        use instructions::Opcode;

        let mut ir = String::new();

        match instr.opcode {
            // ==================== 常量加载指令 ====================
            Opcode::Ldc => {
                let index = u16::from_le_bytes([instr.operands[0], instr.operands[1]]);
                let val = self.get_constant_value(index, ctx.pool)?;
                let temp = ctx.next_temp();

                // 根据常量类型决定如何处理
                if let Some(int_val) = ctx.pool.get_integer(index) {
                    ir.push_str(&format!("  {} = add i32 {}, 0\n", temp, int_val));
                    ctx.push(temp);
                } else if let Some(str_val) = ctx.pool.get_string(index) {
                    // 字符串常量 - 使用全局字符串
                    ir.push_str(&format!(
                        "  {} = getelementptr [{} x i8], [{} x i8]* @str_{}, i64 0, i64 0\n",
                        temp,
                        str_val.len() + 1,
                        str_val.len() + 1,
                        index
                    ));
                    ctx.push(temp);
                } else {
                    ir.push_str(&format!("  {} = add i32 {}, 0\n", temp, val));
                    ctx.push(temp);
                }
            }

            Opcode::Iconst => {
                let value = instr.operands[0] as i8 as i32;
                let temp = ctx.next_temp();
                ir.push_str(&format!("  {} = add i32 {}, 0\n", temp, value));
                ctx.push(temp);
            }

            Opcode::Lconst => {
                let value = i64::from_le_bytes([
                    instr.operands[0],
                    instr.operands[1],
                    instr.operands[2],
                    instr.operands[3],
                    instr.operands[4],
                    instr.operands[5],
                    instr.operands[6],
                    instr.operands[7],
                ]);
                let temp = ctx.next_temp();
                ir.push_str(&format!("  {} = add i64 {}, 0\n", temp, value));
                ctx.push(temp);
            }

            Opcode::Fconst => {
                let value = f32::from_le_bytes([
                    instr.operands[0],
                    instr.operands[1],
                    instr.operands[2],
                    instr.operands[3],
                ]);
                let temp = ctx.next_temp();
                ir.push_str(&format!("  {} = fadd float {}, 0.0\n", temp, value));
                ctx.push(temp);
            }

            Opcode::Dconst => {
                let value = f64::from_le_bytes([
                    instr.operands[0],
                    instr.operands[1],
                    instr.operands[2],
                    instr.operands[3],
                    instr.operands[4],
                    instr.operands[5],
                    instr.operands[6],
                    instr.operands[7],
                ]);
                let temp = ctx.next_temp();
                ir.push_str(&format!("  {} = fadd double {}, 0.0\n", temp, value));
                ctx.push(temp);
            }

            Opcode::AconstNull => {
                let temp = ctx.next_temp();
                ir.push_str(&format!("  {} = inttoptr i64 0 to i8*\n", temp));
                ctx.push(temp);
            }

            Opcode::Iconst0 => {
                let temp = ctx.next_temp();
                ir.push_str(&format!("  {} = add i32 0, 0\n", temp));
                ctx.push(temp);
            }

            Opcode::Iconst1 => {
                let temp = ctx.next_temp();
                ir.push_str(&format!("  {} = add i32 1, 0\n", temp));
                ctx.push(temp);
            }

            Opcode::IconstM1 => {
                let temp = ctx.next_temp();
                ir.push_str(&format!("  {} = add i32 -1, 0\n", temp));
                ctx.push(temp);
            }

            // ==================== 局部变量加载指令 ====================
            Opcode::Iload | Opcode::Lload | Opcode::Fload | Opcode::Dload | Opcode::Aload => {
                let index = u16::from_le_bytes([instr.operands[0], instr.operands[1]]);
                let local_name = ctx.get_local(index);
                ctx.push(local_name);
            }

            Opcode::Iload0 | Opcode::Iload1 | Opcode::Iload2 | Opcode::Iload3 => {
                let index = match instr.opcode {
                    Opcode::Iload0 => 0,
                    Opcode::Iload1 => 1,
                    Opcode::Iload2 => 2,
                    Opcode::Iload3 => 3,
                    _ => 0,
                };
                let local_name = ctx.get_local(index);
                ctx.push(local_name);
            }

            Opcode::Aload0 | Opcode::Aload1 | Opcode::Aload2 | Opcode::Aload3 => {
                let index = match instr.opcode {
                    Opcode::Aload0 => 0,
                    Opcode::Aload1 => 1,
                    Opcode::Aload2 => 2,
                    Opcode::Aload3 => 3,
                    _ => 0,
                };
                let local_name = ctx.get_local(index);
                ctx.push(local_name);
            }

            // ==================== 局部变量存储指令 ====================
            Opcode::Istore | Opcode::Lstore | Opcode::Fstore | Opcode::Dstore | Opcode::Astore => {
                let index = u16::from_le_bytes([instr.operands[0], instr.operands[1]]);
                if let Some(value) = ctx.pop() {
                    let local_name = format!("%local{}", index);
                    ir.push_str(&format!("  {} = {}\n", local_name, value));
                    ctx.set_local(index, local_name);
                }
            }

            Opcode::Istore0 | Opcode::Istore1 | Opcode::Istore2 | Opcode::Istore3 => {
                let index = match instr.opcode {
                    Opcode::Istore0 => 0,
                    Opcode::Istore1 => 1,
                    Opcode::Istore2 => 2,
                    Opcode::Istore3 => 3,
                    _ => 0,
                };
                if let Some(value) = ctx.pop() {
                    let local_name = format!("%local{}", index);
                    ir.push_str(&format!("  {} = {}\n", local_name, value));
                    ctx.set_local(index, local_name);
                }
            }

            Opcode::Astore0 | Opcode::Astore1 | Opcode::Astore2 | Opcode::Astore3 => {
                let index = match instr.opcode {
                    Opcode::Astore0 => 0,
                    Opcode::Astore1 => 1,
                    Opcode::Astore2 => 2,
                    Opcode::Astore3 => 3,
                    _ => 0,
                };
                if let Some(value) = ctx.pop() {
                    let local_name = format!("%local{}", index);
                    ir.push_str(&format!("  {} = {}\n", local_name, value));
                    ctx.set_local(index, local_name);
                }
            }

            // ==================== 栈操作指令 ====================
            Opcode::Pop => {
                let _ = ctx.pop();
            }

            Opcode::Pop2 => {
                let _ = ctx.pop();
                let _ = ctx.pop();
            }

            Opcode::Dup => {
                if let Some(val) = ctx.peek() {
                    ctx.push(val.clone());
                }
            }

            Opcode::DupX1 => {
                // 复制栈顶值并插入到栈顶下方
                if ctx.operand_stack.len() >= 2 {
                    let top = ctx.pop().unwrap();
                    let second = ctx.pop().unwrap();
                    ctx.push(top.clone());
                    ctx.push(second);
                    ctx.push(top);
                }
            }

            Opcode::Swap => {
                if ctx.operand_stack.len() >= 2 {
                    let top = ctx.pop().unwrap();
                    let second = ctx.pop().unwrap();
                    ctx.push(top);
                    ctx.push(second);
                }
            }

            // ==================== 算术运算指令 ====================
            Opcode::Iadd => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = add i32 {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Ladd => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = add i64 {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Fadd => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = fadd float {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Dadd => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = fadd double {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Isub => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = sub i32 {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Lsub => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = sub i64 {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Fsub => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = fsub float {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Dsub => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = fsub double {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Imul => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = mul i32 {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Lmul => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = mul i64 {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Fmul => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = fmul float {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Dmul => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = fmul double {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Idiv => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = sdiv i32 {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Ldiv => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = sdiv i64 {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Fdiv => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = fdiv float {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Ddiv => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = fdiv double {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Irem => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = srem i32 {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Lrem => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = srem i64 {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Ineg => {
                if let Some(val) = ctx.pop() {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = sub i32 0, {}\n", temp, val));
                    ctx.push(temp);
                }
            }

            Opcode::Lneg => {
                if let Some(val) = ctx.pop() {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = sub i64 0, {}\n", temp, val));
                    ctx.push(temp);
                }
            }

            // ==================== 位运算指令 ====================
            Opcode::Ishl => {
                if let (Some(shift), Some(val)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = shl i32 {}, {}\n", temp, val, shift));
                    ctx.push(temp);
                }
            }

            Opcode::Ishr => {
                if let (Some(shift), Some(val)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = ashr i32 {}, {}\n", temp, val, shift));
                    ctx.push(temp);
                }
            }

            Opcode::Iand => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = and i32 {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Ior => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = or i32 {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            Opcode::Ixor => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = xor i32 {}, {}\n", temp, left, right));
                    ctx.push(temp);
                }
            }

            // ==================== 比较指令 ====================
            Opcode::IfIcmpeq
            | Opcode::IfIcmpne
            | Opcode::IfIcmplt
            | Opcode::IfIcmpge
            | Opcode::IfIcmpgt
            | Opcode::IfIcmple => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    let pred = match instr.opcode {
                        Opcode::IfIcmpeq => "eq",
                        Opcode::IfIcmpne => "ne",
                        Opcode::IfIcmplt => "slt",
                        Opcode::IfIcmpge => "sge",
                        Opcode::IfIcmpgt => "sgt",
                        Opcode::IfIcmple => "sle",
                        _ => "eq",
                    };
                    ir.push_str(&format!(
                        "  {} = icmp {} i32 {}, {}\n",
                        temp, pred, left, right
                    ));
                    ctx.push(temp);
                }
            }

            Opcode::Lcmp => {
                if let (Some(right), Some(left)) = (ctx.pop(), ctx.pop()) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = icmp sgt i64 {}, {}\n", temp, left, right));
                    let temp2 = ctx.next_temp();
                    ir.push_str(&format!("  {} = icmp slt i64 {}, {}\n", temp2, left, right));
                    let temp3 = ctx.next_temp();
                    ir.push_str(&format!("  {} = select i1 {}, i32 1, i32 0\n", temp3, temp));
                    let temp4 = ctx.next_temp();
                    ir.push_str(&format!(
                        "  {} = select i1 {}, i32 -1, i32 {}\n",
                        temp4, temp2, temp3
                    ));
                    ctx.push(temp4);
                }
            }

            // ==================== 条件跳转指令 ====================
            Opcode::Ifeq => {
                let offset = i16::from_le_bytes([instr.operands[0], instr.operands[1]]);
                if let Some(val) = ctx.pop() {
                    // 生成条件分支：如果 val == 0，则跳转到目标位置
                    let label_true = ctx.next_label();
                    let label_false = ctx.next_label();

                    // 比较 val 与 0
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = icmp eq i32 {}, 0\n", temp, val));
                    ir.push_str(&format!(
                        "  br i1 {}, label %{}, label %{}\n",
                        temp, label_true, label_false
                    ));
                    ir.push_str(&format!("{}:\n", label_false));
                }
            }

            Opcode::Ifne => {
                let offset = i16::from_le_bytes([instr.operands[0], instr.operands[1]]);
                if let Some(val) = ctx.pop() {
                    // 生成条件分支：如果 val != 0，则跳转到目标位置
                    let label_true = ctx.next_label();
                    let label_false = ctx.next_label();

                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = icmp ne i32 {}, 0\n", temp, val));
                    ir.push_str(&format!(
                        "  br i1 {}, label %{}, label %{}\n",
                        temp, label_true, label_false
                    ));
                    ir.push_str(&format!("{}:\n", label_false));
                }
            }

            Opcode::Goto => {
                let offset = i16::from_le_bytes([instr.operands[0], instr.operands[1]]);
                let label = ctx.next_label();
                ir.push_str(&format!("  br label %{}\n", label));
                ir.push_str(&format!("{}:\n", label));
            }

            // ==================== 方法调用指令 ====================
            Opcode::Invokestatic => {
                let index = u16::from_le_bytes([instr.operands[0], instr.operands[1]]);
                if let Some(name) = ctx.pool.get_utf8(index) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = call i32 @{}()\n", temp, name));
                    ctx.push(temp);
                }
            }

            Opcode::Invokefunction => {
                let index = u16::from_le_bytes([instr.operands[0], instr.operands[1]]);
                if let Some(name) = ctx.pool.get_utf8(index) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = call i32 @{}()\n", temp, name));
                    ctx.push(temp);
                }
            }

            // ==================== 返回指令 ====================
            Opcode::Return => {
                ir.push_str("  ret void\n");
            }

            Opcode::Ireturn => {
                if let Some(val) = ctx.pop() {
                    ir.push_str(&format!("  ret i32 {}\n", val));
                } else {
                    ir.push_str("  ret i32 0\n");
                }
            }

            Opcode::Lreturn => {
                if let Some(val) = ctx.pop() {
                    ir.push_str(&format!("  ret i64 {}\n", val));
                } else {
                    ir.push_str("  ret i64 0\n");
                }
            }

            Opcode::Freturn => {
                if let Some(val) = ctx.pop() {
                    ir.push_str(&format!("  ret float {}\n", val));
                } else {
                    ir.push_str("  ret float 0.0\n");
                }
            }

            Opcode::Dreturn => {
                if let Some(val) = ctx.pop() {
                    ir.push_str(&format!("  ret double {}\n", val));
                } else {
                    ir.push_str("  ret double 0.0\n");
                }
            }

            Opcode::Areturn => {
                if let Some(val) = ctx.pop() {
                    ir.push_str(&format!("  ret i8* {}\n", val));
                } else {
                    ir.push_str("  ret i8* null\n");
                }
            }

            // ==================== 类型转换指令 ====================
            Opcode::I2l => {
                if let Some(val) = ctx.pop() {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = sext i32 {} to i64\n", temp, val));
                    ctx.push(temp);
                }
            }

            Opcode::I2f => {
                if let Some(val) = ctx.pop() {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = sitofp i32 {} to float\n", temp, val));
                    ctx.push(temp);
                }
            }

            Opcode::I2d => {
                if let Some(val) = ctx.pop() {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = sitofp i32 {} to double\n", temp, val));
                    ctx.push(temp);
                }
            }

            Opcode::L2i => {
                if let Some(val) = ctx.pop() {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = trunc i64 {} to i32\n", temp, val));
                    ctx.push(temp);
                }
            }

            Opcode::F2i => {
                if let Some(val) = ctx.pop() {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = fptosi float {} to i32\n", temp, val));
                    ctx.push(temp);
                }
            }

            Opcode::D2i => {
                if let Some(val) = ctx.pop() {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = fptosi double {} to i32\n", temp, val));
                    ctx.push(temp);
                }
            }

            // ==================== 数组操作指令 ====================
            Opcode::Arraylength => {
                if let Some(arr) = ctx.pop() {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!(
                        "  {} = call i32 @cavvy_array_length(i8* {})\n",
                        temp, arr
                    ));
                    ctx.push(temp);
                }
            }

            Opcode::Newarray => {
                let type_index = u16::from_le_bytes([instr.operands[0], instr.operands[1]]);
                if let Some(len) = ctx.pop() {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!(
                        "  {} = call i8* @cavvy_array_new(i32 {}, i32 {})\n",
                        temp, len, type_index
                    ));
                    ctx.push(temp);
                }
            }

            // ==================== 对象操作指令 ====================
            Opcode::New => {
                let index = u16::from_le_bytes([instr.operands[0], instr.operands[1]]);
                if let Some(class_name) = ctx.pool.get_utf8(index) {
                    let temp = ctx.next_temp();
                    ir.push_str(&format!("  {} = call i8* @malloc(i64 64)\n", temp));
                    ctx.push(temp);
                }
            }

            // ==================== 未实现指令的占位符 ====================
            _ => {
                // 对于未完全实现的指令，生成注释
                ir.push_str(&format!("  ; Unhandled opcode: {:?}\n", instr.opcode));
            }
        }

        Ok(ir)
    }

    /// 获取LLVM类型字符串
    fn get_llvm_type_string(
        &self,
        type_index: ConstantIndex,
        pool: &ConstantPool,
    ) -> Result<String, JitError> {
        if let Some(type_name) = pool.get_utf8(type_index) {
            match type_name {
                "void" => Ok("void".to_string()),
                "int" | "i32" => Ok("i32".to_string()),
                "long" | "i64" => Ok("i64".to_string()),
                "float" => Ok("float".to_string()),
                "double" => Ok("double".to_string()),
                "boolean" | "bool" => Ok("i1".to_string()),
                "char" => Ok("i8".to_string()),
                "String" | "string" => Ok("i8*".to_string()),
                "Object" | "object" => Ok("i8*".to_string()),
                name => {
                    // 检查是否是数组类型
                    if name.ends_with("[]") {
                        Ok("i8*".to_string())
                    } else {
                        Ok(format!("%struct.{}*", name))
                    }
                }
            }
        } else {
            Ok("i8*".to_string())
        }
    }

    /// 获取方法签名
    fn get_method_signature(
        &self,
        method: &MethodDefinition,
        pool: &ConstantPool,
    ) -> Result<String, JitError> {
        let ret_type = self.get_llvm_type_string(method.return_type_index, pool)?;
        let mut sig = format!("{} (", ret_type);

        for (i, param_type_idx) in method.param_type_indices.iter().enumerate() {
            if i > 0 {
                sig.push_str(", ");
            }
            let param_type = self.get_llvm_type_string(*param_type_idx, pool)?;
            sig.push_str(&param_type);
        }

        sig.push(')');
        Ok(sig)
    }

    /// 获取常量值
    fn get_constant_value(
        &self,
        index: ConstantIndex,
        pool: &ConstantPool,
    ) -> Result<String, JitError> {
        if let Some(val) = pool.get_integer(index) {
            Ok(val.to_string())
        } else if let Some(val) = pool.get_long(index) {
            Ok(val.to_string())
        } else if let Some(val) = pool.get_float(index) {
            Ok(format!("0x{:x}", val.to_bits()))
        } else if let Some(val) = pool.get_double(index) {
            Ok(format!("0x{:x}", val.to_bits()))
        } else if let Some(val) = pool.get_string(index) {
            let escaped = val
                .replace("\\", "\\\\")
                .replace("\"", "\\\"")
                .replace("\n", "\\0A")
                .replace("\r", "\\0D")
                .replace("\t", "\\09");
            Ok(format!("c\"{}\\00\"", escaped))
        } else if let Some(val) = pool.get_utf8(index) {
            Ok(format!("\"{}\"", val))
        } else {
            Ok("0".to_string())
        }
    }

    /// 编译IR为对象文件
    fn compile_ir_to_object(
        &self,
        ir_file: &std::path::Path,
        obj_file: &std::path::Path,
    ) -> Result<(), JitError> {
        let clang = find_clang()?;

        let mut cmd = std::process::Command::new(&clang);
        cmd.arg("-c")
            .arg("-x")
            .arg("ir")
            .arg(ir_file)
            .arg(&self.options.optimization)
            .arg("-o")
            .arg(obj_file);

        let output = cmd
            .output()
            .map_err(|e| JitError::CompilationError(format!("Failed to run clang: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(JitError::CompilationError(stderr.to_string()));
        }

        Ok(())
    }

    /// 链接可执行文件
    fn link_executable(
        &self,
        obj_file: &std::path::Path,
        output_path: &str,
    ) -> Result<(), JitError> {
        let clang = find_clang()?;

        let mut cmd = std::process::Command::new(&clang);
        cmd.arg(obj_file)
            .arg(&self.options.optimization)
            .arg("-o")
            .arg(output_path);

        // 添加库搜索路径
        for path in &self.options.lib_paths {
            cmd.arg("-L").arg(path);
        }

        // 添加链接的库
        for lib in &self.options.link_libs {
            cmd.arg(format!("-l{}", lib));
        }

        // 平台特定的库
        if self.options.target.contains("windows") || self.options.target.contains("mingw") {
            // Windows不需要额外的库
        } else {
            // Linux/macOS
            cmd.arg("-lm"); // 数学库
        }

        let output = cmd
            .output()
            .map_err(|e| JitError::LinkingError(format!("Failed to run linker: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(JitError::LinkingError(stderr.to_string()));
        }

        Ok(())
    }
}

impl Default for JitCompiler {
    fn default() -> Self {
        Self::new(JitOptions::default())
    }
}

/// 查找clang编译器
fn find_clang() -> Result<std::path::PathBuf, JitError> {
    // 1. 尝试系统PATH中的clang
    if let Ok(output) = std::process::Command::new("clang")
        .arg("--version")
        .output()
    {
        if output.status.success() {
            return Ok(std::path::PathBuf::from("clang"));
        }
    }

    // 2. 尝试编译器所在目录下的llvm-minimal
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let bundled_clang = exe_dir.join("llvm-minimal/bin/clang.exe");
            if bundled_clang.exists() {
                return Ok(bundled_clang);
            }
        }
    }

    Err(JitError::CompilationError(
        "找不到clang编译器。请确保clang已安装并在PATH中。".to_string(),
    ))
}

/// 便捷函数：编译字节码文件为可执行文件
pub fn compile_bytecode_file(input_path: &str, output_path: &str) -> Result<(), JitError> {
    // 1. 读取字节码文件
    let module = serializer::deserialize_from_file(input_path)
        .map_err(|e| JitError::InvalidBytecode(e.to_string()))?;

    // 2. 编译
    let compiler = JitCompiler::new(JitOptions::default());
    compiler.compile_to_executable(&module, output_path)
}

/// 便捷函数：将字节码模块转换为IR字符串
pub fn bytecode_to_ir(module: &BytecodeModule) -> Result<String, JitError> {
    let compiler = JitCompiler::new(JitOptions::default());
    compiler.bytecode_to_ir(module)
}
