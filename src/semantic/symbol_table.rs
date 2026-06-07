//! 语义分析符号表

use crate::types::Type;
use std::collections::HashMap;

/// 语义分析符号表
pub struct SemanticSymbolTable {
    scopes: Vec<HashMap<String, SemanticSymbolInfo>>,
}

/// 符号信息
#[derive(Debug, Clone)]
pub struct SemanticSymbolInfo {
    pub name: String,
    pub symbol_type: Type,
    pub is_final: bool,
    pub is_initialized: bool,
}

impl SemanticSymbolTable {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    pub fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn exit_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn declare(&mut self, name: String, info: SemanticSymbolInfo) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, info);
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&SemanticSymbolInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }
        None
    }

    pub fn lookup_current(&self, name: &str) -> Option<&SemanticSymbolInfo> {
        self.scopes.last().and_then(|s| s.get(name))
    }

    /// 更新已存在符号的信息（用于修改 is_initialized 等）
    pub fn update(&mut self, name: &str, info: SemanticSymbolInfo) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), info);
                return true;
            }
        }
        false
    }
}

impl Default for SemanticSymbolTable {
    fn default() -> Self {
        Self::new()
    }
}
