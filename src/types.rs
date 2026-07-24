// 运行时字段类型定义
//
// 这份文件被两处编译单元各自独立编译一次：
// 1. 正常作为 spec-engine crate 的一个模块（供 lib.rs/parser.rs 等使用）；
// 2. 被 build.rs 用 include!("src/types.rs") 原样再拼进它自己的源码中间，
//    供 build.rs 在编译期直接构造 FieldSpec 值树并用 bincode 序列化成二进制。
//    正因为是"拼进中间"，这里不能用 //! 模块级 doc 注释（inner doc comment
//    只能出现在文件/作用域最开头，前面不能已经有别的 item），必须用普通
//    // 注释；作为 spec-engine 自己的模块看待时用普通注释也完全没问题，只是
//    少了一条 module doc，无伤大雅。
//
// 两份编译产物互不相干（build.rs 编译出的那份只在构建期临时用一次，
// 不会进入最终产物），这样 build.rs 不需要反过来依赖 spec-engine 自身
// （crate 还没编译出来），也不需要引入额外的子 crate。
//
// 同理，下面的 `use serde::{Deserialize, Serialize};` 和
// `use std::collections::HashMap;` 拼进 build.rs 后也会跟 build.rs 自己
// 顶部的同名 use 冲突（Rust 不允许同一符号在同一作用域被 use 两次，即使
// 指向完全相同的 item），所以 build.rs 里对应的两行已经删掉，统一由这里
// include 进去的这两行提供。修改这两行时记得同步检查 build.rs 顶部有没有
// 重复。
//
// 正因为要被 bincode 序列化/反序列化，FieldSpec 树上所有类型都必须
// derive Serialize/Deserialize——新增字段/新增枚举分支时记得同步加。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 字节序
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Endian {
    /// 小端（低字节在前）
    Little,
    /// 大端（高字节在前）
    Big,
}

/// 时间编码方式，`type: bcd` 或 `type: bin` 影响时间字段的字节解释
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeEncoding {
    Bcd,
    Bin { endian: Endian },
}

/// 字段长度定义
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldLength {
    /// 固定长度
    Fixed(usize),
    /// 引用前面字段的解析结果
    Ref(String),
    /// 表达式形式的长度，运行时求值（例如 "2*ref(pn_count)"）
    Expr(String),
}

/// 编码方式
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Encoding {
    /// 二进制数值
    Bin { endian: Endian, signed: bool },
    /// BCD 编码
    ///
    /// 如果没有在 YAML 里显式设置 endian，则按逆序（reverse byte order）解析。
    /// 如果设置了 endian，则按给定字节序解析。
    Bcd {
        decimals: u8,
        signed: bool,
        endian: Option<Endian>,
    },
    /// ASCII 字符串
    Ascii,
    /// 十六进制字符串
    Hex,
    /// 时间格式
    Time { format: String, encoding: TimeEncoding },
    /// 原始字节
    Raw,
}

/// 外部协议长度定义
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExternalLength {
    /// 剩余全部
    Remaining,
    /// 固定长度
    Fixed(usize),
    /// 引用其他字段的值
    Ref(String),
}

/// 位域规格
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BitSpec {
    /// 位范围 [start, end]
    pub range: (usize, usize),
    /// 字段名称
    pub name: String,
    /// 可选的 bit 级别 ref_id，用于按 bit 选择后续解析
    pub ref_id: Option<String>,
    /// 枚举映射
    pub enum_map: Option<HashMap<String, String>>,
}

/// 解析值
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// 整数
    Int(i64),
    /// 浮点数
    Float(f64),
    /// 字符串
    Str(String),
    /// 字节数组
    Bytes(Vec<u8>),
    /// 列表
    List(Vec<Value>),
    /// 键值映射
    Map(Vec<(String, Value)>),
    /// 带单位的值
    WithUnit { value: Box<Value>, unit: String },
    /// 解析节点：字段名称、原始字节、解析值
    Node {
        name: String,
        raw: Vec<u8>,
        value: Box<Value>,
    },
    /// 位项：便于表示 bitmask/bitpattern 的单个位信息和可选的后续元素
    /// 使用区间表示位范围（支持单个位和多位区间）
    Bit {
        bit_start: usize,
        bit_end: usize,
        bit_value: u64,
        bit_byte: Vec<u8>,
        value: Option<Box<Value>>,
    },
    /// 显式跳过输出的占位值
    Skip,
    /// 解析失败结果
    Invalid { reason: String },
    Pn(i64),
}

impl Value {
    /// 获取单位（如字段定义里有 unit）
    pub fn unit(&self) -> Option<&str> {
        match self {
            Value::WithUnit { unit, .. } => Some(unit),
            Value::Node { value, .. } => value.unit(),
            Value::Bit { .. } => None,
            _ => None,
        }
    }

    /// 转换为字符串引用
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s.as_str()),
            Value::WithUnit { value, .. } => value.as_str(),
            Value::Node { value, .. } => value.as_str(),
            Value::Bit { .. } => None,
            _ => None,
        }
    }

    /// 转换为整数
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::WithUnit { value, .. } => value.as_int(),
            Value::Node { value, .. } => value.as_int(),
            Value::Bit { value, .. } => value.as_deref().and_then(|v| v.as_int()),
            _ => None,
        }
    }

    /// 转换为浮点数
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Int(i) => Some(*i as f64),
            Value::WithUnit { value, .. } => value.as_float(),
            Value::Node { value, .. } => value.as_float(),
            Value::Bit { value, .. } => value.as_deref().and_then(|v| v.as_float()),
            _ => None,
        }
    }

    /// 转换为 u32（供 count_ref / dict_ref / external length 引用使用）
    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Value::Int(i) if *i >= 0 => Some(*i as u32),
            Value::Float(f) if *f >= 0.0 => Some(*f as u32),
            Value::WithUnit { value, .. } => value.as_u32(),
            Value::Node { value, .. } => value.as_u32(),
            Value::Bit { value, .. } => value.as_deref().and_then(|v| v.as_u32()),
            _ => None,
        }
    }

    /// 转换为 usize（供 count_ref 使用）
    pub fn as_usize(&self) -> Option<usize> {
        self.as_u32().map(|v| v as usize)
    }

    /// 以树形结构格式化解析结果，方便 UI 展示
    pub fn format_tree(&self) -> String {
        let mut buf = String::new();
        self.fmt_tree(&mut buf, 0);
        buf
    }

    fn fmt_tree(&self, buf: &mut String, indent: usize) {
        let pad = "  ".repeat(indent);
        match self {
            Value::Int(i) => {
                buf.push_str(&format!("{}Int({})\n", pad, i));
            }
            Value::Float(f) => {
                buf.push_str(&format!("{}Float({})\n", pad, f));
            }
            Value::Str(s) => {
                buf.push_str(&format!("{}Str(\"{}\")\n", pad, s));
            }
            Value::Bytes(bytes) => {
                buf.push_str(&format!("{}Bytes({})\n", pad, format_bytes(bytes)));
            }
            Value::List(items) => {
                buf.push_str(&format!("{}List\n", pad));
                for item in items {
                    item.fmt_tree(buf, indent + 1);
                }
            }
            Value::Map(entries) => {
                buf.push_str(&format!("{}Map\n", pad));
                for (k, v) in entries {
                    buf.push_str(&format!("{}  {}:\n", pad, k));
                    v.fmt_tree(buf, indent + 2);
                }
            }
            Value::WithUnit { value, unit } => {
                buf.push_str(&format!("{}WithUnit(unit=\"{}\")\n", pad, unit));
                value.fmt_tree(buf, indent + 1);
            }
            Value::Invalid { reason } => {
                buf.push_str(&format!("{}Invalid(reason=\"{}\")\n", pad, reason));
            }
            Value::Pn(n) => {
                buf.push_str(&format!("{}Pn({})\n", pad, n));
            }
            Value::Node { name, raw, value } => {
                let repr = format_bytes(raw);
                buf.push_str(&format!("{}{} [{}]\n", pad, name, repr));
                value.fmt_tree(buf, indent + 1);
            }
            Value::Bit {
                bit_start,
                bit_end,
                bit_value,
                bit_byte,
                value,
            } => {
                let repr = format_bytes(bit_byte);
                if bit_start == bit_end {
                    buf.push_str(&format!(
                        "{}BIT(bit={} bit_value={}) [{}]\n",
                        pad, bit_start, bit_value, repr
                    ));
                } else {
                    buf.push_str(&format!(
                        "{}BIT(range={}..{} bit_value={}) [{}]\n",
                        pad, bit_start, bit_end, bit_value, repr
                    ));
                }
                if let Some(elem) = value {
                    elem.fmt_tree(buf, indent + 1);
                }
            }
            Value::Skip => {
                buf.push_str(&format!("{}Skip\n", pad));
            }
        }
    }
}

fn format_bytes(bytes: &[u8]) -> String {
    let parts: Vec<String> = bytes.iter().map(|b| format!("0x{:02X}", b)).collect();
    format!("[{}]", parts.join(", "))
}

/// 格式化规格类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FormatType {
    Hex,
    Bcd,
    Bin,
}

/// 字节格式化规格
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FormatOrder {
    Normal,
    Reverse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormatSpec {
    /// 格式类型：hex|bcd|bin
    pub ftype: FormatType,
    /// 分组字节数（默认 1）
    pub group_bytes: Option<usize>,
    /// 分隔符（默认 ":")
    pub separator: Option<String>,
    /// 是否填充到固定宽度（hex 每字节 2 位）
    pub pad: Option<bool>,
    /// 字节序，仅对格式化显示有效
    pub byte_order: Option<Endian>,
    /// 组序，normal=原序，reverse=整体组序反转
    pub order: Option<FormatOrder>,
}

pub(crate) fn format_bytes_with_spec(bytes: &[u8], spec: &FormatSpec) -> String {
    let group = match spec.group_bytes {
        Some(n) if n > 0 => n,
        _ => 1,
    };
    let sep = spec.separator.as_deref();
    let pad = matches!(spec.pad, Some(true));
    let little_endian = spec.byte_order == Some(Endian::Little);
    let reverse_group_order = spec.order == Some(FormatOrder::Reverse);

    let mut parts: Vec<String> = Vec::new();
    for chunk in bytes.chunks(group) {
        let chunk_iter: Box<dyn Iterator<Item = &u8>> = if little_endian && chunk.len() > 1 {
            Box::new(chunk.iter().rev())
        } else {
            Box::new(chunk.iter())
        };
        let mut inner: Vec<String> = Vec::new();
        for b in chunk_iter {
            let s = match spec.ftype {
                FormatType::Hex => {
                    if pad {
                        format!("{:02X}", b)
                    } else {
                        format!("{:X}", b)
                    }
                }
                FormatType::Bcd => {
                    // BCD display stays hex-like for raw byte formatting.
                    if pad {
                        format!("{:02X}", b)
                    } else {
                        format!("{:X}", b)
                    }
                }
                FormatType::Bin => {
                    // Binary display with formatting shows raw bytes in decimal.
                    if pad {
                        format!("{:03}", b)
                    } else {
                        format!("{}", b)
                    }
                }
            };
            inner.push(s);
        }
        parts.push(inner.join(""));
    }
    if reverse_group_order {
        parts.reverse();
    }
    if let Some(sep) = sep {
        parts.join(sep)
    } else {
        parts.concat()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_with_spec_little_endian_group_bytes_one() {
        let spec = FormatSpec {
            ftype: FormatType::Bin,
            group_bytes: Some(1),
            separator: Some(".".into()),
            pad: Some(false),
            byte_order: Some(Endian::Little),
            order: None,
        };
        assert_eq!(format_bytes_with_spec(&[0x0A, 0x2F, 0x12, 0xE4], &spec), "10.47.18.228");
    }

    #[test]
    fn format_bytes_with_spec_little_endian_within_chunks() {
        let spec = FormatSpec {
            ftype: FormatType::Bin,
            group_bytes: Some(2),
            separator: Some(":".into()),
            pad: Some(false),
            byte_order: Some(Endian::Little),
            order: None,
        };
        assert_eq!(format_bytes_with_spec(&[0x01, 0x02, 0x03, 0x04], &spec), "21:43");
    }

    #[test]
    fn format_bytes_with_spec_reverse_order() {
        let spec = FormatSpec {
            ftype: FormatType::Bin,
            group_bytes: Some(1),
            separator: Some(".".into()),
            pad: Some(false),
            byte_order: None,
            order: Some(FormatOrder::Reverse),
        };
        assert_eq!(format_bytes_with_spec(&[0x0A, 0x2F, 0x12, 0xE4], &spec), "228.18.47.10");
    }

    #[test]
    fn format_bytes_with_spec_no_defaults_when_none() {
        let spec = FormatSpec {
            ftype: FormatType::Hex,
            group_bytes: None,
            separator: None,
            pad: None,
            byte_order: None,
            order: Some(FormatOrder::Reverse),
        };
        assert_eq!(format_bytes_with_spec(&[0x0A, 0x2F, 0x12], &spec), "122FA");
    }

    #[test]
    fn format_bytes_with_spec_group_bytes_without_separator() {
        let spec = FormatSpec {
            ftype: FormatType::Hex,
            group_bytes: Some(2),
            separator: None,
            pad: Some(true),
            byte_order: None,
            order: Some(FormatOrder::Reverse),
        };
        assert_eq!(format_bytes_with_spec(&[0x0A, 0x2F, 0x12, 0x34], &spec), "12340A2F");
    }
}

/// 字段规格（运行时，build.rs 已展开大部分节点）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldSpec {
    /// 定长字段，支持静态或引用其它字段的长度
    Fixed {
        encoding: Encoding,
        length: FieldLength,
        unit: Option<String>,
        enum_map: Option<HashMap<String, String>>,
        format: Option<FormatSpec>,
    },
    /// 位域
    BitField { length: usize, bits: Vec<BitSpec> },
    /// 条件分支
    Switch {
        on: String,
        cases: HashMap<String, Box<FieldSpec>>,
        /// 可选的 case 名称映射（case key -> name，从 YAML 的 case.name 处获取）
        case_names: Option<HashMap<String, String>>,
        default: Option<Box<FieldSpec>>,
    },
    /// 计数重复
    Repeat {
        count_ref: Option<String>,
        count_expr: Option<String>,
        bits_ref: Option<String>,
        /// 位读取方向：`msb` 或 `lsb`（如果需要）
        bit_direction: Option<String>,
        /// 位定义的迭代顺序：`asc` 或 `desc`
        iterate_order: Option<String>,
        bit_specs: Option<Vec<BitSpec>>,
        element: Box<FieldSpec>,
        /// 可选的元素名称模板，支持 {index}、{index0}、{id} 和 {bit_name} 占位符
        name_template: Option<String>,
        /// 可选的 id 表达式（如 "0x00010100 + index0*0x0100"），编译期
        /// 可用 `count`+`id_expr` 展开为具体命名字段。运行时仍保留
        /// `count_ref` 以支持动态计数。
        id_expr: Option<String>,
    },
    /// 按位图展开
    BitMask {
        length: usize,
        /// 位读取方向：`msb` 或 `lsb`
        bit_direction: Option<String>,
        /// 位定义的迭代顺序：`asc` 或 `desc`
        iterate_order: Option<String>,
        bit_specs: Vec<BitSpec>,
        element: Box<FieldSpec>,
        /// 可选的元素名称模板，支持 {index}、{index0}、{id} 和 {bit_name} 占位符
        name_template: Option<String>,
    },
    /// 跳过该分支，不产生输出结果
    Skip,
    /// 外部协议
    External {
        protocol: String,
        length: ExternalLength,
    },
    /// 容器（字段序列）
    Container(Vec<NamedField>),
    /// 自定义处理
    Custom(String),
    /// 运行时字典引用
    DictRef { di_ref: String },
    /// 信息点标识 DA（6.1.3）：固定2字节，DA1(位掩码)+DA2(组号)，
    /// 运行时按组号动态计算命中的测量点号，含 p0/全选两个哨兵值。
    /// 不需要任何编译期配置，故为无字段的单元变体。
    InfoPoint,
    /// 数据标识编码 DI（6.1.4）：固定4字节，按 DI0 DI1 DI2 DI3 传输顺序
    /// （小端）读出后，去同协议全局 DI 字典查名称，只取名字不解析该 DI
    /// 自身的数据内容（"只有数据标识无数据标识内容"的场景，如任务定义里
    /// 枚举包含哪些DI）。同样不需要任何编译期配置。
    DiCode,
}

/// 命名字段
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedField {
    /// DI 标识（可选）
    pub id: Option<String>,
    /// 仅用来支持 run-time ref 绑定的引用 id（例如 count_ref）
    pub ref_id: Option<String>,
    /// 字段名称
    pub name: String,
    /// 字段规格
    pub spec: FieldSpec,
    /// 可选的显示格式规格（由 build.rs 从 YAML 的 `format` 字段透传）
    pub format: Option<FormatSpec>,
}
