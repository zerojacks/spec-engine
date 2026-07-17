//! 外部协议解析器与自定义处理器注册表

use super::{DictError, Value};
use std::collections::HashMap;
use std::sync::OnceLock;

/// 外部协议解析器类型
pub type ExternalParser = fn(&[u8]) -> Result<Value, String>;

/// 自定义处理器类型
pub type CustomHandler = fn(&[u8]) -> Result<(Value, usize), DictError>;

/// 外部协议注册表
static EXTERNAL_REGISTRY: OnceLock<HashMap<&'static str, ExternalParser>> = OnceLock::new();

/// 自定义处理器注册表
static CUSTOM_REGISTRY: OnceLock<HashMap<&'static str, CustomHandler>> = OnceLock::new();

/// 获取外部协议解析器
pub fn get_external_parser(protocol: &str) -> Option<ExternalParser> {
    EXTERNAL_REGISTRY
        .get()
        .and_then(|m| m.get(protocol).copied())
}

/// 获取自定义处理器
pub fn get_custom_handler(handler: &str) -> Option<CustomHandler> {
    CUSTOM_REGISTRY.get().and_then(|m| m.get(handler).copied())
}

/// 初始化注册表
pub fn init_registries() {
    // 注册 DL/T 645 协议解析器
    let mut external: HashMap<&'static str, ExternalParser> = HashMap::new();
    external.insert("dlt645-2007", parse_dlt645_demo);
    EXTERNAL_REGISTRY.set(external).ok();

    // 注册自定义处理器
    let mut custom: HashMap<&'static str, CustomHandler> = HashMap::new();
    custom.insert("parse_xxx_field", parse_xxx_demo);
    custom.insert("IPWITHPORT", parse_ip_with_port);
    CUSTOM_REGISTRY.set(custom).ok();
}

/// DL/T 645 示例解析器（仅演示）
fn parse_dlt645_demo(raw: &[u8]) -> Result<Value, String> {
    Ok(Value::Str(format!("[DL/T 645] {}", super::decode_hex(raw))))
}

/// 自定义解析示例
fn parse_xxx_demo(raw: &[u8]) -> Result<(Value, usize), DictError> {
    Ok((Value::Str(super::decode_hex(raw)), raw.len()))
}

/// IP+端口自定义处理器示例
fn parse_ip_with_port(raw: &[u8]) -> Result<(Value, usize), DictError> {
    if raw.len() < 8 {
        return Err(DictError::UnexpectedEof {
            needed: 8,
            available: raw.len(),
        });
    }
    let ip = format!("{}.{}.{}.{}", raw[0], raw[1], raw[2], raw[3]);
    let port = u16::from_be_bytes([raw[4], raw[5]]);
    Ok((Value::Str(format!("{}:{}", ip, port)), 8))
}
