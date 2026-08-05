mod context;
mod decode;
mod error;
mod registry;
mod repeat;

// 新架构模块
mod engine;
mod config;

// 动态加载模块（内部）
mod dynamic_loader;

// Prelude 模块
pub mod prelude;

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

// === 重新导出：新架构 API ===

/// Engine 核心类型
pub use engine::Engine;

/// 配置管理
pub use config::{EngineConfig, ConfigSource};

/// 动态加载基础类型
pub use dynamic_loader::{Layer, LayerStats, DiTable, DiKey, DEFAULT_REGION};

// Value 类型从 spec_compiler 导入
pub use spec_compiler::types::Value;

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

// === 内部辅助：静态字典访问（供 build 使用）===

use std::collections::HashMap;
use std::sync::OnceLock;

static SPEC_CATALOG_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/di_table.bin"));
static SPEC_CATALOG: OnceLock<HashMap<(String, u32, String, Option<String>), NamedField>> =
    OnceLock::new();

pub(crate) fn get_spec_catalog() -> &'static HashMap<(String, u32, String, Option<String>), NamedField> {
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