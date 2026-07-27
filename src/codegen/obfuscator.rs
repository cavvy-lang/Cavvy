use std::collections::{HashMap, HashSet};

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
        // 预扫描：收集不可混淆的符号 —
        // 1. `declare` 声明的外部符号：定义在模块外（libc、cayrt 等），
        //    重命名后链接器找不到原始符号，产物无法链接
        // 2. 入口函数 main：被 C 运行时启动代码引用
        let mut preserved: HashSet<String> = HashSet::new();
        preserved.insert("main".to_string());
        for line in ir_code.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("declare") {
                if let Some(pos) = trimmed.find('@') {
                    if let Some(end_pos) =
                        trimmed[pos + 1..].find(|c: char| c == '(' || c == ' ')
                    {
                        preserved.insert(trimmed[pos + 1..pos + 1 + end_pos].to_string());
                    }
                }
            }
        }

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
                            && !preserved.contains(original_symbol)
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
            // 注意：必须跳过 c"..." 字符串字面量内容 —— 字面量中的 @
            // （如 c"email: a@b.com\00"）不是符号引用，替换它会破坏字符串
            // 内容并使其与声明的长度不符。
            if line.contains('@') && !line.trim().starts_with(";") {
                let mut out = String::with_capacity(processed_line.len());
                let mut rest: &str = &processed_line;
                while !rest.is_empty() {
                    // c"..." 字符串字面量：整体原样拷贝。
                    // LLVM c 字符串中 `"` 会被转义为 \22，因此 raw `"` 必然终止字面量。
                    if rest.starts_with("c\"") {
                        let end = rest[2..]
                            .find('"')
                            .map(|p| p + 3)
                            .unwrap_or(rest.len());
                        out.push_str(&rest[..end]);
                        rest = &rest[end..];
                        continue;
                    }
                    if rest.starts_with('@') {
                        let remaining = &rest[1..];
                        let symbol_end = remaining
                            .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
                            .unwrap_or(remaining.len());
                        let symbol = &remaining[..symbol_end];
                        if !symbol.starts_with("llvm.")
                            && !symbol.starts_with("__obf_")
                            && !symbol.is_empty()
                            && !preserved.contains(symbol)
                        {
                            let obfuscated = self.obfuscate_symbol(symbol);
                            out.push('@');
                            out.push_str(&obfuscated);
                        } else {
                            out.push('@');
                            out.push_str(symbol);
                        }
                        rest = &remaining[symbol_end..];
                        continue;
                    }
                    // 普通字符：按 UTF-8 边界原样拷贝
                    let ch_len = rest.chars().next().expect("rest 非空").len_utf8();
                    out.push_str(&rest[..ch_len]);
                    rest = &rest[ch_len..];
                }
                processed_line = out;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 含 @ 的字符串字面量内容必须原样保留，符号则被混淆
    #[test]
    fn test_string_literal_content_not_obfuscated() {
        let mut obf = IRObfuscator::new();
        let ir = concat!(
            "@.str.0 = private constant [16 x i8] c\"email: a@b.com\\00\"\n",
            "define i32 @my_func() {\n",
            "  %1 = call i32 @my_helper(i8* getelementptr ([16 x i8], [16 x i8]* @.str.0, i64 0, i64 0))\n",
            "  ret i32 %1\n",
            "}\n",
        );
        let out = obf.obfuscate_ir(ir);
        // 字符串字面量内容（含 @b.com）必须逐字节保留
        assert!(
            out.contains("c\"email: a@b.com\\00\""),
            "字符串字面量内容被混淆器破坏:\n{}",
            out
        );
        // 符号应被混淆且全文一致
        assert!(!out.contains("@my_func"), "函数符号未被混淆:\n{}", out);
        assert!(!out.contains("@my_helper"), "调用符号未被混淆:\n{}", out);
        assert!(out.contains("@__obf_"));
        // 同一符号的多次出现应映射为同一个混淆名
        let map = obf.get_symbol_map();
        assert!(map.contains_key("my_func"));
        assert!(map.contains_key("my_helper"));
        assert!(map.contains_key(".str.0"));
    }

    /// llvm.* 内建符号不应被混淆
    #[test]
    fn test_llvm_intrinsic_not_obfuscated() {
        let mut obf = IRObfuscator::new();
        let ir = "  call void @llvm.memset.p0i8.i64(i8* %p, i8 0, i64 8, i1 false)\n";
        let out = obf.obfuscate_ir(ir);
        assert!(out.contains("@llvm.memset.p0i8.i64"), "{}", out);
    }

    /// declare 的外部符号与入口 main 不应被混淆，否则产物无法链接
    #[test]
    fn test_declared_and_main_symbols_preserved() {
        let mut obf = IRObfuscator::new();
        let ir = concat!(
            "declare i32 @printf(i8*, ...)\n",
            "define i32 @main() {\n",
            "  %1 = call i32 (i8*, ...) @printf(i8* %fmt)\n",
            "  %2 = call i32 @my_internal()\n",
            "  ret i32 0\n",
            "}\n",
            "define internal i32 @my_internal() {\n",
            "  ret i32 1\n",
            "}\n",
        );
        let out = obf.obfuscate_ir(ir);
        // 外部声明与入口保持原名
        assert!(out.contains("declare i32 @printf(i8*, ...)"), "{}", out);
        assert!(out.contains("define i32 @main()"), "{}", out);
        assert!(out.contains("@printf(i8* %fmt)"), "{}", out);
        // 模块内定义的符号仍然被混淆，且定义与调用一致
        assert!(!out.contains("@my_internal"), "{}", out);
        let map = obf.get_symbol_map();
        assert!(map.contains_key("my_internal"));
        assert!(!map.contains_key("printf"));
        assert!(!map.contains_key("main"));
    }
}
