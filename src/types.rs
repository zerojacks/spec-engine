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

/// 字段长度定义
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldLength {
    /// 固定长度
    Fixed(usize),
    /// 引用前面字段的解析结果
    Ref(String),
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
    Time { format: String },
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
    pub range: (u8, u8),
    /// 字段名称
    pub name: String,
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
}

impl Value {
    /// 获取单位（如字段定义里有 unit）
    pub fn unit(&self) -> Option<&str> {
        match self {
            Value::WithUnit { unit, .. } => Some(unit),
            Value::Node { value, .. } => value.unit(),
            _ => None,
        }
    }

    /// 转换为字符串引用
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s.as_str()),
            Value::WithUnit { value, .. } => value.as_str(),
            Value::Node { value, .. } => value.as_str(),
            _ => None,
        }
    }

    /// 转换为整数
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::WithUnit { value, .. } => value.as_int(),
            Value::Node { value, .. } => value.as_int(),
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
            Value::Node { name, raw, value } => {
                buf.push_str(&format!("{}{} [{}]\n", pad, name, format_bytes(raw)));
                value.fmt_tree(buf, indent + 1);
            }
        }
    }
}

fn format_bytes(bytes: &[u8]) -> String {
    let parts: Vec<String> = bytes.iter().map(|b| format!("0x{:02X}", b)).collect();
    format!("[{}]", parts.join(", "))
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
    },
    /// 位域
    BitField { length: usize, bits: Vec<BitSpec> },
    /// 条件分支
    Switch {
        on: String,
        cases: HashMap<String, Box<FieldSpec>>,
        default: Option<Box<FieldSpec>>,
    },
    /// 计数重复
    Repeat {
        count_ref: String,
        element: Box<FieldSpec>,
        /// 可选的元素名称模板，支持 {index}、{index0} 和 {id} 占位符
        name_template: Option<String>,
        /// 可选的 id 表达式（如 "0x00010100 + index0*0x0100"），编译期
        /// 可用 `count`+`id_expr` 展开为具体命名字段。运行时仍保留
        /// `count_ref` 以支持动态计数。
        id_expr: Option<String>,
    },
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
}
