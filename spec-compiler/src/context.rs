//! 编译上下文模块
//!
//! 提供编译期间的上下文管理，包括：
//! - BuildCtx: 全局编译上下文，存储模板、原始DI映射、注册列表
//! - BuildScope: 作用域栈，用于 ref_id 引用的验证和查找

use crate::ast::{RawField, RawTemplate};
use crate::types::{FieldSpec, NamedField};
use std::collections::{HashMap, HashSet};

/// 编译上下文
///
/// 存储整个编译过程中需要共享的数据：
/// - 模板定义（用于 template_ref 展开）
/// - DI原始定义映射（用于 di_sequence 展开）
/// - 待注册的 DI 列表
pub struct BuildCtx {
    /// 模板是纯结构定义（不挂 DI 号）。按照 (id, protocol, region, dir) 进行查找，
    /// protocol/region/dir 与 data_items 的 id 查找规则一致。
    pub templates: HashMap<(String, String, String, Option<String>), RawTemplate>,
    
    /// 顶层 data_items 里 (id, protocol, region, dir) -> 原始定义，供
    /// di_sequence 解析引用。同一个 id 在不同 protocol/region 下可以有不同
    /// 定义，key 必须把它们都带上。
    pub di_raw_map: HashMap<(String, String, String, Option<String>), RawField>,
    
    /// (DI数值, protocol, region, dir, 该节点展开好的 NamedField 值)——不再是
    /// 源码字符串，是真正可以直接 bincode 序列化的数据。
    pub registrations: Vec<(u32, String, String, Option<String>, NamedField)>,
}

impl BuildCtx {
    /// 创建新的编译上下文
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
            di_raw_map: HashMap::new(),
            registrations: Vec::new(),
        }
    }

    /// 使用已有的模板和DI映射创建编译上下文
    pub fn with_data(
        templates: HashMap<(String, String, String, Option<String>), RawTemplate>,
        di_raw_map: HashMap<(String, String, String, Option<String>), RawField>,
    ) -> Self {
        Self {
            templates,
            di_raw_map,
            registrations: Vec::new(),
        }
    }

    /// 注册一个 DI
    pub fn register(
        &mut self,
        id: u32,
        protocol: String,
        region: String,
        dir: Option<String>,
        named_field: NamedField,
    ) {
        self.registrations.push((id, protocol, region, dir, named_field));
    }

    /// 获取已注册的 DI（如果存在）
    pub fn get_registered(
        &self,
        id: u32,
        protocol: &str,
        region: &str,
        dir: &Option<String>,
    ) -> Option<&NamedField> {
        self.registrations
            .iter()
            .find(|(reg_id, reg_protocol, reg_region, reg_dir, _)| {
                *reg_id == id && reg_protocol == protocol && reg_region == region && reg_dir == dir
            })
            .map(|(_, _, _, _, named_field)| named_field)
    }
}

impl Default for BuildCtx {
    fn default() -> Self {
        Self::new()
    }
}

/// 作用域栈
///
/// 用于在递归展开字段树时追踪：
/// - 哪些 id 已经被定义（避免重复定义）
/// - 哪些 ref_id 可以被引用
/// - ref_id 对应的 FieldSpec（用于 bits_ref 等功能）
#[derive(Clone)]
pub struct BuildScope {
    /// id 作用域栈（每层是一个 HashSet<String>）
    scopes: Vec<HashSet<String>>,
    
    /// ref_id 作用域栈
    ref_scopes: Vec<HashSet<String>>,
    
    /// ref_id -> FieldSpec 映射栈（用于运行时引用查找）
    ref_specs: Vec<HashMap<String, FieldSpec>>,
}

impl BuildScope {
    /// 创建新的作用域（带一个默认的根作用域）
    pub fn new() -> Self {
        Self {
            scopes: vec![HashSet::new()],
            ref_scopes: vec![HashSet::new()],
            ref_specs: vec![HashMap::new()],
        }
    }

    /// 进入一个新的作用域（push）
    pub fn push_scope(&mut self) {
        self.scopes.push(HashSet::new());
        self.ref_scopes.push(HashSet::new());
        self.ref_specs.push(HashMap::new());
    }

    /// 退出当前作用域（pop）
    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
            self.ref_scopes.pop();
            self.ref_specs.pop();
        }
    }

    /// 在当前作用域中插入一个 id
    pub fn insert(&mut self, id: String) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(id);
        }
    }

    /// 在当前作用域中插入一个 ref_id
    pub fn insert_ref(&mut self, ref_id: String) {
        if let Some(scope) = self.ref_scopes.last_mut() {
            scope.insert(ref_id);
        }
    }

    /// 在当前作用域中插入一个 ref_id 及其对应的 FieldSpec
    pub fn insert_ref_spec(&mut self, ref_id: String, spec: FieldSpec) {
        if let Some(scope) = self.ref_specs.last_mut() {
            scope.insert(ref_id, spec);
        }
    }

    /// 查找 ref_id 对应的 FieldSpec（从当前作用域向上查找）
    pub fn get_ref_spec(&self, ref_id: &str) -> Option<&FieldSpec> {
        for scope in self.ref_specs.iter().rev() {
            if let Some(spec) = scope.get(ref_id) {
                return Some(spec);
            }
        }
        None
    }

    /// 检查某个 id 是否在当前可见的作用域中
    /// （当前仅被本模块单元测试使用，只在测试构建中编译）
    #[cfg(test)]
    pub fn contains(&self, id: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(id))
    }

    /// 检查某个 ref_id 是否在当前可见的作用域中
    pub fn contains_ref_id(&self, id: &str) -> bool {
        self.ref_scopes.iter().rev().any(|scope| scope.contains(id))
    }

    /// 根据 RawField 和 FieldSpec 自动插入相关的 id/ref_id
    pub fn insert_field(&mut self, rf: &RawField, spec: &FieldSpec) {
        if let Some(id) = &rf.id {
            self.insert(id.clone());
        }
        if let Some(ref_id) = &rf.ref_id {
            self.insert(ref_id.clone());
            self.insert_ref(ref_id.clone());
            self.insert_ref_spec(ref_id.clone(), spec.clone());
        }
    }
}

impl Default for BuildScope {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_scope_basic() {
        let mut scope = BuildScope::new();
        
        // 根作用域插入
        scope.insert("id1".to_string());
        assert!(scope.contains("id1"));
        assert!(!scope.contains("id2"));
        
        // 新作用域
        scope.push_scope();
        scope.insert("id2".to_string());
        assert!(scope.contains("id1")); // 可以看到父作用域
        assert!(scope.contains("id2"));
        
        // 退出作用域
        scope.pop_scope();
        assert!(scope.contains("id1"));
        assert!(!scope.contains("id2")); // id2 不可见了
    }

    #[test]
    fn test_build_scope_ref_id() {
        let mut scope = BuildScope::new();
        
        scope.insert_ref("ref1".to_string());
        assert!(scope.contains_ref_id("ref1"));
        assert!(!scope.contains_ref_id("ref2"));
    }
}
