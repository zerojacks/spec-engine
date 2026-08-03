//! # spec-engine
//!
//! 计量自动化终端上行通信规约 DI 字典解析引擎
//!
//! `spec-engine` 是一个强大的电力通信协议解析引擎，支持编译期静态字典和运行时动态加载。
//! 它专为处理中国电力行业的各种通信协议（如 DL/T645、Q/CSG1209022 等）而设计。
//!
//! ## 核心特性
//!
//! ### 编译期静态字典
//!
//! - 从 YAML 文件编译生成嵌入式二进制字典
//! - 零运行时开销的字典查找
//! - 支持 35,000+ DI 条目的快速访问
//!
//! ### 运行时动态加载
//!
//! - 支持动态加载 YAML 字典层
//! - 层级优先级：后加载的层覆盖先加载的层
//! - 自动回退到基础字典
//!
//! ### 多编码支持
//!
//! - **BCD**: Binary-Coded Decimal，电力行业常用编码
//! - **Binary**: 二进制整数（支持大小端、有符号/无符号）
//! - **ASCII**: ASCII 字符串
//! - **Hex**: 十六进制字符串
//! - **Time**: 时间格式（多种编码方式）
//! - **Raw**: 原始字节
//!
//! ### 高级解析特性
//!
//! - **位域解析**: 将字节拆分为位字段
//! - **条件分支**: 根据前面字段的值选择不同的解析规则
//! - **重复结构**: 支持计数重复和位图重复
//! - **动态长度**: 字段长度可引用前面字段的值
//! - **外部协议**: 支持嵌套解析其他协议的报文
//! - **区域覆盖**: 按省份/局方自定义 DI 定义，自动回退到通用定义
//!
//! ## 快速开始
//!
//! ### 安装
//!
//! ```toml
//! [dependencies]
//! spec-engine = "0.2"
//! ```
//!
//! ### 基础解析示例
//!
//! 解析一个 DI 数据项：
//!
//! ```rust,ignore
//! use spec_engine::{parse_di, DEFAULT_REGION};
//!
//! // 报文数据（BCD 编码的电能值）
//! let data = vec![0x34, 0x12, 0x00, 0x00];  // 表示 1234.00 kWh
//!
//! // 按 DI 码解析
//! let (value, consumed) = parse_di(
//!     "csg13",           // 协议名称
//!     0x00010000,        // DI 标识
//!     DEFAULT_REGION,    // 区域（"南网"）
//!     None,              // 方向（None 表示通用）
//!     &data,             // 报文数据
//! )?;
//!
//! println!("解析结果: {:?}", value);
//! println!("消耗字节: {}", consumed);
//! ```
//!
//! ### 简化 API
//!
//! 使用简化 API（自动使用默认 region）：
//!
//! ```rust,ignore
//! use spec_engine::parse;
//!
//! let (value, consumed) = parse(0x00010000, "csg13", &data)?;
//! ```
//!
//! ### 使用 Prelude
//!
//! 一次性导入所有常用 API：
//!
//! ```rust,ignore
//! use spec_engine::prelude::*;
//!
//! let (value, consumed) = parse_di("csg13", 0x00010000, DEFAULT_REGION, None, &data)?;
//! let catalog = get_spec_catalog();
//! let mut dynamic = create_dynamic_catalog();
//! ```
//!
//! ## 动态字典加载
//!
//! ### 创建动态字典管理器
//!
//! ```rust,ignore
//! use spec_engine::create_dynamic_catalog;
//!
//! // 使用嵌入的静态字典作为基础
//! let mut catalog = create_dynamic_catalog();
//! ```
//!
//! ### 加载自定义层
//!
//! ```rust,ignore
//! // 从 YAML 目录加载
//! catalog.load_yaml_dir("custom_layer".into(), "path/to/yaml")?;
//!
//! // 或加载单个 YAML 文件
//! catalog.load_yaml_file("override".into(), "path/to/override.yaml")?;
//! ```
//!
//! ### 查找 DI 定义
//!
//! ```rust,ignore
//! if let Some(field) = catalog.lookup("csg13", 0x00010000, "南网", None) {
//!     println!("字段名: {}", field.name);
//!     println!("规格: {:?}", field.spec);
//! }
//! ```
//!
//! ### 层管理
//!
//! ```rust,ignore
//! // 列出所有层
//! let layers = catalog.list_layers();
//! println!("已加载的层: {:?}", layers);
//!
//! // 卸载某个层
//! catalog.unload_layer("custom_layer")?;
//!
//! // 清空所有动态层（保留基础字典）
//! catalog.clear_layers();
//!
//! // 查看统计信息
//! catalog.stats().print();
//! ```
//!
//! ## 静态字典访问
//!
//! ### 获取整个字典
//!
//! ```rust,ignore
//! use spec_engine::get_spec_catalog;
//!
//! let catalog = get_spec_catalog();
//! println!("字典包含 {} 个条目", catalog.len());
//!
//! // 遍历所有条目
//! for ((protocol, di, region, dir), field) in catalog.iter() {
//!     println!("{} 0x{:08X} [{}]: {}", protocol, di, region, field.name);
//! }
//! ```
//!
//! ### 查找单个 DI 定义
//!
//! ```rust,ignore
//! use spec_engine::lookup_di_spec;
//!
//! if let Some(spec) = lookup_di_spec(0x00010000, "csg13", "南网") {
//!     println!("字段名: {}", spec.name);
//!     println!("规格: {:?}", spec.spec);
//! }
//! ```
//!
//! ## 错误处理
//!
//! 所有解析错误都通过 [`DictError`] 类型返回：
//!
//! ```rust,ignore
//! use spec_engine::{parse, DictError};
//!
//! match parse(0x00010000, "csg13", &data) {
//!     Ok((value, consumed)) => {
//!         println!("成功: {:?}, 消耗 {} 字节", value, consumed);
//!     }
//!     Err(DictError::UnknownDi { protocol, di, region, dir }) => {
//!         eprintln!("未知 DI: {} 0x{:08X} [{}] {:?}", protocol, di, region, dir);
//!     }
//!     Err(DictError::UnexpectedEof { needed, available }) => {
//!         eprintln!("数据不足: 需要 {} 字节, 实际 {}", needed, available);
//!     }
//!     Err(e) => eprintln!("其他错误: {}", e),
//! }
//! ```
//!
//! ## 高级用法
//!
//! ### 直接使用 FieldSpec 解析
//!
//! ```rust,ignore
//! use spec_engine::{parse_field, FieldSpec, Encoding, FieldLength, Endian};
//! use std::collections::HashMap;
//!
//! let spec = FieldSpec::Fixed {
//!     encoding: Encoding::Bin {
//!         endian: Endian::Little,
//!         signed: false,
//!     },
//!     length: FieldLength::Fixed(4),
//!     unit: Some("W".to_string()),
//!     enum_map: None,
//!     format: None,
//! };
//!
//! let mut ctx = Context::new();
//! let (value, consumed) = parse_field(&spec, &data, &mut ctx)?;
//! ```
//!
//! ### 自定义处理器
//!
//! 详见 [`registry`] 模块文档。
//!
//! ## 架构说明
//!
//! ### 编译流程
//!
//! 1. **YAML → AST**: `spec-compiler` 解析 YAML 文件为 AST
//! 2. **AST → FieldSpec**: 编译器展开模板、重复结构，生成 `FieldSpec` 树
//! 3. **FieldSpec → Binary**: `build.rs` 序列化为二进制文件
//! 4. **Binary → Embedded**: 二进制文件嵌入到最终可执行文件中
//!
//! ### 运行流程
//!
//! 1. **查找**: 根据 `(protocol, di, region, dir)` 查找 `NamedField`
//! 2. **解析**: 递归遍历 `FieldSpec` 树，按规则解析字节流
//! 3. **输出**: 生成 `Value` 树，包含解析结果和元数据
//!
//! ### 字典键结构
//!
//! ```text
//! (protocol: String, di: u32, region: String, dir: Option<String>)
//! ```
//!
//! - `protocol`: 协议名称（如 "csg13", "dlt645-2007"）
//! - `di`: DI 标识（32位无符号整数）
//! - `region`: 区域/省份（如 "南网", "广东", "云南"）
//! - `dir`: 方向（`None` 表示通用，`Some("0")` 表示下行，`Some("1")` 表示上行）
//!
//! ## 性能特性
//!
//! - 字典查找：O(1) 时间复杂度（基于 HashMap）
//! - 零拷贝解析：直接引用输入缓冲区
//! - 最小内存分配：重用上下文对象
//! - 编译期优化：大部分结构在编译期展开
//!
//! ## 相关 Crate
//!
//! - [`spec-compiler`]: 字典编译器，将 YAML 编译为 `FieldSpec`
//! - `spec-tools`: 命令行工具，用于编译和查询字典
//!
//! ## 示例
//!
//! 完整示例请参考 `examples/` 目录：
//!
//! - `simple_usage.rs`: 基础解析示例
//! - `dynamic_catalog_demo.rs`: 动态字典加载示例
//! - `dump_di_table.rs`: 字典导出工具
//! - `list_keys.rs`: 列出所有 DI 条目
//!
//! [`spec-compiler`]: ../spec_compiler/index.html

mod context;
mod decode;
mod error;
mod parser;
mod registry;
mod repeat;

// 动态加载模块（公开）
pub mod dynamic_loader;

// === 重新导出：类型系统 ===

pub use spec_compiler::{
    // 核心类型
    FieldSpec,
    NamedField,
    Encoding,
    FieldLength,
    
    // 辅助类型
    TimeEncoding,
    Endian,
    FormatSpec,
    FormatType,
    FormatOrder,
    BitSpec,
    ExternalLength,
    
    // AST（高级用户）
    ast,
};

// === 重新导出：核心 API ===

/// 解析 API
pub use parser::{parse_di, parse_field, DEFAULT_REGION};

// Value 类型从 spec_compiler 导入
pub use spec_compiler::types::Value;

/// 动态加载 API
pub use dynamic_loader::{
    DynamicCatalog,
    Layer,
    LayerStats,
    CatalogStats,
    DiTable,
    DiKey,
};

/// 错误处理
pub use error::DictError;

/// 上下文（高级）
pub use context::Context;

/// 解码函数（高级）
pub use decode::{
    decode_ascii,
    decode_bcd_u64,
    decode_bin_u64,
    decode_hex,
    decode_signed_bcd,
    decode_signed_bin,
    decode_time,
};

/// 注册器（高级）
pub use registry::{
    get_custom_handler,
    get_external_parser,
    init_registries,
    CustomHandler,
    ExternalParser,
};

// === 静态字典访问 ===

use std::collections::HashMap;
use std::sync::OnceLock;

static SPEC_CATALOG_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/di_table.bin"));
static SPEC_CATALOG: OnceLock<HashMap<(String, u32, String, Option<String>), NamedField>> =
    OnceLock::new();

/// 获取编译时嵌入的静态字典
///
/// 返回包含所有 DI 定义的全局静态字典。这个字典在编译期从 YAML 文件生成，
/// 嵌入到最终的可执行文件中，运行时首次访问时反序列化。
///
/// # 字典结构
///
/// 返回的 `HashMap` 键为 `(protocol, di, region, dir)` 四元组：
///
/// - `protocol`: 协议名称（如 "csg13", "dlt645-2007"）
/// - `di`: DI 标识（32位无符号整数）
/// - `region`: 区域/省份（如 "南网", "广东"）
/// - `dir`: 方向（`None` 表示通用，`Some("0")` 下行，`Some("1")` 上行）
///
/// # 性能
///
/// - 首次调用时反序列化二进制数据（约10MB）
/// - 后续调用直接返回缓存的引用（零开销）
/// - 字典查找为 O(1) 时间复杂度
///
/// # Panic
///
/// 如果二进制数据损坏或与当前 `FieldSpec` 类型定义不匹配，会 panic。
/// 这通常发生在：
/// - 修改了 `types.rs` 但没有重新编译
/// - 二进制文件被手动修改
///
/// 解决方法：执行 `cargo clean` 后重新编译。
///
/// # 示例
///
/// ```rust,ignore
/// use spec_engine::get_spec_catalog;
///
/// let catalog = get_spec_catalog();
/// println!("字典包含 {} 个条目", catalog.len());
///
/// // 查找特定 DI
/// let key = ("csg13".to_string(), 0x00010000, "南网".to_string(), None);
/// if let Some(field) = catalog.get(&key) {
///     println!("字段名: {}", field.name);
/// }
///
/// // 遍历所有条目
/// for ((protocol, di, region, dir), field) in catalog.iter() {
///     println!("{} 0x{:08X} [{}]: {}", protocol, di, region, field.name);
/// }
/// ```
pub fn get_spec_catalog() -> &'static HashMap<(String, u32, String, Option<String>), NamedField> {
    SPEC_CATALOG.get_or_init(|| {
        bincode::deserialize(SPEC_CATALOG_BYTES).unwrap_or_else(|e| {
            panic!(
                "规范目录二进制反序列化失败: {}\n\
                 通常是 di_table.bin 与当前 FieldSpec 类型定义不匹配（比如改过\n\
                 src/types.rs 但没有重新构建）导致的，试试 `cargo clean` 后重新构建。",
                e
            )
        })
    })
}

/// 从文件初始化字典（覆盖编译时嵌入的字典）
///
/// 从外部二进制文件加载字典，替代编译时嵌入的字典。这对于以下场景很有用：
///
/// - 运行时更新字典而无需重新编译
/// - 使用不同的字典文件进行测试
/// - 减小可执行文件体积（使用外部字典文件）
///
/// # 参数
///
/// - `path`: 二进制字典文件路径（通常由 `build.rs` 或 `spec-tools compile` 生成）
///
/// # 注意
///
/// - 只有首次调用有效，后续调用会被忽略（字典已初始化）
/// - 文件必须与当前 `FieldSpec` 类型定义兼容
/// - 加载后无法回退到嵌入的字典
///
/// # 错误
///
/// - `Err(std::io::Error)`: 文件读取失败
///
/// # Panic
///
/// 如果文件内容无法反序列化为有效的字典结构，会 panic。
///
/// # 示例
///
/// ```rust,ignore
/// use spec_engine::init_spec_catalog_from_file;
///
/// // 从外部文件加载字典
/// init_spec_catalog_from_file("/path/to/di_table.bin")?;
///
/// // 后续使用 get_spec_catalog() 访问
/// let catalog = get_spec_catalog();
/// println!("加载了 {} 个条目", catalog.len());
/// ```
pub fn init_spec_catalog_from_file<P: AsRef<std::path::Path>>(
    path: P,
) -> Result<(), std::io::Error> {
    let bytes = std::fs::read(path)?;
    SPEC_CATALOG.get_or_init(|| {
        bincode::deserialize(&bytes).unwrap_or_else(|e| {
            panic!(
                "从文件加载规范目录失败（反序列化错误）: {}\n请确认文件是由 build.rs 生成的 di_table.bin。",
                e
            )
        })
    });
    Ok(())
}

/// 检查字典是否已初始化
///
/// 返回静态字典是否已经加载（无论是从嵌入的二进制数据还是外部文件）。
///
/// # 用途
///
/// - 避免重复初始化
/// - 延迟加载策略的判断
/// - 测试和调试
///
/// # 示例
///
/// ```rust,ignore
/// use spec_engine::{spec_catalog_initialized, get_spec_catalog};
///
/// if !spec_catalog_initialized() {
///     println!("字典尚未初始化，首次访问会触发加载");
/// }
///
/// let catalog = get_spec_catalog();  // 触发初始化
///
/// assert!(spec_catalog_initialized());
/// ```
pub fn spec_catalog_initialized() -> bool {
    SPEC_CATALOG.get().is_some()
}

// === 便捷 API ===

/// 使用默认 region 解析 DI（简化版 parse_di）
///
/// 这是 [`parse_di`] 的便捷包装，自动使用默认 region（"南网"）和通用方向（`None`）。
/// 适用于大多数不需要区分省份或方向的场景。
///
/// # 参数
///
/// - `di`: DI 标识（32位无符号整数）
/// - `protocol`: 协议名称（如 "csg13", "dlt645-2007"）
/// - `data`: 待解析的字节数据
///
/// # 返回值
///
/// - `Ok((Value, usize))`: 解析成功，返回值和消耗的字节数
/// - `Err(DictError)`: 解析失败
///
/// # 等价于
///
/// ```rust,ignore
/// parse_di(protocol, di, DEFAULT_REGION, None, data)
/// ```
///
/// # 示例
///
/// ```rust,ignore
/// use spec_engine::parse;
///
/// // BCD 编码的电能值: 34 12 00 00 -> 1234.00
/// let data = vec![0x34, 0x12, 0x00, 0x00];
/// let (value, consumed) = parse(0x00010000, "csg13", &data)?;
///
/// println!("解析结果: {:?}", value);
/// assert_eq!(consumed, 4);
/// ```
///
/// # 何时使用 parse_di
///
/// 如果需要指定特定的 region 或 direction，请使用 [`parse_di`]：
///
/// ```rust,ignore
/// // 指定广东省的定义
/// let (value, consumed) = parse_di("csg13", 0x00010000, "广东", None, &data)?;
///
/// // 指定上行方向
/// let (value, consumed) = parse_di("csg13", 0x00010000, "南网", Some("1"), &data)?;
/// ```
pub fn parse(di: u32, protocol: &str, data: &[u8]) -> Result<(Value, usize), DictError> {
    parse_di(protocol, di, DEFAULT_REGION, None, data)
}

/// 创建动态字典管理器（使用嵌入的静态字典作为基础）
///
/// 创建一个 [`DynamicCatalog`] 实例，使用编译时嵌入的静态字典作为基础层。
/// 可以在此基础上动态加载额外的 YAML 层，实现运行时字典扩展和覆盖。
///
/// # 层级优先级
///
/// 动态字典采用分层架构：
///
/// 1. **基础层**: 编译时嵌入的静态字典（优先级最低）
/// 2. **动态层**: 运行时加载的 YAML 层（按加载顺序，后加载优先级更高）
///
/// 查找时从最高优先级的层开始，找不到时依次向下查找。
///
/// # 使用场景
///
/// - **测试**: 加载测试用的临时定义
/// - **定制**: 针对特定客户或项目定制 DI 定义
/// - **热更新**: 运行时更新字典而无需重启
/// - **局方差异**: 不同地区的特殊定义
///
/// # 示例
///
/// ```rust,ignore
/// use spec_engine::create_dynamic_catalog;
///
/// let mut catalog = create_dynamic_catalog();
///
/// // 加载自定义层
/// catalog.load_yaml_dir("custom".into(), "path/to/yaml")?;
/// catalog.load_yaml_file("override".into(), "path/to/override.yaml")?;
///
/// // 查找会优先从动态层查找
/// if let Some(field) = catalog.lookup("csg13", 0x00010000, "南网", None) {
///     println!("字段: {}", field.name);
/// }
///
/// // 层管理
/// println!("已加载层: {:?}", catalog.list_layers());
/// catalog.stats().print();
/// ```
///
/// # 参见
///
/// - [`DynamicCatalog`](crate::DynamicCatalog): 动态字典管理器的详细文档
/// - [`dynamic_loader`](crate::dynamic_loader): 动态加载模块
pub fn create_dynamic_catalog() -> DynamicCatalog {
    DynamicCatalog::new(get_spec_catalog().clone())
}

/// 查找 DI 定义（不解析数据，只查找规范）
///
/// 在静态字典中查找指定 DI 的定义，不进行数据解析。这对于以下场景很有用：
///
/// - 检查 DI 是否存在
/// - 获取字段名称和元数据
/// - 预先验证 DI 的有效性
/// - 构建 DI 目录或索引
///
/// # 查找策略
///
/// 采用两级回退策略：
///
/// 1. 首先查找指定 region 的精确匹配
/// 2. 如果找不到且 region 不是默认值，回退到默认 region
///
/// 这样可以优先使用区域特定的定义，同时保证通用定义的可用性。
///
/// # 参数
///
/// - `di`: DI 标识（32位无符号整数）
/// - `protocol`: 协议名称（如 "csg13", "dlt645-2007"）
/// - `region`: 区域/省份（如 "南网", "广东", "云南"）
///
/// # 返回值
///
/// - `Some(&NamedField)`: 找到定义
/// - `None`: 未找到
///
/// # 示例
///
/// ```rust,ignore
/// use spec_engine::lookup_di_spec;
///
/// // 查找广东的定义
/// if let Some(spec) = lookup_di_spec(0x00010000, "csg13", "广东") {
///     println!("字段名: {}", spec.name);
///     println!("规格: {:?}", spec.spec);
/// } else {
///     println!("DI 不存在");
/// }
///
/// // 遍历查找多个 DI
/// let di_list = vec![0x00010000, 0x00010100, 0x00010200];
/// for di in di_list {
///     if let Some(spec) = lookup_di_spec(di, "csg13", "南网") {
///         println!("0x{:08X}: {}", di, spec.name);
///     }
/// }
/// ```
///
/// # 与 parse_di 的区别
///
/// - `lookup_di_spec`: 只查找定义，不解析数据，快速但功能有限
/// - [`parse_di`]: 查找定义并解析数据，返回解析结果
///
/// 如果需要解析数据，请使用 [`parse_di`] 或 [`parse`]。
pub fn lookup_di_spec(di: u32, protocol: &str, region: &str) -> Option<&'static NamedField> {
    let catalog = get_spec_catalog();
    
    // 1. 精确匹配
    if let Some(field) = catalog.get(&(protocol.to_string(), di, region.to_string(), None)) {
        return Some(field);
    }
    
    // 2. 回退到默认 region
    if region != DEFAULT_REGION {
        if let Some(field) =
            catalog.get(&(protocol.to_string(), di, DEFAULT_REGION.to_string(), None))
        {
            return Some(field);
        }
    }
    
    None
}

// === Prelude 模块 ===

/// 常用 API 的便捷导入
///
/// # 示例
///
/// ```rust,ignore
/// use spec_engine::prelude::*;
///
/// let (value, consumed) = parse_di(0x00010000, "csg13", DEFAULT_REGION, &data)?;
/// let mut catalog = create_dynamic_catalog();
/// ```
pub mod prelude {
    pub use super::{
        // 解析 API
        parse_di,
        parse,
        parse_field,
        
        // 字典访问
        get_spec_catalog,
        lookup_di_spec,
        
        // 动态加载
        create_dynamic_catalog,
        DynamicCatalog,
        
        // 常量
        DEFAULT_REGION,
        
        // 错误
        DictError,
        
        // 类型
        FieldSpec,
        NamedField,
        Encoding,
        Value,
    };
}
