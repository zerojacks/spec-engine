//! 常用导出（prelude）——为上层使用者提供简洁的 API 入口
//!
//! 使用示例：
//!
//! ```rust
//! use spec_engine::prelude::*;
//!
//! let (v, n) = parse_di("proto", 0x00010001, DEFAULT_REGION, None, &[]).unwrap();
//! ```

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

// 亦可通过 `use spec_engine::prelude::*;` 引入解析结果的辅助方法
