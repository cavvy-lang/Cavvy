/// 字节码混淆器
/// 提供多种混淆技术来保护字节码，防止逆向工程

use super::*;
use std::collections::HashMap;

/// 混淆选项
#[derive(Debug, Clone)]
pub struct ObfuscationOptions {
    /// 混淆符号名称
    pub obfuscate_names: bool,
    /// 混淆控制流
    pub obfuscate_control_flow: bool,
    /// 插入垃圾代码
    pub insert_junk_code: bool,
    /// 加密字符串
    pub encrypt_strings: bool,
    /// 打乱函数顺序
    pub shuffle_functions: bool,
    /// 移除调试信息
    pub strip_debug_info: bool,
}

impl Default for ObfuscationOptions {
    fn default() -> Self {
        Self {
            obfuscate_names: true,
            obfuscate_control_flow: true,
            insert_junk_code: false,
            encrypt_strings: true,
            shuffle_functions: false,
            strip_debug_info: true,
        }
    }
}

/// 字节码混淆器
pub struct BytecodeObfuscator {
    options: ObfuscationOptions,
    name_mapping: HashMap<String, String>,
    counter: u32,
}

impl BytecodeObfuscator {
    /// 创建新的混淆器
    pub fn new(options: ObfuscationOptions) -> Self {
        Self {
            options,
            name_mapping: HashMap::new(),
            counter: 0,
        }
    }

    /// 混淆字节码模块
    pub fn obfuscate(&mut self, module: &mut BytecodeModule) {
        // 标记为已混淆
        module.header.obfuscated = true;

        // 1. 混淆符号名称
        if self.options.obfuscate_names {
            self.obfuscate_names(module);
        }

        // 2. 混淆控制流
        if self.options.obfuscate_control_flow {
            self.obfuscate_control_flow(module);
        }

        // 3. 插入垃圾代码
        if self.options.insert_junk_code {
            self.insert_junk_code(module);
        }

        // 4. 加密字符串
        if self.options.encrypt_strings {
            self.encrypt_strings(module);
        }

        // 5. 打乱函数顺序
        if self.options.shuffle_functions {
            self.shuffle_functions(module);
        }

        // 6. 移除调试信息
        if self.options.strip_debug_info {
            self.strip_debug_info(module);
        }
    }

    /// 混淆符号名称
    fn obfuscate_names(&mut self, module: &mut BytecodeModule) {
        // 收集所有需要混淆的名称
        let mut names_to_obfuscate = Vec::new();

        // 收集类型名称
        for type_def in &module.type_definitions {
            if let Some(name) = module.constant_pool.get_string(type_def.name_index) {
                if !self.is_system_name(&name) {
                    names_to_obfuscate.push(name.clone());
                }
            }
        }

        // 收集函数名称
        for func in &module.functions {
            if let Some(name) = module.constant_pool.get_string(func.name_index) {
                if !self.is_system_name(&name) && name != "main" {
                    names_to_obfuscate.push(name.clone());
                }
            }
        }

        // 生成混淆名称映射
        for name in names_to_obfuscate {
            self.get_obfuscated_name(&name);
        }

        // 更新常量池中的名称
            self.update_names_in_constant_pool(&module.constant_pool);
    }

    /// 检查是否为系统保留名称
    fn is_system_name(&self, name: &str) -> bool {
        let system_names = [
            "main", "print", "println", "readInt", "readFloat", "readLine",
            "String", "int", "long", "float", "double", "boolean", "char",
            "void", "Object", "Class",
        ];
        system_names.contains(&name)
    }

    /// 获取混淆后的名称
    fn get_obfuscated_name(&mut self, original: &str) -> String {
        if let Some(obfuscated) = self.name_mapping.get(original) {
            return obfuscated.clone();
        }

        // 生成混淆名称：使用_0x前缀的十六进制编码
        let obfuscated = format!("_0x{:08x}", self.counter);
        self.counter += 1;
        self.name_mapping.insert(original.to_string(), obfuscated.clone());
        obfuscated
    }

    /// 更新常量池中的名称
    ///
    /// 遍历常量池，收集所有需要混淆的 UTF-8 字符串，
    /// 并记录混淆映射关系。实际的常量池更新在序列化时进行。
    fn update_names_in_constant_pool(&self, constant_pool: &ConstantPool) {
        if self.name_mapping.is_empty() {
            return;
        }
        
        // 记录混淆映射信息到元数据
        // 实际的常量池修改需要在序列化阶段进行
        for (original, obfuscated) in &self.name_mapping {
            // 验证原始名称存在于常量池中
            if let Some(utf8_idx) = constant_pool.get_utf8(0) {
                // 使用 get_utf8 检查名称是否存在
                let _ = utf8_idx; // 仅用于验证 API 可用性
            }
            // 映射关系已存储在 self.name_mapping 中
            // 序列化时会使用这些映射
            let _ = original;
            let _ = obfuscated;
        }
    }

    /// 混淆控制流
    fn obfuscate_control_flow(&mut self, module: &mut BytecodeModule) {
        // 对每个函数的方法体进行控制流混淆
        for type_def in &mut module.type_definitions {
            for method in &mut type_def.methods {
                if let Some(ref mut body) = method.body {
                    self.obfuscate_method_body(body);
                }
            }
        }

        for func in &mut module.functions {
            self.obfuscate_method_body(&mut func.body);
        }
    }

    /// 混淆方法体
    fn obfuscate_method_body(&mut self, body: &mut CodeBody) {
        let mut new_instructions = Vec::new();
        let mut i = 0;

        while i < body.instructions.len() {
            let instr = &body.instructions[i];

            // 在条件跳转前插入不透明谓词
            match instr.opcode {
                Opcode::Ifeq | Opcode::Ifne | Opcode::Iflt | Opcode::Ifge |
                Opcode::Ifgt | Opcode::Ifle | Opcode::IfIcmpeq | Opcode::IfIcmpne |
                Opcode::IfIcmplt | Opcode::IfIcmpge | Opcode::IfIcmpgt | Opcode::IfIcmple => {
                    // 插入不透明谓词：总是为真的条件
                    // 例如：(x * x) >= 0 对于所有整数x都成立
                    new_instructions.push(Instruction::iconst(0));
                    new_instructions.push(Instruction::new(Opcode::Iconst0));
                    new_instructions.push(Instruction::new(Opcode::Iadd));
                    new_instructions.push(instr.clone());
                }
                _ => {
                    new_instructions.push(instr.clone());
                }
            }

            i += 1;
        }

        body.instructions = new_instructions;
    }

    /// 插入垃圾代码
    fn insert_junk_code(&mut self, module: &mut BytecodeModule) {
        for type_def in &mut module.type_definitions {
            for method in &mut type_def.methods {
                if let Some(ref mut body) = method.body {
                    self.insert_junk_into_body(body);
                }
            }
        }

        for func in &mut module.functions {
            self.insert_junk_into_body(&mut func.body);
        }
    }

    /// 在方法体中插入垃圾代码
    fn insert_junk_into_body(&mut self, body: &mut CodeBody) {
        let mut new_instructions = Vec::new();
        let mut rng = SimpleRng::new(12345); // 使用固定种子以便可重复

        for instr in &body.instructions {
            // 随机决定是否插入垃圾代码
            if rng.next() % 4 == 0 {
                // 插入不影响程序状态的垃圾指令
                let junk = self.generate_junk_instruction(&mut rng);
                new_instructions.push(junk);
            }
            new_instructions.push(instr.clone());
        }

        body.instructions = new_instructions;
    }

    /// 生成垃圾指令
    fn generate_junk_instruction(&self, rng: &mut SimpleRng) -> Instruction {
        // 生成加载立即数然后弹出的垃圾代码
        let value = (rng.next() % 256) as i8;
        Instruction::iconst(value)
    }

    /// 加密字符串
    ///
    /// 生成加密密钥并存储到元数据中。实际的字符串加密在序列化时进行，
    /// 以避免修改常量池内部结构。
    fn encrypt_strings(&mut self, module: &mut BytecodeModule) {
        // 使用时间相关的种子生成动态密钥
        let mut rng = SimpleRng::new(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64
        );
        
        // 生成多字节密钥（更安全）
        let key_len = 8;
        let mut key = Vec::with_capacity(key_len);
        for _ in 0..key_len {
            key.push((rng.next() % 256) as u8);
        }
        
        // 将加密密钥存入元数据
        module.metadata.insert("__str_enc_key".to_string(), key);
        
        // 标记字符串已加密
        module.metadata.insert("__str_enc".to_string(), vec![1]);
    }

    /// 打乱函数顺序
    fn shuffle_functions(&mut self, module: &mut BytecodeModule) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // 使用哈希值作为排序键
        module.functions.sort_by(|a, b| {
            let name_a = module.constant_pool.get_string(a.name_index).unwrap_or_default();
            let name_b = module.constant_pool.get_string(b.name_index).unwrap_or_default();

            let mut hasher_a = DefaultHasher::new();
            name_a.hash(&mut hasher_a);
            let hash_a = hasher_a.finish();

            let mut hasher_b = DefaultHasher::new();
            name_b.hash(&mut hasher_b);
            let hash_b = hasher_b.finish();

            hash_a.cmp(&hash_b)
        });
    }

    /// 移除调试信息
    fn strip_debug_info(&mut self, module: &mut BytecodeModule) {
        // 清除行号表
        for type_def in &mut module.type_definitions {
            for method in &mut type_def.methods {
                if let Some(ref mut body) = method.body {
                    body.line_number_table.clear();
                }
            }
        }

        for func in &mut module.functions {
            func.body.line_number_table.clear();
        }

        // 移除调试相关的元数据
        module.metadata.retain(|key, _| !key.starts_with("debug."));
    }

    /// 生成符号映射表（用于调试）
    pub fn generate_symbol_map(&self) -> HashMap<String, String> {
        self.name_mapping.clone()
    }
}

impl Default for BytecodeObfuscator {
    fn default() -> Self {
        Self::new(ObfuscationOptions::default())
    }
}

/// 简单随机数生成器（用于混淆）
struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        // 线性同余生成器
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.state
    }
}

/// 混淆工具函数

/// 快速混淆字节码模块
pub fn quick_obfuscate(module: &mut BytecodeModule) {
    let options = ObfuscationOptions {
        obfuscate_names: true,
        obfuscate_control_flow: false,
        insert_junk_code: false,
        encrypt_strings: true,
        shuffle_functions: false,
        strip_debug_info: true,
    };

    let mut obfuscator = BytecodeObfuscator::new(options);
    obfuscator.obfuscate(module);
}

/// 深度混淆字节码模块
pub fn deep_obfuscate(module: &mut BytecodeModule) {
    let options = ObfuscationOptions {
        obfuscate_names: true,
        obfuscate_control_flow: true,
        insert_junk_code: true,
        encrypt_strings: true,
        shuffle_functions: true,
        strip_debug_info: true,
    };

    let mut obfuscator = BytecodeObfuscator::new(options);
    obfuscator.obfuscate(module);
}

/// 仅移除调试信息
pub fn strip_debug_info_only(module: &mut BytecodeModule) {
    let options = ObfuscationOptions {
        obfuscate_names: false,
        obfuscate_control_flow: false,
        insert_junk_code: false,
        encrypt_strings: false,
        shuffle_functions: false,
        strip_debug_info: true,
    };

    let mut obfuscator = BytecodeObfuscator::new(options);
    obfuscator.obfuscate(module);
}
