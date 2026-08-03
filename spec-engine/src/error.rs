//! 错误类型定义
//!
//! 本模块定义了解析过程中可能遇到的所有错误类型。所有错误都实现了
//! `std::error::Error` trait，可以与标准错误处理机制无缝集成。
//!
//! # 主要错误类型
//!
//! - [`DictError`]: 解析过程中的所有错误
//!
//! # 错误分类
//!
//! ## 数据相关
//!
//! - `UnexpectedEof`: 缓冲区数据不足
//!
//! ## 字典相关
//!
//! - `UnknownDi`: DI 在字典中不存在
//! - `MissingRef`: 引用的字段不存在
//! - `UnknownSwitchCase`: switch 分支未命中
//!
//! ## 协议相关
//!
//! - `UnknownProtocol`: 协议未注册
//! - `ExternalParseError`: 外部协议解析失败
//!
//! ## 处理器相关
//!
//! - `UnknownHandler`: 自定义处理器未注册
//!
//! # 示例
//!
//! ```rust,ignore
//! use spec_engine::{parse, DictError};
//!
//! match parse(0x00010000, "csg13", &data) {
//!     Ok((value, consumed)) => {
//!         println!("成功: {:?}", value);
//!     }
//!     Err(DictError::UnexpectedEof { needed, available }) => {
//!         eprintln!("数据不足: 需要 {} 字节, 实际 {} 字节", needed, available);
//!     }
//!     Err(DictError::UnknownDi { protocol, di, region, dir }) => {
//!         eprintln!("未知 DI: {} 0x{:08X} [{}] {:?}", protocol, di, region, dir);
//!     }
//!     Err(DictError::MissingRef(ref_id)) => {
//!         eprintln!("缺少引用字段: {}", ref_id);
//!     }
//!     Err(e) => {
//!         eprintln!("其他错误: {}", e);
//!     }
//! }
//! ```

use thiserror::Error;
use spec_compiler::types::Direction;

/// 用于错误消息中显示方向的包装类型
#[derive(Debug)]
pub struct DirectionDisplay(pub Option<Direction>);

impl std::fmt::Display for DirectionDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(dir) => write!(f, "{}", dir),
            None => write!(f, "通用"),
        }
    }
}

/// 解析错误
///
/// 包含解析过程中可能遇到的所有错误类型。每个错误变体都包含详细的上下文信息，
/// 有助于快速定位和解决问题。
///
/// # 错误变体
///
/// ## UnexpectedEof
///
/// 缓冲区数据不足，无法完成解析。
///
/// ```rust,ignore
/// Err(DictError::UnexpectedEof { needed: 4, available: 2 })
/// ```
///
/// ## MissingRef
///
/// 引用的字段在上下文中不存在。通常是 `count_ref`、`length_ref` 或 `dict_ref`
/// 引用了一个不存在或尚未解析的字段。
///
/// ```rust,ignore
/// Err(DictError::MissingRef("data_length".to_string()))
/// ```
///
/// ## UnknownDi
///
/// 指定的 DI 在字典中不存在。包含完整的查找键信息。
///
/// ```rust,ignore
/// Err(DictError::UnknownDi {
///     protocol: "csg13".to_string(),
///     di: 0x00010000,
///     region: "南网".to_string(),
///     dir: DirectionDisplay(None),
/// })
/// ```
///
/// ## UnknownSwitchCase
///
/// switch 字段的值不匹配任何已定义的 case，且没有 default 分支。
///
/// ```rust,ignore
/// Err(DictError::UnknownSwitchCase {
///     on: "data_type".to_string(),
///     key: "99".to_string(),
/// })
/// ```
///
/// ## UnknownProtocol
///
/// 外部协议解析器未注册。需要通过 `registry::register_external_parser` 注册。
///
/// ```rust,ignore
/// Err(DictError::UnknownProtocol("modbus".to_string()))
/// ```
///
/// ## UnknownHandler
///
/// 自定义处理器未注册。需要通过 `registry::register_custom_handler` 注册。
///
/// ```rust,ignore
/// Err(DictError::UnknownHandler("special_decoder".to_string()))
/// ```
///
/// ## ExternalParseError
///
/// 外部协议解析失败。包含来自外部解析器的错误消息。
///
/// ```rust,ignore
/// Err(DictError::ExternalParseError("DL/T645 校验和错误".to_string()))
/// ```
///
/// # 使用 `?` 运算符
///
/// 所有错误都实现了 `std::error::Error`，可以直接使用 `?` 运算符传播：
///
/// ```rust,ignore
/// fn process_data(data: &[u8]) -> Result<Value, DictError> {
///     let (value, _) = parse(0x00010000, "csg13", data)?;
///     Ok(value)
/// }
/// ```
///
/// # 错误转换
///
/// 通过 `thiserror` crate，错误消息会自动格式化：
///
/// ```rust,ignore
/// let err = DictError::UnexpectedEof { needed: 10, available: 5 };
/// println!("{}", err);  // "缓冲区不足: 需要 10 字节, 实际 5"
/// ```
#[derive(Debug, Error)]
pub enum DictError {
    /// 缓冲区不足
    #[error("缓冲区不足: 需要 {needed} 字节, 实际 {available}")]
    UnexpectedEof { needed: usize, available: usize },

    /// 缺少引用字段
    #[error("缺少引用字段: {0}")]
    MissingRef(String),

    /// 未知的 DI
    #[error("未知的 {protocol} DI: 0x{di:08X} (region={region}, dir={dir})")]
    UnknownDi { 
        protocol: String, 
        di: u32, 
        region: String, 
        dir: DirectionDisplay,
    },

    /// switch 未命中任何分支
    #[error("switch 字段 {on} 未命中任何分支: {key}")]
    UnknownSwitchCase { on: String, key: String },

    /// 未知的协议
    #[error("未知的协议: {0}")]
    UnknownProtocol(String),

    /// 未知的处理器
    #[error("未知的处理器: {0}")]
    UnknownHandler(String),

    /// 外部协议解析错误
    #[error("外部协议解析失败: {0}")]
    ExternalParseError(String),
}
