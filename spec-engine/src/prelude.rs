//! Prelude 模块 - 常用 API 的便捷导入
//!
//! 提供最常用的类型和函数，方便用户通过 `use spec_engine::prelude::*;` 一次性导入。
//!
//! # 示例
//!
//! ```rust,ignore
//! use spec_engine::prelude::*;
//!
//! // 方式 1: 使用默认引擎（最简单）
//! let engine = Engine::new_default();
//! let (value, consumed) = engine.parse("csg13", 0x00010000, "南网", None, &data)?;
//!
//! // 方式 2: 使用配置（支持动态层）
//! let engine = EngineConfig::new()
//!     .yaml_dir("custom", "config/custom")
//!     .build()?;
//! let (value, consumed) = engine.parse("csg13", 0x00010000, "南网", None, &data)?;
//! ```

// === 核心引擎 ===
pub use crate::Engine;

// === 配置管理 ===
pub use crate::config::{EngineConfig, ConfigSource};

// === 动态加载 ===
pub use crate::dynamic_loader::{Layer, LayerStats, DiTable, DiKey, DEFAULT_REGION};

// === 类型系统 ===
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
    BitSpec,
    ExternalLength,
};

// Value 从 spec_compiler::types 导入
pub use spec_compiler::types::Value;

// === 错误处理 ===
pub use crate::error::DictError;

// === 高级 API ===
pub use crate::context::Context;