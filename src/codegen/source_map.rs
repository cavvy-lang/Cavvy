//! IR源映射管理
//!
//! 将IR行号映射回Cavvy源代码位置，用于clang错误报告

use std::collections::HashMap;

/// 源位置信息
#[derive(Debug, Clone, PartialEq)]
pub struct SourcePosition {
    pub file: String,
    pub line: usize,
    pub column: usize,
}

impl SourcePosition {
    pub fn new(file: impl Into<String>, line: usize, column: usize) -> Self {
        Self {
            file: file.into(),
            line,
            column,
        }
    }
}

/// IR源映射表
#[derive(Debug, Clone, Default)]
pub struct IRSourceMap {
    /// IR行号 -> 源位置
    mappings: HashMap<usize, SourcePosition>,
    /// 当前IR行号（生成时维护）
    current_ir_line: usize,
}

impl IRSourceMap {
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
            current_ir_line: 1,
        }
    }

    /// 添加映射
    pub fn add_mapping(
        &mut self,
        ir_line: usize,
        source_file: impl Into<String>,
        source_line: usize,
        source_column: usize,
    ) {
        self.mappings.insert(
            ir_line,
            SourcePosition::new(source_file, source_line, source_column),
        );
    }

    /// 获取源位置
    pub fn get_source_position(&self, ir_line: usize) -> Option<&SourcePosition> {
        self.mappings.get(&ir_line)
    }

    /// 递增IR行号
    pub fn advance_line(&mut self) {
        self.current_ir_line += 1;
    }

    /// 获取当前IR行号
    pub fn current_line(&self) -> usize {
        self.current_ir_line
    }

    /// 设置当前IR行号
    pub fn set_current_line(&mut self, line: usize) {
        self.current_ir_line = line;
    }

    /// 序列化为JSON格式
    pub fn to_json(&self) -> String {
        let mut entries: Vec<String> = Vec::new();
        let mut sorted_mappings: Vec<_> = self.mappings.iter().collect();
        sorted_mappings.sort_by_key(|(k, _)| *k);

        for (ir_line, pos) in sorted_mappings {
            entries.push(format!(
                "  \"{}\": {{\"file\": \"{}\", \"line\": {}, \"column\": {}}}",
                ir_line, pos.file, pos.line, pos.column
            ));
        }

        format!("{{\n{}\n}}", entries.join(",\n"))
    }

    /// 从JSON反序列化
    pub fn from_json(json: &str) -> Result<Self, String> {
        let mut map = Self::new();

        // 简单的JSON解析
        for line in json.lines() {
            let line = line.trim();
            if line.is_empty() || line == "{" || line == "}" {
                continue;
            }

            // 解析 "ir_line": {"file": "...", "line": n, "column": m}
            if let Some(colon_pos) = line.find(':') {
                let ir_line_str = line[..colon_pos].trim().trim_matches('"');
                let rest = &line[colon_pos + 1..];

                if let Ok(ir_line) = ir_line_str.parse::<usize>() {
                    let file = Self::extract_json_string(rest, "file");
                    let line_num = Self::extract_json_number(rest, "line");
                    let col_num = Self::extract_json_number(rest, "column");

                    if let (Some(file), Some(line), Some(col)) = (file, line_num, col_num) {
                        map.add_mapping(ir_line, file, line, col);
                    }
                }
            }
        }

        Ok(map)
    }

    fn extract_json_string(json: &str, key: &str) -> Option<String> {
        let pattern = format!("\"{}\": \"", key);
        if let Some(start) = json.find(&pattern) {
            let start = start + pattern.len();
            if let Some(end) = json[start..].find('"') {
                return Some(json[start..start + end].to_string());
            }
        }
        None
    }

    fn extract_json_number(json: &str, key: &str) -> Option<usize> {
        let pattern = format!("\"{}\": ", key);
        if let Some(start) = json.find(&pattern) {
            let start = start + pattern.len();
            let end = json[start..]
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(json[start..].len());
            return json[start..start + end].parse().ok();
        }
        None
    }
}

/// 源映射注释生成器
pub struct SourceMapEmitter {
    source_map: IRSourceMap,
    current_source_file: String,
    current_source_line: usize,
    current_source_column: usize,
}

impl SourceMapEmitter {
    pub fn new(source_file: impl Into<String>) -> Self {
        Self {
            source_map: IRSourceMap::new(),
            current_source_file: source_file.into(),
            current_source_line: 1,
            current_source_column: 1,
        }
    }

    /// 设置当前源位置
    pub fn set_source_position(&mut self, file: impl Into<String>, line: usize, column: usize) {
        self.current_source_file = file.into();
        self.current_source_line = line;
        self.current_source_column = column;
    }

    /// 生成源映射注释
    pub fn emit_source_comment(&mut self, ir_line: usize) -> String {
        self.source_map.add_mapping(
            ir_line,
            self.current_source_file.clone(),
            self.current_source_line,
            self.current_source_column,
        );
        format!(
            "; !source {}:{}:{}",
            self.current_source_file, self.current_source_line, self.current_source_column
        )
    }

    /// 获取源映射
    pub fn get_source_map(&self) -> &IRSourceMap {
        &self.source_map
    }

    /// 获取源映射的可变引用
    pub fn get_source_map_mut(&mut self) -> &mut IRSourceMap {
        &mut self.source_map
    }
}

/// 解析clang错误信息中的行号
pub fn parse_clang_error_line(error_msg: &str) -> Option<usize> {
    // 匹配格式: filename.ll:123:45: error: ...
    // 或: <stdin>:123:45: error: ...
    for line in error_msg.lines() {
        // 查找 .ll: 后的数字
        if let Some(pos) = line.find(".ll:") {
            let rest = &line[pos + 4..];
            if let Some(colon_pos) = rest.find(':') {
                let line_num_str = &rest[..colon_pos];
                if let Ok(line_num) = line_num_str.parse::<usize>() {
                    return Some(line_num);
                }
            }
        }

        // 匹配 <stdin>: 格式
        if let Some(pos) = line.find("<stdin>:") {
            let rest = &line[pos + 8..];
            if let Some(colon_pos) = rest.find(':') {
                let line_num_str = &rest[..colon_pos];
                if let Ok(line_num) = line_num_str.parse::<usize>() {
                    return Some(line_num);
                }
            }
        }
    }
    None
}

/// 将clang错误信息中的IR行号替换为源位置
pub fn remap_clang_error(error_msg: &str, source_map: &IRSourceMap) -> String {
    let mut result = String::new();

    for line in error_msg.lines() {
        if let Some(ir_line) = parse_clang_error_line(line) {
            if let Some(source_pos) = source_map.get_source_position(ir_line) {
                // 替换行号信息
                let new_line = if line.contains(".ll:") {
                    line.replace(
                        &format!(".ll:{}:", ir_line),
                        &format!(
                            " ({}:{}:{})",
                            source_pos.file, source_pos.line, source_pos.column
                        ),
                    )
                } else if line.contains("<stdin>:") {
                    line.replace(
                        &format!("<stdin>:{}:", ir_line),
                        &format!(
                            " ({}:{}:{})",
                            source_pos.file, source_pos.line, source_pos.column
                        ),
                    )
                } else {
                    line.to_string()
                };
                result.push_str(&new_line);
            } else {
                result.push_str(line);
            }
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }

    result.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_map_basic() {
        let mut map = IRSourceMap::new();
        map.add_mapping(10, "test.cay", 5, 3);
        map.add_mapping(20, "test.cay", 8, 1);

        assert_eq!(
            map.get_source_position(10),
            Some(&SourcePosition::new("test.cay", 5, 3))
        );
        assert_eq!(
            map.get_source_position(20),
            Some(&SourcePosition::new("test.cay", 8, 1))
        );
        assert_eq!(map.get_source_position(30), None);
    }

    #[test]
    fn test_parse_clang_error() {
        let error = "test.ll:123:45: error: invalid syntax";
        assert_eq!(parse_clang_error_line(error), Some(123));

        let error2 = "<stdin>:456:10: error: type mismatch";
        assert_eq!(parse_clang_error_line(error2), Some(456));
    }

    #[test]
    fn test_remap_clang_error() {
        let mut map = IRSourceMap::new();
        map.add_mapping(123, "test.cay", 10, 5);

        let error = "test.ll:123:45: error: invalid syntax";
        let remapped = remap_clang_error(error, &map);

        assert!(remapped.contains("test.cay:10:5"));
        assert!(!remapped.contains("test.ll:123"));
    }
}
