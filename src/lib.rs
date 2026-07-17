//! 计量自动化终端上行通信规约 DI 字典解析库
//!
//! ## 功能特性
//!
//! - 编译期从 YAML 生成静态 DI 字典
//! - 支持多种编码方式：BCD、BIN、ASCII、Hex、Time
//! - 支持位域解析、条件分支、重复结构
//! - 支持外部协议嵌套和运行时字典引用
//! - 支持按省份/局方（region）覆盖 DI 定义，查不到时自动回退到通用定义
//!
//! ## 使用示例
//!
//! ```rust,ignore
//! use spec_engine::{parse_di, get_di_table, Value, DEFAULT_REGION};
//!
//! // 按 DI 码解析报文（不区分省份时传 DEFAULT_REGION）
//! let raw = vec![0x01];
//! let (value, consumed) = parse_di(0x00010001, DEFAULT_REGION, &raw)?;
//! println!("解析结果: {:?}, 消耗 {} 字节", value, consumed);
//!
//! // 按具体省份解析：查不到该省份的专门定义会自动回退到通用定义
//! let (value, consumed) = parse_di(0x00010001, "GD", &raw)?;
//! ```

mod context;
mod decode;
mod error;
mod parser;
mod registry;
mod types;

pub use context::Context;
pub use decode::{
    decode_ascii, decode_bcd_u64, decode_bin_u64, decode_hex, decode_signed_bcd, decode_signed_bin,
    decode_time,
};
pub use error::DictError;
pub use parser::{parse_di, parse_field, DEFAULT_REGION};
pub use registry::{
    get_custom_handler, get_external_parser, init_registries, CustomHandler, ExternalParser,
};
pub use types::*;

use std::collections::HashMap;
use std::sync::OnceLock;

/// build.rs 展开完 YAML 字典后，用 bincode 序列化写进 OUT_DIR/di_table.bin
/// 的整块字节。`include_bytes!` 只是把文件内容原样拷进程序的只读数据段，
/// 不管字典有多少条目，这一步对 rustc 来说都是常数开销——不会再出现旧
/// 方案（把每条目生成成一行 `m.insert(...)` 源码、几千条堆进同一个函数）
/// 那种编译期超线性开销和 debug 构建下的栈帧溢出问题。
static DI_TABLE_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/di_table.bin"));

/// 全局 DI 表，首次访问时从 `DI_TABLE_BYTES` 反序列化一次并缓存。
/// key 是 `(protocol, DI码, region, dir)`——同一个 DI 在不同协议/省份/方向
/// 下可以有不同定义，字典里没写 `region:` 的条目落在 `DEFAULT_REGION`
/// 这个通用桶里。
static DI_TABLE: OnceLock<HashMap<(String, u32, String, Option<String>), NamedField>> =
    OnceLock::new();

pub fn get_di_table() -> &'static HashMap<(String, u32, String, Option<String>), NamedField> {
    DI_TABLE.get_or_init(|| {
        bincode::deserialize(DI_TABLE_BYTES).unwrap_or_else(|e| {
            panic!(
                "DI 字典二进制反序列化失败: {}\n\
                 通常是 di_table.bin 与当前 FieldSpec 类型定义不匹配（比如改过\n\
                 src/types.rs 但没有重新构建）导致的，试试 `cargo clean` 后重新构建。",
                e
            )
        })
    })
}

/// 在运行时（程序启动阶段）从外部 `di_table.bin` 文件初始化 DI 表。
///
/// 语义：如果全局 DI 表尚未初始化，则从指定路径读取 bincode 二进制并
/// 反序列化为内部 DI 表；若已经初始化则不会替换（调用方应在首次解析前调用）。
pub fn init_di_table_from_file<P: AsRef<std::path::Path>>(path: P) -> Result<(), std::io::Error> {
    let bytes = std::fs::read(path)?;
    DI_TABLE.get_or_init(|| {
        bincode::deserialize(&bytes).unwrap_or_else(|e| {
            panic!(
                "从文件加载 DI 表失败（反序列化错误）: {}\n请确认文件是由 build.rs 生成的 di_table.bin。",
                e
            )
        })
    });
    Ok(())
}

/// 返回 DI 表是否已初始化（用于运行时检查）
pub fn di_table_initialized() -> bool {
    DI_TABLE.get().is_some()
}
