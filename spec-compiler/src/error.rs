//! 编译器错误类型定义

use std::io;
use thiserror::Error;

/// 编译器错误类型
#[derive(Error, Debug)]
pub enum CompilerError {
    /// YAML 语法错误
    #[error("YAML 解析失败: {message}")]
    YamlParse {
        message: String,
        line: Option<usize>,
        column: Option<usize>,
    },

    /// 语义验证错误
    #[error("语义错误: {context}: {reason}")]
    SemanticError { context: String, reason: String },

    /// 未知模板引用
    #[error("未知模板引用: template_id={template_id}, protocol={protocol}, region={region}")]
    UnknownTemplate {
        template_id: String,
        protocol: String,
        region: String,
    },

    /// 未知 DI 引用
    #[error("未知 DI 引用: di={di}, protocol={protocol}, region={region}")]
    UnknownDi {
        di: String,
        protocol: String,
        region: String,
    },

    /// 循环引用
    #[error("检测到循环引用: {}", format_path(.path))]
    CircularReference { path: Vec<String> },

    /// 重复定义
    #[error("重复定义: key={key}, location1={location1}, location2={location2}")]
    DuplicateDefinition {
        key: String,
        location1: String,
        location2: String,
    },

    /// 缺少必需字段
    #[error("缺少必需字段: {field} in {context}")]
    MissingField { field: String, context: String },

    /// 无效的字段值
    #[error("无效的字段值: {field}={value} in {context}: {reason}")]
    InvalidFieldValue {
        field: String,
        value: String,
        context: String,
        reason: String,
    },

    /// 文件 I/O 错误
    #[error("文件 I/O 错误: {0}")]
    Io(#[from] io::Error),

    /// 序列化错误
    #[error("序列化错误: {0}")]
    Serialization(String),

    /// 反序列化错误
    #[error("反序列化错误: {0}")]
    Deserialization(String),
}

/// 加载错误类型
#[derive(Error, Debug)]
pub enum LoadError {
    /// 编译错误
    #[error("编译错误: {0}")]
    CompileError(#[from] CompilerError),

    /// 文件不存在
    #[error("文件不存在: {}", .0.display())]
    FileNotFound(std::path::PathBuf),

    /// 无效的二进制格式
    #[error("无效的二进制格式: {reason}")]
    InvalidBinaryFormat { reason: String },

    /// 反序列化错误
    #[error("反序列化错误: {0}")]
    DeserializeError(String),

    /// 层不存在
    #[error("层不存在: LayerId={0}")]
    LayerNotFound(usize),

    /// I/O 错误
    #[error("I/O 错误: {0}")]
    Io(#[from] io::Error),
}

// 辅助函数：格式化循环引用路径
fn format_path(path: &[String]) -> String {
    path.join(" -> ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compiler_error_display() {
        let err = CompilerError::YamlParse {
            message: "unexpected token".to_string(),
            line: Some(42),
            column: Some(10),
        };
        assert!(err.to_string().contains("YAML 解析失败"));
    }

    #[test]
    fn test_circular_reference_format() {
        let err = CompilerError::CircularReference {
            path: vec![
                "template_a".to_string(),
                "template_b".to_string(),
                "template_a".to_string(),
            ],
        };
        assert!(err.to_string().contains("template_a -> template_b -> template_a"));
    }

    #[test]
    fn test_load_error_from_compiler_error() {
        let compile_err = CompilerError::SemanticError {
            context: "field1".to_string(),
            reason: "invalid type".to_string(),
        };
        let load_err: LoadError = compile_err.into();
        assert!(matches!(load_err, LoadError::CompileError(_)));
    }
}
