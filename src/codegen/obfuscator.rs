use std::collections::HashMap;

/// IR 代码混淆器
pub struct IRObfuscator {
    symbol_map: HashMap<String, String>,
    counter: u32,
}

impl IRObfuscator {
    pub fn new() -> Self {
        Self {
            symbol_map: HashMap::new(),
            counter: 0,
        }
    }

    /// 混淆函数名和变量名
    pub fn obfuscate_symbol(&mut self, original_name: &str) -> String {
        if let Some(obfuscated) = self.symbol_map.get(original_name) {
            return obfuscated.clone();
        }

        let obfuscated = format!("__obf_{:x}", self.counter);
        self.counter += 1;
        self.symbol_map
            .insert(original_name.to_string(), obfuscated.clone());
        obfuscated
    }

    /// 混淆整个 IR 代码
    pub fn obfuscate_ir(&mut self, ir_code: &str) -> String {
        let mut result = String::new();
        let lines: Vec<&str> = ir_code.lines().collect();

        for line in lines {
            let mut processed_line = line.to_string();

            // 混淆函数定义和声明
            if line.trim().starts_with("define") || line.trim().starts_with("declare") {
                if let Some(pos) = line.find('@') {
                    if let Some(end_pos) = line[pos + 1..].find(|c: char| c == '(' || c == ' ') {
                        let symbol_start = pos + 1;
                        let symbol_end = pos + 1 + end_pos;
                        let original_symbol = &line[symbol_start..symbol_end];
                        if !original_symbol.starts_with("llvm.")
                            && !original_symbol.starts_with("__obf_")
                        {
                            let obfuscated = self.obfuscate_symbol(original_symbol);
                            processed_line = format!(
                                "{}{}{}",
                                &line[..symbol_start],
                                obfuscated,
                                &line[symbol_end..]
                            );
                        }
                    }
                }
            }

            // 混淆函数调用和变量引用
            if line.contains('@') && !line.trim().starts_with(";") {
                let mut last_pos = 0;
                while let Some(pos) = processed_line[last_pos..].find('@') {
                    let actual_pos = last_pos + pos;
                    let remaining = &processed_line[actual_pos + 1..];

                    if let Some(end_pos) =
                        remaining.find(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
                    {
                        let symbol = &remaining[..end_pos];
                        if !symbol.starts_with("llvm.")
                            && !symbol.starts_with("__obf_")
                            && !symbol.is_empty()
                        {
                            let obfuscated = self.obfuscate_symbol(symbol);
                            processed_line = format!(
                                "{}{}{}{}",
                                &processed_line[..actual_pos + 1],
                                obfuscated,
                                &remaining[end_pos..],
                                &processed_line[actual_pos + 1 + remaining.len()..]
                            );
                            last_pos = actual_pos + 1 + obfuscated.len();
                        } else {
                            last_pos = actual_pos + 1;
                        }
                    } else {
                        break;
                    }
                }
            }

            result.push_str(&processed_line);
            result.push('\n');
        }

        result
    }

    /// 获取符号映射表（用于调试）
    pub fn get_symbol_map(&self) -> &HashMap<String, String> {
        &self.symbol_map
    }
}

impl Default for IRObfuscator {
    fn default() -> Self {
        Self::new()
    }
}
