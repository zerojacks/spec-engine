//! 动态字典层加载器
//!
//! 提供 `Layer` 类型和相关工具函数，用于从 YAML 文件、二进制文件或内存加载 DI 定义层。
//!
//! # 核心类型
//!
//! - [`Layer`]: 独立的字典层，包含 DI 定义表
//! - [`DiTable`]: DI 定义的 HashMap
//! - [`DiKey`]: 字典键（protocol, di, region, dir）
//! - [`LayerStats`]: 层统计信息
//!
//! # Region 回退规则
//!
//! 层的查找顺序：
//!
//! 1. 精确匹配：`(protocol, di, region, dir)`
//! 2. 忽略 dir：`(protocol, di, region, None)`
//! 3. 默认 region + dir：`(protocol, di, DEFAULT_REGION, dir)`
//! 4. 默认 region：`(protocol, di, DEFAULT_REGION, None)`
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use spec_engine::Layer;
//!
//! // 从 YAML 加载
//! let layer = Layer::from_yaml_dir("custom".into(), "config/custom")?;
//!
//! // 查找定义
//! if let Some(field) = layer.lookup("csg13", 0x00010000, "南网", None) {
//!     println!("找到: {}", field.name);
//! }
//! ```

use spec_compiler::types::NamedField;
use spec_compiler::Compiler;
use std::collections::HashMap;
use std::path::Path;

/// 默认 region（通用定义）
pub const DEFAULT_REGION: &str = "南网";

/// DI 表的键：(protocol, di_id, region, dir)
///
/// 字典键的四元组结构，用于唯一标识一个 DI 定义。
///
/// # 字段说明
///
/// - `protocol`: 协议名称（如 "csg13", "dlt645-2007"）
/// - `di_id`: DI 标识（32位无符号整数）
/// - `region`: 区域/省份（如 "南网", "广东", "云南"）
/// - `dir`: 方向（`None` 表示通用，`Some("0")` 下行，`Some("1")` 上行）
///
/// # 示例
///
/// ```rust
/// use spec_engine::DiKey;
///
/// // 通用定义（所有方向）
/// let key1: DiKey = ("csg13".into(), 0x00010000, "南网".into(), None);
///
/// // 下行定义
/// let key2: DiKey = ("csg13".into(), 0x00010000, "南网".into(), Some("0".into()));
///
/// // 广东省定义
/// let key3: DiKey = ("csg13".into(), 0x00010000, "广东".into(), None);
/// ```
pub type DiKey = (String, u32, String, Option<String>);

/// DI 表类型
///
/// 存储 DI 定义的 HashMap，键为 [`DiKey`]，值为 [`NamedField`]。
///
/// # 示例
///
/// ```rust,ignore
/// use spec_engine::DiTable;
/// use std::collections::HashMap;
///
/// let mut table = DiTable::new();
/// table.insert(
///     ("csg13".into(), 0x00010000, "南网".into(), None),
///     my_field,
/// );
/// ```
pub type DiTable = HashMap<DiKey, NamedField>;

/// 动态字典层
///
/// 表示一个独立的 DI 字典层，包含名称、表和元数据。
/// 每个层都是完全独立的，可以单独加载、卸载和重新加载。
///
/// # 字段说明
///
/// - `name`: 层名称，用于标识和管理（如 "custom", "guangdong"）
/// - `table`: 该层的 DI 定义表
/// - `loaded_at`: 加载时间戳，用于排序和审计
///
/// # 创建方式
///
/// ## 从 YAML 目录加载
///
/// ```rust,ignore
/// use spec_engine::Layer;
///
/// let layer = Layer::from_yaml_dir("custom".into(), "path/to/yaml")?;
/// ```
///
/// ## 从二进制文件加载
///
/// ```rust,ignore
/// let layer = Layer::from_bin("compiled".into(), "path/to/di_table.bin")?;
/// ```
///
/// ## 从内存表创建
///
/// ```rust,ignore
/// use std::collections::HashMap;
///
/// let table = HashMap::new();
/// // ... 填充 table
/// let layer = Layer::new("memory".into(), table);
/// ```
///
/// # 查找
///
/// 每个层都支持独立查找，带有完整的 region 回退逻辑：
///
/// ```rust,ignore
/// if let Some(field) = layer.lookup("csg13", 0x00010000, "广东", None) {
///     println!("找到: {}", field.name);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Layer {
    /// 层名称（用于标识和调试）
    pub name: String,
    /// 该层的 DI 表
    pub table: DiTable,
    /// 加载时间戳（用于排序和管理）
    pub loaded_at: std::time::SystemTime,
}

impl Layer {
    /// 创建新的层
    ///
    /// 从已有的 DI 表创建一个新层。
    ///
    /// # 参数
    ///
    /// - `name`: 层名称（用于标识和管理）
    /// - `table`: DI 定义表
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// use spec_engine::{Layer, DiTable};
    ///
    /// let table = DiTable::new();
    /// let layer = Layer::new("my_layer".into(), table);
    /// ```
    pub fn new(name: String, table: DiTable) -> Self {
        Self {
            name,
            table,
            loaded_at: std::time::SystemTime::now(),
        }
    }

    /// 从 YAML 目录加载层
    ///
    /// 编译指定目录下的所有 YAML 文件为一个层。
    ///
    /// # 参数
    ///
    /// - `name`: 层名称
    /// - `path`: YAML 文件所在目录
    ///
    /// # 错误
    ///
    /// - 目录不存在
    /// - YAML 文件格式错误
    /// - 编译失败
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// use spec_engine::Layer;
    ///
    /// let layer = Layer::from_yaml_dir("custom".into(), "config/custom")?;
    /// println!("加载了 {} 个条目", layer.table.len());
    /// ```
    pub fn from_yaml_dir<P: AsRef<Path>>(name: String, path: P) -> Result<Self, String> {
        let compiler = Compiler::new();
        let table = compiler.compile_schema_dir(path)?;
        Ok(Self::new(name, table))
    }

    /// 从二进制文件加载层
    ///
    /// 从预编译的二进制文件加载层。这比从 YAML 加载快约 20 倍。
    ///
    /// # 参数
    ///
    /// - `name`: 层名称
    /// - `path`: 二进制文件路径（由 `spec-tools compile` 或 `build.rs` 生成）
    ///
    /// # 错误
    ///
    /// - 文件不存在
    /// - 文件格式错误（不是有效的 bincode 序列化数据）
    /// - 版本不兼容（文件与当前 `FieldSpec` 类型定义不匹配）
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// use spec_engine::Layer;
    ///
    /// // 首先使用 spec-tools 编译
    /// // $ spec-tools compile --input schema --output compiled.bin
    ///
    /// let layer = Layer::from_bin("compiled".into(), "compiled.bin")?;
    /// ```
    pub fn from_bin<P: AsRef<Path>>(name: String, path: P) -> Result<Self, String> {
        let table = Compiler::load_from_bin(path)?;
        Ok(Self::new(name, table))
    }

    /// 查找 DI（带 region 回退）
    ///
    /// 在该层中查找指定的 DI 定义，支持自动回退到默认 region。
    ///
    /// # 查找策略
    ///
    /// 1. 精确匹配：`(protocol, di, region, dir)`
    /// 2. 忽略 dir：`(protocol, di, region, None)`
    /// 3. 默认 region + dir：`(protocol, di, DEFAULT_REGION, dir)`
    /// 4. 默认 region：`(protocol, di, DEFAULT_REGION, None)`
    ///
    /// # 参数
    ///
    /// - `protocol`: 协议名称
    /// - `di`: DI 标识
    /// - `region`: 区域/省份
    /// - `dir`: 方向（`None` 表示通用）
    ///
    /// # 返回值
    ///
    /// - `Some(&NamedField)`: 找到定义
    /// - `None`: 未找到
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// // 查找广东的定义，如果没有则回退到南网
    /// if let Some(field) = layer.lookup("csg13", 0x00010000, "广东", None) {
    ///     println!("字段: {}", field.name);
    /// }
    /// ```
    pub fn lookup(
        &self,
        protocol: &str,
        di: u32,
        region: &str,
        dir: Option<&str>,
    ) -> Option<&NamedField> {
        // 1. 精确匹配：(protocol, di, region, dir)
        if let Some(field) = self.table.get(&(
            protocol.to_string(),
            di,
            region.to_string(),
            dir.map(|s| s.to_string()),
        )) {
            return Some(field);
        }

        // 2. 忽略 dir：(protocol, di, region, None)
        if dir.is_some() {
            if let Some(field) = self.table.get(&(
                protocol.to_string(),
                di,
                region.to_string(),
                None,
            )) {
                return Some(field);
            }
        }

        // 3. 默认 region + dir：(protocol, di, DEFAULT_REGION, dir)
        if region != DEFAULT_REGION {
            if let Some(field) = self.table.get(&(
                protocol.to_string(),
                di,
                DEFAULT_REGION.to_string(),
                dir.map(|s| s.to_string()),
            )) {
                return Some(field);
            }

            // 4. 默认 region：(protocol, di, DEFAULT_REGION, None)
            if let Some(field) = self.table.get(&(
                protocol.to_string(),
                di,
                DEFAULT_REGION.to_string(),
                None,
            )) {
                return Some(field);
            }
        }

        None
    }

    /// 获取该层的统计信息
    ///
    /// 返回该层的详细统计信息，包括条目数、协议分布、区域分布等。
    ///
    /// # 返回值
    ///
    /// [`LayerStats`] 对象，包含：
    /// - 层名称
    /// - 总条目数
    /// - 每个协议的条目数
    /// - 每个区域的条目数
    /// - 加载时间戳
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let stats = layer.stats();
    /// println!("层 {}: {} 条目", stats.name, stats.total_entries);
    /// println!("协议: {:?}", stats.protocols);
    /// println!("区域: {:?}", stats.regions);
    /// ```
    pub fn stats(&self) -> LayerStats {
        let mut protocols = HashMap::new();
        let mut regions = HashMap::new();

        for (protocol, _, region, _) in self.table.keys() {
            *protocols.entry(protocol.clone()).or_insert(0) += 1;
            *regions.entry(region.clone()).or_insert(0) += 1;
        }

        LayerStats {
            name: self.name.clone(),
            total_entries: self.table.len(),
            protocols,
            regions,
            loaded_at: self.loaded_at,
        }
    }
}

/// 层统计信息
///
/// 包含某个字典层的详细统计信息。
///
/// # 字段说明
///
/// - `name`: 层名称
/// - `total_entries`: 总条目数
/// - `protocols`: 每个协议的条目数（如 `{"csg13": 1000, "dlt645-2007": 500}`）
/// - `regions`: 每个区域的条目数（如 `{"南网": 800, "广东": 200}`）
/// - `loaded_at`: 加载时间戳
///
/// # 示例
///
/// ```rust,ignore
/// let stats = layer.stats();
/// println!("层: {}", stats.name);
/// println!("条目数: {}", stats.total_entries);
/// for (protocol, count) in &stats.protocols {
///     println!("  {}: {} 条目", protocol, count);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct LayerStats {
    pub name: String,
    pub total_entries: usize,
    pub protocols: HashMap<String, usize>,
    pub regions: HashMap<String, usize>,
    pub loaded_at: std::time::SystemTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_creation() {
        let table = DiTable::new();
        let layer = Layer::new("test".to_string(), table);
        assert_eq!(layer.name, "test");
        assert_eq!(layer.table.len(), 0);
    }

    #[test]
    fn test_layer_lookup_priority() {
        use spec_compiler::types::{Encoding, FieldLength, FieldSpec};

        let mut table = DiTable::new();

        // 添加测试数据
        let field = NamedField {
            id: Some("00010000".to_string()),
            ref_id: None,
            name: "test_field".to_string(),
            spec: FieldSpec::Fixed {
                encoding: Encoding::Raw,
                length: FieldLength::Fixed(1),
                unit: None,
                enum_map: None,
                format: None,
            },
            format: None,
        };

        table.insert(
            ("test".to_string(), 0x00010000, "南网".to_string(), None),
            field,
        );

        let layer = Layer::new("test".to_string(), table);
        let result = layer.lookup("test", 0x00010000, "南网", None);

        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "test_field");
    }
}
