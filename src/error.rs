//! 错误类型定义

use thiserror::Error;
use crate::types::Direction;

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
