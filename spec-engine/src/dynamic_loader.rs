//! 动态 YAML 字典加载器
//!
//! 提供运行时动态加载和管理多层 YAML 字典的能力，支持字典扩展、覆盖和热更新。
//!
//! # 核心概念
//!
//! ## 分层架构
//!
//! `DynamicCatalog` 采用分层架构，每一层都是一个独立的 DI 字典：
//!
//! ```text
//! DynamicCatalog
//!   ├─ embedded: 编译时嵌入的静态字典（优先级最低，不可修改）
//!   └─ layers: 运行时加载的动态层（按加载顺序，后加载覆盖先加载）
//!       ├─ Layer 1: 基础字典
//!       ├─ Layer 2: 区域扩展
//!       └─ Layer 3: 客户定制（优先级最高）
//! ```
//!
//! ## 查找优先级
//!
//! 查找某个 DI 时，从最新加载的层开始往回查，找到第一个匹配即返回：
//!
//! 1. Layer N（最后加载）
//! 2. Layer N-1
//! 3. ...
//! 4. Layer 1
//! 5. embedded（编译时字典）
//!
//! 这种优先级设计允许：
//! - 动态层覆盖基础定义
//! - 多次加载同一 DI，最新的生效
//! - 卸载某层后，自动回退到下一优先级
//!
//! ## Region 回退规则
//!
//! 对于每一层，查找顺序为：
//!
//! 1. 精确匹配：`(protocol, di, region, dir)`
//! 2. 忽略 dir：`(protocol, di, region, None)`
//! 3. 默认 region + dir：`(protocol, di, DEFAULT_REGION, dir)`
//! 4. 默认 region：`(protocol, di, DEFAULT_REGION, None)`
//!
//! 这种回退机制确保：
//! - 优先使用区域特定定义
//! - 自动回退到通用定义
//! - 支持方向特定定义（上行/下行）
//!
//! **注意**: Protocol 维度是硬边界，不跨协议查找。
//!
//! # 使用场景
//!
//! ## 1. 测试和调试
//!
//! 加载临时的测试定义，无需修改主字典：
//!
//! ```rust,ignore
//! use spec_engine::create_dynamic_catalog;
//!
//! let mut catalog = create_dynamic_catalog();
//! catalog.load_yaml_dir("test".into(), "test/fixtures/custom_di")?;
//! ```
//!
//! ## 2. 客户定制
//!
//! 为特定客户提供定制化的 DI 定义：
//!
//! ```rust,ignore
//! catalog.load_yaml_dir("customer_a".into(), "config/customer_a/di")?;
//! ```
//!
//! ## 3. 局方差异
//!
//! 不同地区的特殊定义：
//!
//! ```rust,ignore
//! // 广东省特殊定义
//! catalog.load_yaml_dir("guangdong".into(), "config/regions/guangdong")?;
//!
//! // 查找时优先使用广东定义
//! if let Some(field) = catalog.lookup("csg13", 0x00010000, "广东", None) {
//!     println!("使用广东定义: {}", field.name);
//! }
//! ```
//!
//! ## 4. 热更新
//!
//! 运行时更新字典而无需重启：
//!
//! ```rust,ignore
//! // 重新加载配置
//! catalog.reload_yaml_dir("config", "path/to/updated/config")?;
//! ```
//!
//! # 快速开始
//!
//! ## 创建目录
//!
//! ```rust,ignore
//! use spec_engine::create_dynamic_catalog;
//!
//! // 使用嵌入的静态字典作为基础
//! let mut catalog = create_dynamic_catalog();
//! ```
//!
//! ## 加载层
//!
//! ```rust,ignore
//! // 从 YAML 目录加载
//! catalog.load_yaml_dir("custom".into(), "path/to/yaml")?;
//!
//! // 从二进制文件加载（更快）
//! catalog.load_bin("compiled".into(), "path/to/di_table.bin")?;
//!
//! // 从内存表加载
//! use std::collections::HashMap;
//! let table = HashMap::new();
//! catalog.load_table("memory".into(), table);
//! ```
//!
//! ## 查找 DI
//!
//! ```rust,ignore
//! if let Some(field) = catalog.lookup("csg13", 0x00010000, "南网", None) {
//!     println!("字段名: {}", field.name);
//!     println!("规格: {:?}", field.spec);
//! }
//! ```
//!
//! ## 层管理
//!
//! ```rust,ignore
//! // 列出所有层
//! let layers = catalog.list_layers();
//! println!("已加载的层: {:?}", layers);
//!
//! // 获取层数量
//! println!("动态层数: {}", catalog.layer_count());
//!
//! // 卸载某个层
//! catalog.unload_layer("custom")?;
//!
//! // 清空所有动态层
//! catalog.clear_layers();
//! ```
//!
//! ## 统计信息
//!
//! ```rust,ignore
//! // 获取详细统计
//! let stats = catalog.stats();
//! println!("嵌入字典: {} 条目", stats.embedded_entries);
//! println!("动态层数: {}", stats.layers.len());
//! println!("总条目数: {}", stats.total_entries());
//!
//! // 打印格式化的统计信息
//! stats.print();
//! ```
//!
//! # 性能考虑
//!
//! ## 查找性能
//!
//! - 每层的查找为 O(1)（HashMap）
//! - 最坏情况需要遍历所有层：O(N * 4)，其中 N 是层数，4 是回退步骤数
//! - 实践中，大多数查找在前几层就能命中
//!
//! ## 内存占用
//!
//! - 每个 `NamedField` 约 200-500 字节（取决于字段复杂度）
//! - 35,000 条目约需 7-17 MB 内存
//! - 动态层完全独立，不共享数据
//!
//! ## 加载性能
//!
//! - YAML 加载：约 1-2 秒（35,000 条目）
//! - 二进制加载：约 50-100 ms（35,000 条目）
//! - 建议生产环境使用二进制格式
//!
//! # 线程安全
//!
//! `DynamicCatalog` 不是线程安全的。如果需要在多线程环境中使用，请使用：
//!
//! ```rust,ignore
//! use std::sync::{Arc, RwLock};
//!
//! let catalog = Arc::new(RwLock::new(create_dynamic_catalog()));
//!
//! // 读取
//! let catalog_read = catalog.read().unwrap();
//! let result = catalog_read.lookup("csg13", 0x00010000, "南网", None);
//!
//! // 写入
//! let mut catalog_write = catalog.write().unwrap();
//! catalog_write.load_yaml_dir("new_layer".into(), "path")?;
//! ```
//!
//! # 示例
//!
//! 完整示例请参考 `examples/dynamic_catalog_demo.rs`。

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
/// use spec_engine::dynamic_loader::DiKey;
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
/// use spec_engine::dynamic_loader::DiTable;
/// use spec_engine::NamedField;
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
/// use spec_engine::dynamic_loader::Layer;
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
    /// use spec_engine::dynamic_loader::{Layer, DiTable};
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
    /// use spec_engine::dynamic_loader::Layer;
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
    /// use spec_engine::dynamic_loader::Layer;
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

/// 动态字典目录
///
/// 管理多层字典的核心类型，支持运行时动态加载、卸载和查找。
///
/// # 架构
///
/// `DynamicCatalog` 由两部分组成：
///
/// 1. **embedded**: 编译时嵌入的静态字典（优先级最低，不可修改）
/// 2. **layers**: 运行时加载的动态层（可加载、卸载、重新加载）
///
/// # 查找优先级
///
/// 查找时从最新加载的层开始：
///
/// ```text
/// lookup("csg13", 0x00010000, "南网", None)
///   ↓
/// Layer 3（最新）→ 找到? 返回 : 继续
///   ↓
/// Layer 2 → 找到? 返回 : 继续
///   ↓
/// Layer 1 → 找到? 返回 : 继续
///   ↓
/// embedded → 找到? 返回 : None
/// ```
///
/// # 线程安全
///
/// `DynamicCatalog` 不是线程安全的。多线程环境请使用 `Arc<RwLock<DynamicCatalog>>`。
///
/// # 示例
///
/// ```rust,ignore
/// use spec_engine::create_dynamic_catalog;
///
/// let mut catalog = create_dynamic_catalog();
///
/// // 加载自定义层
/// catalog.load_yaml_dir("custom".into(), "config/custom")?;
///
/// // 查找（优先从 custom 层查找）
/// if let Some(field) = catalog.lookup("csg13", 0x00010000, "南网", None) {
///     println!("找到: {}", field.name);
/// }
///
/// // 卸载层
/// catalog.unload_layer("custom")?;
/// ```
pub struct DynamicCatalog {
    /// 编译时嵌入的静态字典（优先级最低）
    embedded: DiTable,
    /// 运行时加载的动态层（按加载顺序，索引越大优先级越高）
    layers: Vec<Layer>,
}

impl DynamicCatalog {
    /// 创建新的动态目录，使用编译时嵌入的字典作为基础层
    ///
    /// # 参数
    ///
    /// - `embedded`: 基础字典（通常来自 [`get_spec_catalog()`](crate::get_spec_catalog)）
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// use spec_engine::{DynamicCatalog, get_spec_catalog};
    ///
    /// let catalog = DynamicCatalog::new(get_spec_catalog().clone());
    /// ```
    ///
    /// 或使用便捷函数：
    ///
    /// ```rust,ignore
    /// use spec_engine::create_dynamic_catalog;
    ///
    /// let catalog = create_dynamic_catalog();
    /// ```
    pub fn new(embedded: DiTable) -> Self {
        Self {
            embedded,
            layers: Vec::new(),
        }
    }

    /// 加载 YAML 目录作为新层
    ///
    /// 编译指定目录下的所有 YAML 文件并作为新层加载。
    ///
    /// # 参数
    ///
    /// - `name`: 层名称（用于后续管理）
    /// - `path`: YAML 文件所在目录
    ///
    /// # 错误
    ///
    /// - 目录不存在或无法访问
    /// - YAML 文件格式错误
    /// - 编译失败
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// catalog.load_yaml_dir("custom".into(), "config/custom")?;
    /// ```
    pub fn load_yaml_dir<P: AsRef<Path>>(&mut self, name: String, path: P) -> Result<(), String> {
        let layer = Layer::from_yaml_dir(name, path)?;
        self.layers.push(layer);
        Ok(())
    }

    /// 加载二进制文件作为新层
    ///
    /// 从预编译的二进制文件加载层，比 YAML 加载快约 20 倍。
    ///
    /// # 参数
    ///
    /// - `name`: 层名称
    /// - `path`: 二进制文件路径
    ///
    /// # 错误
    ///
    /// - 文件不存在或无法访问
    /// - 文件格式错误
    /// - 版本不兼容
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// catalog.load_bin("compiled".into(), "compiled.bin")?;
    /// ```
    pub fn load_bin<P: AsRef<Path>>(&mut self, name: String, path: P) -> Result<(), String> {
        let layer = Layer::from_bin(name, path)?;
        self.layers.push(layer);
        Ok(())
    }

    /// 从内存表加载层
    ///
    /// 直接从已有的 DI 表创建新层。适用于：
    /// - 测试场景
    /// - 运行时动态生成的定义
    /// - 从其他数据源转换而来的定义
    ///
    /// # 参数
    ///
    /// - `name`: 层名称
    /// - `table`: DI 定义表
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// use spec_engine::dynamic_loader::DiTable;
    ///
    /// let mut table = DiTable::new();
    /// // ... 填充 table
    /// catalog.load_table("memory".into(), table);
    /// ```
    pub fn load_table(&mut self, name: String, table: DiTable) {
        self.layers.push(Layer::new(name, table));
    }

    /// 卸载指定名称的层
    ///
    /// 移除指定的动态层，返回被移除的层。卸载后，查找会自动回退到下一优先级。
    ///
    /// # 参数
    ///
    /// - `name`: 要卸载的层名称
    ///
    /// # 返回值
    ///
    /// - `Ok(Layer)`: 成功卸载，返回被移除的层
    /// - `Err(String)`: 层不存在
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// match catalog.unload_layer("custom") {
    ///     Ok(layer) => println!("卸载了 {} (共 {} 条目)", layer.name, layer.table.len()),
    ///     Err(e) => eprintln!("卸载失败: {}", e),
    /// }
    /// ```
    pub fn unload_layer(&mut self, name: &str) -> Result<Layer, String> {
        if let Some(pos) = self.layers.iter().position(|l| l.name == name) {
            Ok(self.layers.remove(pos))
        } else {
            Err(format!("层 '{}' 不存在", name))
        }
    }

    /// 重新加载指定层（先卸载再加载）
    ///
    /// 从 YAML 目录重新加载指定的层。如果加载失败，原层保持不变。
    /// 用于热更新场景。
    ///
    /// # 参数
    ///
    /// - `name`: 要重新加载的层名称
    /// - `path`: YAML 文件所在目录
    ///
    /// # 错误
    ///
    /// - 层不存在
    /// - YAML 加载或编译失败（原层保持不变）
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// // 更新配置文件后重新加载
    /// catalog.reload_yaml_dir("config", "path/to/config")?;
    /// ```
    pub fn reload_yaml_dir<P: AsRef<Path>>(
        &mut self,
        name: &str,
        path: P,
    ) -> Result<(), String> {
        // 记录原来的位置
        let pos = self
            .layers
            .iter()
            .position(|l| l.name == name)
            .ok_or_else(|| format!("层 '{}' 不存在", name))?;

        // 先加载新层（如果失败，旧层保持不变）
        let new_layer = Layer::from_yaml_dir(name.to_string(), path)?;

        // 替换旧层
        self.layers[pos] = new_layer;

        Ok(())
    }

    /// 查找 DI（从最新层开始查找，直到找到或查完所有层）
    ///
    /// 这是 `DynamicCatalog` 的核心查找方法，实现了完整的层级优先级和 region 回退逻辑。
    ///
    /// # 查找流程
    ///
    /// 1. 从最新加载的动态层开始，逐层往回查找
    /// 2. 每层内部进行 region 回退（精确 → 忽略 dir → 默认 region）
    /// 3. 如果所有动态层都没找到，最后查找 embedded 基础层
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
    /// - `None`: 所有层都没有找到
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// // 查找广东的定义
    /// if let Some(field) = catalog.lookup("csg13", 0x00010000, "广东", None) {
    ///     println!("字段: {}", field.name);
    /// }
    ///
    /// // 查找下行方向的定义
    /// if let Some(field) = catalog.lookup("csg13", 0x00010000, "南网", Some("0")) {
    ///     println!("下行字段: {}", field.name);
    /// }
    /// ```
    pub fn lookup(
        &self,
        protocol: &str,
        di: u32,
        region: &str,
        dir: Option<&str>,
    ) -> Option<&NamedField> {
        // 从最新的层开始查找
        for layer in self.layers.iter().rev() {
            if let Some(field) = layer.lookup(protocol, di, region, dir) {
                return Some(field);
            }
        }

        // 最后查找嵌入的基础层
        self.lookup_embedded(protocol, di, region, dir)
    }

    /// 在嵌入的静态字典中查找
    fn lookup_embedded(
        &self,
        protocol: &str,
        di: u32,
        region: &str,
        dir: Option<&str>,
    ) -> Option<&NamedField> {
        // 1. 精确匹配：(protocol, di, region, dir)
        if let Some(field) = self.embedded.get(&(
            protocol.to_string(),
            di,
            region.to_string(),
            dir.map(|s| s.to_string()),
        )) {
            return Some(field);
        }

        // 2. 忽略 dir：(protocol, di, region, None)
        if dir.is_some() {
            if let Some(field) = self.embedded.get(&(
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
            if let Some(field) = self.embedded.get(&(
                protocol.to_string(),
                di,
                DEFAULT_REGION.to_string(),
                dir.map(|s| s.to_string()),
            )) {
                return Some(field);
            }

            // 4. 默认 region：(protocol, di, DEFAULT_REGION, None)
            if let Some(field) = self.embedded.get(&(
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

    /// 列出所有层的名称
    ///
    /// 返回所有已加载动态层的名称列表，按加载顺序排列。
    ///
    /// # 返回值
    ///
    /// 层名称的切片引用列表，不包括 embedded 基础层。
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let layers = catalog.list_layers();
    /// println!("已加载的层: {:?}", layers);
    /// // 输出: ["layer1", "layer2", "custom"]
    /// ```
    pub fn list_layers(&self) -> Vec<&str> {
        self.layers.iter().map(|l| l.name.as_str()).collect()
    }

    /// 获取所有层的统计信息
    ///
    /// 返回完整的目录统计信息，包括基础层和所有动态层。
    ///
    /// # 返回值
    ///
    /// [`CatalogStats`] 对象，包含：
    /// - 嵌入字典的条目数
    /// - 每个动态层的详细统计
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let stats = catalog.stats();
    /// println!("嵌入字典: {} 条目", stats.embedded_entries);
    /// println!("动态层数: {}", stats.layers.len());
    /// println!("总条目数: {}", stats.total_entries());
    ///
    /// // 打印格式化的统计信息
    /// stats.print();
    /// ```
    pub fn stats(&self) -> CatalogStats {
        let mut layer_stats = Vec::new();
        for layer in &self.layers {
            layer_stats.push(layer.stats());
        }

        CatalogStats {
            embedded_entries: self.embedded.len(),
            layers: layer_stats,
        }
    }

    /// 清空所有动态层（保留嵌入的基础层）
    ///
    /// 移除所有动态层，使目录恢复到初始状态（只包含 embedded 基础层）。
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// catalog.clear_layers();
    /// assert_eq!(catalog.layer_count(), 0);
    /// ```
    pub fn clear_layers(&mut self) {
        self.layers.clear();
    }

    /// 获取动态层数量
    ///
    /// 返回当前加载的动态层数量，不包括 embedded 基础层。
    ///
    /// # 返回值
    ///
    /// 动态层的数量。
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// println!("当前有 {} 个动态层", catalog.layer_count());
    /// ```
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// 迭代所有条目（包括嵌入字典和所有动态层）
    ///
    /// 返回一个迭代器，遍历所有层的所有条目。
    /// 注意：可能包含重复的 DI（不同层定义了相同的 DI）。
    ///
    /// # 返回值
    ///
    /// 迭代器，每项为 `(&DiKey, &NamedField)`
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// for ((protocol, di, region, dir), field) in catalog.iter_all() {
    ///     println!("{}: {} - {}", protocol, di, field.name);
    /// }
    /// ```
    pub fn iter_all(&self) -> impl Iterator<Item = (&DiKey, &NamedField)> {
        self.embedded
            .iter()
            .chain(self.layers.iter().flat_map(|layer| layer.table.iter()))
    }

    /// 列出所有协议
    ///
    /// 返回所有已定义的协议名称列表（去重）。
    ///
    /// # 返回值
    ///
    /// 协议名称的向量，已排序。
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let protocols = catalog.list_protocols();
    /// println!("支持的协议: {:?}", protocols);
    /// // 输出: ["csg13", "csg16", "dlt645-2007"]
    /// ```
    pub fn list_protocols(&self) -> Vec<String> {
        use std::collections::HashSet;
        let mut protocols = HashSet::new();
        for ((protocol, _, _, _), _) in self.iter_all() {
            protocols.insert(protocol.clone());
        }
        let mut result: Vec<String> = protocols.into_iter().collect();
        result.sort();
        result
    }

    /// 列出指定协议的所有 DI
    ///
    /// 返回指定协议的所有 DI 标识列表（去重）。
    ///
    /// # 参数
    ///
    /// - `protocol`: 协议名称
    ///
    /// # 返回值
    ///
    /// DI 标识的向量，已排序。
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let dis = catalog.list_dis_for_protocol("csg13");
    /// println!("csg13 协议有 {} 个 DI", dis.len());
    /// ```
    pub fn list_dis_for_protocol(&self, protocol: &str) -> Vec<u32> {
        use std::collections::HashSet;
        let mut dis = HashSet::new();
        for ((p, di, _, _), _) in self.iter_all() {
            if p == protocol {
                dis.insert(*di);
            }
        }
        let mut result: Vec<u32> = dis.into_iter().collect();
        result.sort();
        result
    }

    /// 列出指定协议的所有区域
    ///
    /// 返回指定协议支持的所有区域列表（去重）。
    ///
    /// # 参数
    ///
    /// - `protocol`: 协议名称
    ///
    /// # 返回值
    ///
    /// 区域名称的向量，已排序。
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let regions = catalog.list_regions_for_protocol("csg13");
    /// println!("csg13 支持的区域: {:?}", regions);
    /// ```
    pub fn list_regions_for_protocol(&self, protocol: &str) -> Vec<String> {
        use std::collections::HashSet;
        let mut regions = HashSet::new();
        for ((p, _, region, _), _) in self.iter_all() {
            if p == protocol {
                regions.insert(region.clone());
            }
        }
        let mut result: Vec<String> = regions.into_iter().collect();
        result.sort();
        result
    }
}

/// 目录统计信息
///
/// 包含整个 `DynamicCatalog` 的统计信息，用于监控和调试。
///
/// # 字段说明
///
/// - `embedded_entries`: 嵌入字典的条目数
/// - `layers`: 每个动态层的统计信息列表
///
/// # 计算总数
///
/// 使用 [`total_entries()`](Self::total_entries) 方法计算所有层的总条目数：
///
/// ```rust,ignore
/// let stats = catalog.stats();
/// let total = stats.total_entries();
/// ```
///
/// **注意**: 总数可能包含重复的 DI（不同层定义了相同的 DI），这是预期行为。
/// 实际有效条目数取决于层级优先级。
///
/// # 格式化输出
///
/// 使用 [`print()`](Self::print) 方法打印格式化的统计信息：
///
/// ```rust,ignore
/// stats.print();
/// ```
///
/// 输出示例：
///
/// ```text
/// ╔════════════════════════════════════════════════════════╗
/// ║           DynamicCatalog 统计信息                      ║
/// ╚════════════════════════════════════════════════════════╝
///
/// 嵌入字典：35310 条目
///
/// 动态层：2 层
///
///   层 1 - custom
///     条目数：150
///     协议数：1
///     区域数：2
///
///   层 2 - guangdong
///     条目数：50
///     协议数：1
///     区域数：1
///
/// 总计：35510 条目
/// ```
#[derive(Debug)]
pub struct CatalogStats {
    pub embedded_entries: usize,
    pub layers: Vec<LayerStats>,
}

impl CatalogStats {
    /// 计算总条目数（包括嵌入字典和所有动态层）
    ///
    /// **注意**: 这是所有层的条目数之和，可能包含重复的 DI。
    /// 实际有效条目数取决于层级优先级（高优先级层覆盖低优先级层）。
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let stats = catalog.stats();
    /// println!("总条目数: {}", stats.total_entries());
    /// ```
    pub fn total_entries(&self) -> usize {
        self.embedded_entries + self.layers.iter().map(|l| l.total_entries).sum::<usize>()
    }

    /// 打印格式化的统计信息
    ///
    /// 以表格形式打印整个目录的统计信息，包括：
    /// - 嵌入字典条目数
    /// - 每个动态层的详细信息
    /// - 总条目数
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let stats = catalog.stats();
    /// stats.print();
    /// ```
    pub fn print(&self) {
        println!("╔════════════════════════════════════════════════════════╗");
        println!("║           DynamicCatalog 统计信息                      ║");
        println!("╚════════════════════════════════════════════════════════╝");
        println!();
        println!("嵌入字典：{} 条目", self.embedded_entries);
        println!();
        if self.layers.is_empty() {
            println!("动态层：无");
        } else {
            println!("动态层：{} 层", self.layers.len());
            for (i, layer) in self.layers.iter().enumerate() {
                println!();
                println!("  层 {} - {}", i + 1, layer.name);
                println!("    条目数：{}", layer.total_entries);
                println!("    协议数：{}", layer.protocols.len());
                println!("    区域数：{}", layer.regions.len());
            }
        }
        println!();
        println!("总计：{} 条目", self.total_entries());
    }
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
    fn test_catalog_creation() {
        let embedded = DiTable::new();
        let catalog = DynamicCatalog::new(embedded);
        assert_eq!(catalog.layer_count(), 0);
    }

    #[test]
    fn test_layer_management() {
        let embedded = DiTable::new();
        let mut catalog = DynamicCatalog::new(embedded);

        // 加载层
        catalog.load_table("layer1".to_string(), DiTable::new());
        assert_eq!(catalog.layer_count(), 1);

        catalog.load_table("layer2".to_string(), DiTable::new());
        assert_eq!(catalog.layer_count(), 2);

        // 卸载层
        let removed = catalog.unload_layer("layer1");
        assert!(removed.is_ok());
        assert_eq!(catalog.layer_count(), 1);

        // 清空所有层
        catalog.clear_layers();
        assert_eq!(catalog.layer_count(), 0);
    }

    #[test]
    fn test_lookup_priority() {
        use spec_compiler::types::{FieldSpec, Encoding, FieldLength};

        let mut embedded = DiTable::new();
        let mut layer1 = DiTable::new();
        let mut layer2 = DiTable::new();

        // 在不同层定义同一个 DI
        let field_embedded = NamedField {
            id: Some("00010000".to_string()),
            ref_id: None,
            name: "embedded".to_string(),
            spec: FieldSpec::Fixed {
                encoding: Encoding::Raw,
                length: FieldLength::Fixed(1),
                unit: None,
                enum_map: None,
                format: None,
            },
            format: None,
        };

        let field_layer1 = NamedField {
            name: "layer1".to_string(),
            ..field_embedded.clone()
        };

        let field_layer2 = NamedField {
            name: "layer2".to_string(),
            ..field_embedded.clone()
        };

        embedded.insert(
            ("csg13".to_string(), 0x00010000, "南网".to_string(), None),
            field_embedded,
        );
        layer1.insert(
            ("csg13".to_string(), 0x00010000, "南网".to_string(), None),
            field_layer1,
        );
        layer2.insert(
            ("csg13".to_string(), 0x00010000, "南网".to_string(), None),
            field_layer2,
        );

        let mut catalog = DynamicCatalog::new(embedded);
        catalog.load_table("layer1".to_string(), layer1);
        catalog.load_table("layer2".to_string(), layer2);

        // 应该返回最后加载的 layer2
        let result = catalog.lookup("csg13", 0x00010000, "南网", None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "layer2");
    }
}
