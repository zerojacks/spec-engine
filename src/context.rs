//! 解析上下文 - 管理字段引用作用域

use crate::Value;
use std::collections::HashMap;

/// 字段绑定：同时保留原始字节（供 switch 按十六进制key匹配）
/// 和已解码的 Value（供 repeat/external/dict_ref 按数值引用 ——
/// Value 是用该字段自己声明的编码方式/字节序解出来的，不需要
/// 在引用处重新猜字节序）。
#[derive(Debug, Clone)]
struct Binding {
    raw: Vec<u8>,
    value: Value,
}

type Bindings = HashMap<String, Binding>;

/// 解析上下文，支持嵌套作用域
#[derive(Debug, Default)]
pub struct Context {
    scopes: Vec<Bindings>,
}

impl Context {
    /// 创建新的上下文
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    /// 进入新作用域
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// 离开当前作用域
    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// 绑定字段值：原始字节 + 已解码值一起存
    ///
    /// 绑定是按解析作用域进行的：每次进入容器/模板时会创建新作用域，离开时弹出。
    /// ref 查询时会从当前作用域开始向外查找，最近的同名绑定会优先匹配。
    /// 这使得模板内定义的同名 ref 可以覆盖外层定义，避免不同作用域间的冲突。
    pub fn bind(&mut self, name: &str, raw: Vec<u8>, value: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), Binding { raw, value });
        }
    }

    /// 获取字段的原始字节（从内到外查找）—— 供 switch 按 hex key 匹配
    pub fn get(&self, name: &str) -> Option<&Vec<u8>> {
        for scope in self.scopes.iter().rev() {
            if let Some(b) = scope.get(name) {
                return Some(&b.raw);
            }
        }
        None
    }

    /// 获取字段已解码的 Value（从内到外查找）—— 供 repeat/external/dict_ref
    /// 按数值引用，天然遵循该字段自己声明的字节序，不需要重新猜
    pub fn get_value(&self, name: &str) -> Option<&Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(b) = scope.get(name) {
                return Some(&b.value);
            }
        }
        None
    }
}
