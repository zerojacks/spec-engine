//! YAML AST 定义
//!
//! 这些类型直接对应 YAML 文件的结构，用于反序列化。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 通用（跨省份）定义落在这个桶里
pub const DEFAULT_REGION: &str = "南网";

/// 位域定义
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct RawBit {
    pub range: (usize, usize),
    pub name: String,
    #[serde(default)]
    pub ref_id: Option<String>,
    #[serde(rename = "enum", default)]
    pub enum_map: Option<HashMap<String, String>>,
}

/// 候选 DI 定义
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct RawCandidate {
    #[serde(default)]
    pub count: Option<usize>,
    #[serde(default)]
    pub count_ref: Option<String>,
    #[serde(default)]
    pub count_expr: Option<String>,
    #[serde(default)]
    pub id_expr: Option<String>,
    #[serde(default)]
    pub name_template: Option<String>,
    /// 内嵌的 element 定义，放在 `candidate_ids.element:` 下
    #[serde(default)]
    pub element: Option<Box<RawField>>,
}

/// Switch case 目标
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum RawCaseTarget {
    Name(String),
    Field(Box<RawField>),
}

/// 格式化规格（字符串或对象）
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum RawFormat {
    String(String),
    Object(FormatObject),
}

/// 格式化对象
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct FormatObject {
    #[serde(rename = "type")]
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub group_bytes: Option<usize>,
    #[serde(default)]
    pub separator: Option<String>,
    #[serde(default)]
    pub pad: Option<bool>,
    #[serde(default)]
    pub endian: Option<String>,
    #[serde(default)]
    pub order: Option<String>,
}

/// 字段定义（原始 AST）
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct RawField {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub ref_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "type", default)]
    pub ty: Option<String>,
    #[serde(default)]
    pub length: Option<serde_yaml::Value>,
    #[serde(default)]
    pub group_bytes: Option<usize>,
    #[serde(default)]
    pub separator: Option<String>,
    #[serde(default)]
    pub pad: Option<bool>,
    #[serde(default)]
    pub lengthrule: Option<String>,
    #[serde(default)]
    pub length_ref: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub decimal: Option<u8>,
    #[serde(rename = "enum", default)]
    pub enum_map: Option<HashMap<String, String>>,
    #[serde(default)]
    pub endian: Option<String>,
    #[serde(default)]
    pub signed: Option<bool>,
    #[serde(default)]
    pub format: Option<RawFormat>,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub bits: Option<Vec<RawBit>>,
    #[serde(default)]
    pub on: Option<String>,
    #[serde(default)]
    pub cases: Option<HashMap<String, RawCaseTarget>>,
    #[serde(default)]
    pub default: Option<RawCaseTarget>,
    #[serde(default)]
    pub count_ref: Option<String>,
    #[serde(default)]
    pub count_expr: Option<String>,
    #[serde(default)]
    pub bits_ref: Option<String>,
    #[serde(default)]
    pub bit_direction: Option<String>,
    #[serde(default)]
    pub iterate_order: Option<String>,
    #[serde(default)]
    pub count: Option<usize>,
    #[serde(default)]
    pub name_template: Option<String>,
    #[serde(default)]
    pub element: Option<Box<RawField>>,
    #[serde(rename = "ref", default)]
    pub ref_: Option<String>,
    #[serde(default)]
    pub dict_ref: Option<serde_yaml::Value>,
    #[serde(default)]
    pub template_ref: Option<String>,
    #[serde(default)]
    pub id_expr: Option<String>,
    /// 编译期要额外注册的候选 DI id（结构化），格式示例：
    /// candidate_ids:
    ///   count: 63
    ///   id_expr: "0x00030100 + index0*0x0100"
    ///   name_template: "费率{index}"
    #[serde(default)]
    pub candidate_ids: Option<RawCandidate>,
    #[serde(default)]
    pub protocol: Option<String>,
    /// `type: external` 专用——内嵌报文该用哪个外部协议解析（对应
    /// `registry.rs` 里注册的名字，如 `dlt645-2007`）。故意跟上面的
    /// `protocol` 分开命名：上面那个 `protocol` 是"这个DI属于哪个协议"的
    /// 分类维度（来自目录名/顶层条目），这个是"内嵌报文的协议"，两者语义
    /// 完全不同，撞同一个关键字会导致 `effective_protocol` 在 external
    /// 字段上把分类维度的 protocol 意外覆盖掉。
    #[serde(default)]
    pub external_protocol: Option<String>,
    #[serde(default)]
    pub items: Option<Vec<String>>,
    #[serde(default)]
    pub fields: Option<Vec<RawField>>,
    #[serde(default)]
    pub handler: Option<String>,
    /// 省份/局方覆盖。不写就是 `None`，构建期按 `DEFAULT_REGION` 处理。
    #[serde(default)]
    pub region: Option<Vec<String>>,
    /// 报文方向覆盖（单值，不是数组——一个条目要么跟方向无关，要么精确
    /// 属于某一个方向，不存在"属于多个方向"的中间态）。不写就是 `None`，
    /// 代表"跟方向无关，两个方向通用"；查表时只有 `dir` 显式写了的条目才
    /// 会要求精确匹配，没写的条目对任何方向的查询都是候选。
    #[serde(default)]
    pub dir: Option<String>,
}

/// 模板定义
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RawTemplate {
    pub id: String,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub region: Option<Vec<String>>,
    #[serde(default)]
    pub dir: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub length: Option<usize>,
    pub fields: Vec<RawField>,
}

impl Default for RawTemplate {
    fn default() -> Self {
        Self {
            id: String::new(),
            protocol: None,
            region: None,
            dir: None,
            length: None,
            fields: Vec::new(),
        }
    }
}

/// 字典定义（根节点）
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct RawDict {
    #[serde(default)]
    pub templates: Vec<RawTemplate>,
    #[serde(default)]
    pub data_items: Vec<RawField>,
}
