//! Prelude 模块 - 常用 API 的便捷导入
//!
//! 本模块重新导出 spec-engine 中最常用的类型和函数，方便一次性导入。
//!
//! # 使用方式
//!
//! ```rust,ignore
//! use spec_engine::prelude::*;
//!
//! // 现在可以直接使用所有常用 API
//! let (value, consumed) = parse_di("csg13", 0x00010000, DEFAULT_REGION, None, &data)?;
//! let catalog = get_spec_catalog();
//! let mut dynamic = create_dynamic_catalog();
//! ```
//!
//! # 包含的 API
//!
//! ## 解析函数
//!
//! - [`parse_di`]: 完整的 DI 解析 API
//! - [`parse_field`]: 直接解析 FieldSpec
//!
//! ## 字典访问
//!
//! - [`get_spec_catalog`]: 获取静态字典
//! - [`init_spec_catalog_from_file`]: 从文件加载字典
//! - [`spec_catalog_initialized`]: 检查字典是否已初始化
//!
//! ## 初始化
//!
//! - [`init_registries`]: 初始化注册器
//!
//! ## 上下文
//!
//! - [`Context`]: 解析上下文
//!
//! ## 错误类型
//!
//! - [`DictError`]: 解析错误
//!
//! ## 值类型
//!
//! - [`Value`]: 解析结果值
//!
//! ## 解码函数
//!
//! - [`decode_ascii`]: ASCII 字符串解码
//! - [`decode_bcd_u64`]: BCD 解码
//! - [`decode_bin_u64`]: 二进制解码
//! - [`decode_hex`]: 十六进制解码
//! - [`decode_signed_bcd`]: 有符号 BCD 解码
//! - [`decode_signed_bin`]: 有符号二进制解码
//! - [`decode_time`]: 时间解码
//!
//! ## 常量
//!
//! - [`DEFAULT_REGION`]: 默认区域（"南网"）
//!
//! # 不包含的 API
//!
//! 以下 API 需要显式导入：
//!
//! - 动态加载相关：`DynamicCatalog`、`Layer` 等（从 [`crate::dynamic_loader`] 导入）
//! - 类型系统：`FieldSpec`、`Encoding` 等（从 [`spec_compiler`] 导入）
//! - 注册器：`register_external_parser` 等（从 [`crate::registry`] 导入）
//!
//! [`parse_di`]: crate::parse_di
//! [`parse_field`]: crate::parse_field
//! [`get_spec_catalog`]: crate::get_spec_catalog
//! [`init_spec_catalog_from_file`]: crate::init_spec_catalog_from_file
//! [`spec_catalog_initialized`]: crate::spec_catalog_initialized
//! [`init_registries`]: crate::init_registries
//! [`Context`]: crate::Context
//! [`DictError`]: crate::DictError
//! [`Value`]: spec_compiler::types::Value
//! [`decode_ascii`]: crate::decode_ascii
//! [`decode_bcd_u64`]: crate::decode_bcd_u64
//! [`decode_bin_u64`]: crate::decode_bin_u64
//! [`decode_hex`]: crate::decode_hex
//! [`decode_signed_bcd`]: crate::decode_signed_bcd
//! [`decode_signed_bin`]: crate::decode_signed_bin
//! [`decode_time`]: crate::decode_time
//! [`DEFAULT_REGION`]: crate::DEFAULT_REGION

pub use crate::spec_catalog_initialized as spec_catalog_initialized;
pub use crate::get_spec_catalog as get_spec_catalog;
pub use crate::init_spec_catalog_from_file as init_spec_catalog_from_file;
pub use crate::init_registries;
pub use crate::parser::{parse_di, parse_field, DEFAULT_REGION};
pub use crate::Context;
pub use crate::DictError;
pub use crate::Value;

pub use crate::decode::{
    decode_ascii, decode_bcd_u64, decode_bin_u64, decode_hex, decode_signed_bcd, decode_signed_bin,
    decode_time,
};