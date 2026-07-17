//! 错误类型定义

use thiserror::Error;

/// 解析错误
#[derive(Debug, Error)]
pub enum DictError {
    /// 缓冲区不足
    #[error("缓冲区不足: 需要 {needed} 字节, 实际 {available}")]
    UnexpectedEof { needed: usize, available: usize },

    /// 缺少引用字段
    #[error("缺少引用字段: {0}")]
    MissingRef(String),

    /// 未知的 DI
    #[error("未知的 {protocol} DI: 0x{di:08X}")]
    UnknownDi { protocol: String, di: u32 },

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
