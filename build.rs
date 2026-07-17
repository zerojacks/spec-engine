//! build.rs —— 把 `schema/<protocol>/*.yaml` 展开成一份 DI 表，
//! 用 `bincode` 序列化成二进制写入 `OUT_DIR/di_table.bin`。
//!
//! ## 为什么不再生成 Rust 源码（历史上的 `generated.rs` 方案）
//!
//! 旧方案是把展开结果拼成 `fn build_di_table() { m.insert(...); ... }`
//! 这样一个巨型函数的源码文本，`include!` 进 `lib.rs`。字典条目一多
//! （几千到几万条，每条还可能因 `switch`/`repeat`/`bitfield` 展开出好几层
//! 嵌套表达式），会出两类问题：
//! 1. rustc 编译这个单体函数时，MIR 构建、借用检查、尤其是 LLVM 后端对
//!    超大函数体是超线性开销，编译时间/内存会随字典规模明显劣化；
//! 2. 更致命的是 **debug（未优化）构建下的运行期栈溢出**——debug 模式不
//!    做临时值消除，几千条语句在同一个函数栈帧里同时存活大量嵌套的
//!    `HashMap`/`String`/`Box<FieldSpec>` 临时对象，栈帧体积远超默认栈
//!    大小（Windows 主线程默认 1MB），一调用这个函数就 `STATUS_STACK_OVERFLOW`。
//!    这两个问题都随条目数线性甚至更快地恶化，字典涨到百万级必然复现。
//!
//! 现方案：build.rs 在自己的进程里把 YAML 展开成**真正的 `FieldSpec` 值**
//! （不是源码文本），整表用 `bincode` 序列化成字节流写盘。`lib.rs` 用
//! `include_bytes!` 把这坨字节原样嵌入二进制（rustc 处理 `include_bytes!`
//! 只是把文件内容拷进数据段，不生成任何逐条目的代码，跟条目数无关），
//! 运行时反序列化一次装进 `HashMap`。字典规模再涨，编译期开销和栈帧大小
//! 都不会变——只有反序列化那一次遍历的时间和最终 `HashMap` 占用的堆内存
//! 会线性增长，这是数据量增长本该付出的、唯一合理的代价，不会有栈溢出
//! 或编译期爆炸的问题。
//!
//! ## `FieldSpec` 类型定义从哪来
//!
//! build.rs 用 `include!("src/types.rs")` 把 `Encoding`/`FieldSpec`/
//! `NamedField` 等类型定义原样再编译一份到 build.rs 自己的编译单元里。
//! 这份编译产物只在构建期跑一次、生成 `di_table.bin` 后就丢弃，不会进最终
//! 产物；好处是不需要为了让 build.rs 和 crate 共享类型定义而拆一个额外的
//! 子 crate（build.rs 没法直接依赖还没编译出来的 `spec-engine` 自身）。
//! 唯一的约束：`src/types.rs` 里这几个类型必须 derive `Serialize`/
//! `Deserialize`，且不能引用 `spec-engine` crate 内其它模块的东西（它本来就
//! 没有依赖，天然满足）。
//!
//! ## 展开规则（未变，仍对应设计文档）
//!
//! - `template_ref` / 直接内联的 `fields:`：编译期完全展开，运行时只剩 `Container`；
//! - `di_sequence`：两遍扫描——第一遍把所有 `data_items` 顶层条目建成
//!   `(id, protocol, region) -> 原始定义` 映射表，第二遍再展开
//!   `di_sequence.items` 引用；
//! - `repeat`：`element` 内部如果是 `template_ref`，同样在这里展开好；
//! - `switch`：`cases`/`default` 的值要么是一个模板名，要么是内置编码名；
//! - `dict_ref`：引用目标编译期未知，原样保留为运行时节点；
//! - 容器内子字段的"双重身份"：凡是 `id` 能解析成合法十六进制数的字段，
//!   除了正常内联进父容器，还额外注册一份。
//!
//! ## 协议（protocol）
//!
//! 字典文件必须放在 `schema/<protocol名>/*.yaml`——子目录名就是该文件里
//! 所有顶层条目的默认协议，条目也可以用 `protocol:` 字段显式覆盖（但正常
//! 不需要，一个文件通常只属于一个协议）。**协议维度是硬边界，不参与任何
//! 回退**：不同协议之间即使 DI 号相同，也是完全独立、互不相干的两份定义,
//! 查不到指定协议下的定义直接就是"未知 DI"，绝不会去匹配另一个协议里
//! 同样数值的 DI——这跟下面的 region 维度是相反的语义，region 允许回退是
//! 因为"通用定义兜底"是安全的，protocol 之间如果也允许回退，会出现最危险
//! 的一种bug：报文明明是协议 B 的，却悄悄按协议 A 的字典解出一个看起来
//! 正常但其实错误的值，比直接报"未知 DI"更难排查，所以必须是硬边界。
//!
//! ## 省份（region）
//!
//! 同一个 DI 码在同一个协议下、不同省份/局方可能有不同定义，多数 DI 是
//! 通用的，字典里不用每个省份都抄一遍。字典条目可以带一个 `region:` 字段，
//! 不写就落在 `"default"`（通用）桶里；同一个 `id` 在同一个 `protocol` 下
//! 允许出现多次，只要 `region` 不同就不算冲突。
//!
//! `region` 由顶层 `data_items` 条目自己的 `region:` 字段决定，`protocol`
//! 由它所在的子目录决定（或条目自己的 `protocol:` 覆盖）；容器内的子字段
//! 默认继承父级的 protocol/region（不用逐层重复写），除非子字段自己也显式
//! 写了对应字段覆盖。`gen_named_field`/`gen_field` 及其所有递归的子函数都
//! 带着 `protocol: &str, region: &str` 两个参数一路往下传，跟运行时
//! `parse_field` 一路带着同样两个参数递归是同一个思路。

#![allow(dead_code)]

// 注意：Deserialize 和 HashMap 不在这里 use——它们由下面
// `include!("src/types.rs")` 里的 `use serde::{Deserialize, Serialize};` /
// `use std::collections::HashMap;` 提供。Rust 不允许同一符号在同一模块
// 作用域被 use 两次（即使指向同一个 item 也算 E0252），所以两处只能留一份；
// 因为 include 的内容摆在下面，两个 use 语句本身在模块里是否按文本顺序
// 出现无所谓（同一模块内 use 不要求声明顺序在使用点之前）。
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// 通用（跨省份）定义落在这个桶里。跟 `src/parser.rs` 里的同名常量必须保持
/// 一致——这边生成的表用它做 key，那边查表也用它做 fallback，两处如果不
/// 一致，`default` 定义会在运行时永远查不到。
const DEFAULT_REGION: &str = "南网";

// ---------------------------------------------------------------------------
// 运行时类型定义（原样复用 src/types.rs，见文件头注释说明）
// ---------------------------------------------------------------------------

include!("src/types.rs");
include!("src/repeat.rs");

// ---------------------------------------------------------------------------
// 反映 YAML 结构的原始（未展开）AST
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone, Default)]
struct RawBit {
    range: (u8, u8),
    name: String,
    #[serde(rename = "enum", default)]
    enum_map: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct RawBitPattern {
    #[serde(default)]
    start_bit: Option<usize>,
    #[serde(default)]
    count: Option<usize>,
    #[serde(default)]
    bit_order: Option<String>, // "lsb" or "msb"
    #[serde(default)]
    name_template: Option<String>,
    #[serde(default)]
    byte_mask: Option<String>, // hex string like "0000FF00"
    #[serde(rename = "enum", default)]
    enum_map: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
enum RawCaseTarget {
    Name(String),
    Field(Box<RawField>),
}

// `extended` 已弃用，使用 `candidate_ids` 在编译期注册候选 id

#[derive(Debug, Deserialize, Clone, Default)]
struct RawCandidate {
    #[serde(default)]
    count: Option<usize>,
    #[serde(default)]
    count_ref: Option<String>,
    #[serde(default)]
    id_expr: Option<String>,
    #[serde(default)]
    name_template: Option<String>,
    /// 内嵌的 element 定义，放在 `candidate_ids.element:` 下
    #[serde(default)]
    element: Option<Box<RawField>>,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct RawField {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    ref_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(rename = "type", default)]
    ty: Option<String>,
    #[serde(default)]
    length: Option<serde_yaml::Value>,
    #[serde(default)]
    length_ref: Option<String>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    decimal: Option<u8>,
    #[serde(rename = "enum", default)]
    enum_map: Option<HashMap<String, String>>,
    #[serde(default)]
    endian: Option<String>,
    #[serde(default)]
    signed: Option<bool>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    bits: Option<Vec<RawBit>>,
    #[serde(default)]
    bitpattern: Option<RawBitPattern>,
    #[serde(default)]
    on: Option<String>,
    #[serde(default)]
    cases: Option<HashMap<String, RawCaseTarget>>,
    #[serde(default)]
    default: Option<RawCaseTarget>,
    #[serde(default)]
    count_ref: Option<String>,
    #[serde(default)]
    count: Option<usize>,
    #[serde(default)]
    name_template: Option<String>,
    #[serde(default)]
    element: Option<Box<RawField>>,
    #[serde(rename = "ref", default)]
    ref_: Option<String>,
    #[serde(default)]
    id_expr: Option<String>,
    /// 编译期要额外注册的候选 DI id（结构化），格式示例：
    /// candidate_ids:
    ///   count: 63
    ///   id_expr: "0x00030100 + index0*0x0100"
    ///   name_template: "费率{index}"
    #[serde(default)]
    candidate_ids: Option<RawCandidate>,
    #[serde(default)]
    protocol: Option<String>,
    /// `type: external` 专用——内嵌报文该用哪个外部协议解析（对应
    /// `registry.rs` 里注册的名字，如 `dlt645-2007`）。故意跟上面的
    /// `protocol` 分开命名：上面那个 `protocol` 是"这个DI属于哪个协议"的
    /// 分类维度（来自目录名/顶层条目），这个是"内嵌报文的协议"，两者语义
    /// 完全不同，撞同一个关键字会导致 `effective_protocol` 在 external
    /// 字段上把分类维度的 protocol 意外覆盖掉。
    #[serde(default)]
    external_protocol: Option<String>,
    #[serde(default)]
    items: Option<Vec<String>>,
    #[serde(default)]
    fields: Option<Vec<RawField>>,
    #[serde(default)]
    handler: Option<String>,
    /// 省份/局方覆盖。不写就是 `None`，构建期按 `DEFAULT_REGION` 处理。
    #[serde(default)]
    region: Option<Vec<String>>,
    /// 报文方向覆盖（单值，不是数组——一个条目要么跟方向无关，要么精确
    /// 属于某一个方向，不存在"属于多个方向"的中间态）。不写就是 `None`，
    /// 代表"跟方向无关，两个方向通用"；查表时只有 `dir` 显式写了的条目才
    /// 会要求精确匹配，没写的条目对任何方向的查询都是候选。
    #[serde(default)]
    dir: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct RawTemplate {
    #[serde(default)]
    #[allow(dead_code)]
    length: Option<usize>,
    fields: Vec<RawField>,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct RawDict {
    #[serde(default)]
    templates: HashMap<String, RawTemplate>,
    #[serde(default)]
    data_items: Vec<RawField>,
}

// ---------------------------------------------------------------------------
// 展开 / 数据构造上下文
// ---------------------------------------------------------------------------

struct BuildCtx {
    /// 模板是纯结构定义（不挂 DI 号），协议之间共享同一个模板池——不同协议
    /// 复用同一个 IP_WITH_PORT / PHONE_BCD 这类通用结构是合理的，模板本身
    /// 不需要 protocol 维度。
    templates: HashMap<String, RawTemplate>,
    /// 顶层 data_items 里 (id, protocol, region, dir) -> 原始定义，供
    /// di_sequence 解析引用。同一个 id 在不同 protocol/region 下可以有不同
    /// 定义，key 必须把它们都带上。
    di_raw_map: HashMap<(String, String, String, Option<String>), RawField>,
    /// (DI数值, protocol, region, dir, 该节点展开好的 NamedField 值)——不再是
    /// 源码字符串，是真正可以直接 bincode 序列化的数据。
    registrations: Vec<(u32, String, String, Option<String>, NamedField)>,
}

#[derive(Clone)]
struct BuildScope {
    scopes: Vec<HashSet<String>>,
    ref_scopes: Vec<HashSet<String>>,
}

impl BuildScope {
    fn new() -> Self {
        Self {
            scopes: vec![HashSet::new()],
            ref_scopes: vec![HashSet::new()],
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashSet::new());
        self.ref_scopes.push(HashSet::new());
    }

    fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
            self.ref_scopes.pop();
        }
    }

    fn insert(&mut self, id: String) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(id);
        }
    }

    fn insert_ref(&mut self, ref_id: String) {
        if let Some(scope) = self.ref_scopes.last_mut() {
            scope.insert(ref_id);
        }
    }

    fn contains(&self, id: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(id))
    }

    fn contains_ref_id(&self, id: &str) -> bool {
        self.ref_scopes.iter().rev().any(|scope| scope.contains(id))
    }

    fn insert_field(&mut self, rf: &RawField) {
        if let Some(id) = &rf.id {
            self.insert(id.clone());
        }
        if let Some(ref_id) = &rf.ref_id {
            self.insert(ref_id.clone());
            self.insert_ref(ref_id.clone());
        }
    }
}

fn main() {
    let schema_dir = "schema";
    println!("cargo:rerun-if-changed={}", schema_dir);

    let mut combined = RawDict::default();

    let mut entries: Vec<PathBuf> = fs::read_dir(schema_dir)
        .unwrap_or_else(|e| panic!("无法读取 {} 目录: {}", schema_dir, e))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();

    // schema/ 根目录下不应该直接躺着 .yaml 文件——协议归属必须明确，
    // 不能靠猜。发现了就直接报错，指引挪进协议子目录。
    let stray: Vec<_> = entries
        .iter()
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .map(|e| e == "yaml" || e == "yml")
                    .unwrap_or(false)
        })
        .collect();
    if !stray.is_empty() {
        panic!(
            "{} 目录下发现未归类的 .yaml 文件: {:?}\n\
             字典文件必须放在协议子目录下，例如 schema/csg1209022/xxx.yaml，\n\
             子目录名就是该文件里未显式指定 protocol 的顶层条目的默认协议。",
            schema_dir, stray
        );
    }

    let protocol_dirs: Vec<_> = entries.iter().filter(|p| p.is_dir()).collect();
    if protocol_dirs.is_empty() {
        panic!(
            "{} 目录下没有找到任何协议子目录（例如 schema/csg1209022/）",
            schema_dir
        );
    }

    for proto_dir in &protocol_dirs {
        let protocol_name = proto_dir
            .file_name()
            .expect("协议目录必须有合法目录名")
            .to_string_lossy()
            .to_string();

        let mut paths: Vec<_> = fs::read_dir(proto_dir)
            .unwrap_or_else(|e| panic!("无法读取 {} 目录: {}", proto_dir.display(), e))
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .map(|ext| ext == "yaml" || ext == "yml")
                    .unwrap_or(false)
            })
            .collect();
        paths.sort();

        for path in &paths {
            println!("cargo:rerun-if-changed={}", path.display());
            let content = fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("读取 {} 失败: {}", path.display(), e));
            let mut dict: RawDict = serde_yaml::from_str(&content)
                .unwrap_or_else(|e| panic!("解析 {} 失败: {}", path.display(), e));

            // 顶层条目没有显式 protocol 的，用它所在的协议子目录名兜底。
            // 之后这个字段就一直是 Some(..)，不需要再有"默认协议常量"这种
            // 全局兜底概念——协议归属从一开始就是确定的。
            for rf in &mut dict.data_items {
                if rf.protocol.is_none() {
                    rf.protocol = Some(protocol_name.clone());
                }
            }

            combined.templates.extend(dict.templates);
            combined.data_items.extend(dict.data_items);
        }
    }

    // 第一遍：建立 (id, protocol, region, dir) -> 原始定义 映射
    let mut di_raw_map = HashMap::new();
    for rf in &combined.data_items {
        if let Some(id) = &rf.id {
            let protocol = rf
                .protocol
                .clone()
                .expect("顶层条目的 protocol 到这里必须已经被目录名兜底过");
            let regions = rf
                .region
                .clone()
                .unwrap_or_else(|| vec![DEFAULT_REGION.to_string()]);
            let dir = rf.dir.clone();
            for region in regions {
                di_raw_map.insert(
                    (id.clone(), protocol.clone(), region, dir.clone()),
                    rf.clone(),
                );
            }
        }
    }

    let mut ctx = BuildCtx {
        templates: combined.templates,
        di_raw_map,
        registrations: Vec::new(),
    };

    // 第二遍：展开每个顶层 data_item，直接构造 FieldSpec 值（不再是源码字符串）
    for rf in &combined.data_items {
        let top_protocol = rf
            .protocol
            .clone()
            .expect("顶层条目的 protocol 到这里必须已经被目录名兜底过");
        let top_regions = rf
            .region
            .clone()
            .unwrap_or_else(|| vec![DEFAULT_REGION.to_string()]);
        let top_dir = rf.dir.clone();
        for top_region in top_regions {
            let mut scope = BuildScope::new();
            // debug: dump child fields YAML for diagnosis
            let _ = gen_named_field(
                rf,
                &top_protocol,
                &top_region,
                top_dir.as_deref(),
                &mut ctx,
                &mut scope,
            );
        }
    }

    // 把 registrations 收拢成最终要嵌进二进制里的那张表。
    let mut table: HashMap<(String, u32, String, Option<String>), NamedField> = HashMap::new();
    for (id, protocol, region, dir, named_field) in ctx.registrations {
        table.insert((protocol, id, region, dir), named_field);
    }

    // 关键的一步：不再生成 Rust 源码，而是把整棵 FieldSpec 树序列化成字节流。
    // rustc 后面只需要处理 `include_bytes!` 引入的一段常量数据，不会再对
    // 字典规模产生任何超线性的编译期开销，也不存在"单个巨型函数栈帧"这回事。
    let bytes = bincode::serialize(&table).unwrap_or_else(|e| panic!("序列化 DI 表失败: {}", e));

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR 未设置");
    let dest = Path::new(&out_dir).join("di_table.bin");
    fs::write(&dest, &bytes).unwrap_or_else(|e| panic!("写入 {} 失败: {}", dest.display(), e));

    println!(
        "cargo:warning=DI 字典构建完成：{} 条目，序列化后 {} 字节",
        table.len(),
        bytes.len()
    );
}

// ---------------------------------------------------------------------------
// 展开：从 RawField 递归构造出真正的 FieldSpec / NamedField 值
// ---------------------------------------------------------------------------

/// 一个字段的"生效 protocol/region"：自己显式写了就用自己的，否则继承父级
/// 传下来的值。顶层条目的父级值就是它自己算出来的 protocol/region（见
/// `main` 里的调用），所以这里的覆盖逻辑对顶层条目和嵌套子字段是同一套
/// 规则。
fn effective_protocol<'a>(rf: &'a RawField, parent_protocol: &'a str) -> &'a str {
    rf.protocol.as_deref().unwrap_or(parent_protocol)
}

fn effective_region<'a>(rf: &'a RawField, parent_region: &'a str) -> &'a str {
    rf.region
        .as_ref()
        .and_then(|regions| regions.first().map(|s| s.as_str()))
        .unwrap_or(parent_region)
}

fn effective_dir<'a>(rf: &'a RawField, parent_dir: Option<&'a str>) -> Option<&'a str> {
    rf.dir.as_deref().or(parent_dir)
}

fn field_label(rf: &RawField) -> String {
    rf.name
        .clone()
        .or_else(|| rf.id.clone())
        .unwrap_or_else(|| "<unnamed field>".to_string())
}

fn gen_named_field(
    rf: &RawField,
    protocol: &str,
    region: &str,
    dir: Option<&str>,
    ctx: &mut BuildCtx,
    scope: &mut BuildScope,
) -> NamedField {
    validate_field_refs(rf, scope);
    let protocol = effective_protocol(rf, protocol).to_string();
    let region = effective_region(rf, region).to_string();
    let dir = effective_dir(rf, dir).map(|s| s.to_string());
    let spec = gen_field(rf, &protocol, &region, dir.as_deref(), ctx, scope);
    let name = rf.name.clone().unwrap_or_default();

    if let Some(id_str) = &rf.id {
        if let Ok(id_num) = u32::from_str_radix(id_str, 16) {
            if let Some((_, _, _, _, existing)) =
                ctx.registrations.iter().find(|(id, p, r, d, _)| {
                    *id == id_num && p == &protocol && r == &region && d == &dir
                })
            {
                // 直接比较结构化的 FieldSpec 值（PartialEq 是派生的，递归
                // 结构相等），而不是像旧方案那样比较生成的源码文本——旧方案
                // 里 enum_map 这类 HashMap 字段生成代码时迭代顺序不固定，
                // 同一份逻辑定义两次可能生成不同顺序的 insert 语句，导致
                // 误报"重复定义不一致"；值比较不受这个影响，更准确。
                if existing.spec != spec {
                    panic!(
                        "DI 0x{:08X}（protocol={:?}, region={:?}, dir={:?}）被重复定义为两种\n\
                         不同的结构：请检查字典里是否有同一个 id 在同一个 (protocol, region, dir)\n\
                         组合下出现了不一致的字段定义，必须保证同一个组合全局唯一或定义完全\n\
                         一致。如果是想给不同协议/省份/方向各自定义，请确认对应字段确实不同。",
                        id_num, protocol, region, dir
                    );
                }
            } else {
                ctx.registrations.push((
                    id_num,
                    protocol.clone(),
                    region.clone(),
                    dir.clone(),
                    NamedField {
                        id: rf.id.clone(),
                        ref_id: rf.ref_id.clone(),
                        name: name.clone(),
                        spec: spec.clone(),
                    },
                ));
            }
        }
    }

    // candidate_ids 相关的编译期生成由子字段（fields 中的 candidate_ids 条目）处理，
    // 不在这里重复处理。

    scope.insert_field(rf);
    // candidate_ids 的注册已在上面完成（通过 gen_named_field），无需额外向作用域直接插入原始字符串
    NamedField {
        id: rf.id.clone(),
        ref_id: rf.ref_id.clone(),
        name,
        spec,
    }
}

fn gen_field(
    rf: &RawField,
    protocol: &str,
    region: &str,
    dir: Option<&str>,
    ctx: &mut BuildCtx,
    scope: &mut BuildScope,
) -> FieldSpec {
    // 支持作为子字段出现的 `candidate_ids`：结构化包含 count/id_expr/name_template
    if let Some(cand) = &rf.candidate_ids {
        let count = cand
            .count
            .unwrap_or_else(|| panic!("candidate_ids 在字段 {} 中缺少 count", field_label(rf)));
        let id_expr = cand
            .id_expr
            .as_ref()
            .unwrap_or_else(|| panic!("candidate_ids 在字段 {} 中缺少 id_expr", field_label(rf)));
        let element_rf = cand
            .element
            .as_ref()
            .unwrap_or_else(|| panic!("candidate_ids 在字段 {} 中缺少 element", field_label(rf)));

        // 无条件按 count（编译期上限）注册全部候选，供以后单独寻址；
        // 跟下面本节点自己在父容器里怎么解析完全独立。
        for idx in 0..count {
            let generated_id = format_id_expr(id_expr, idx)
                .unwrap_or_else(|e| panic!("id_expr 解析失败: {:?}", e));
            let mut repeated_rf = (**element_rf).clone();
            repeated_rf.id = Some(generated_id.clone());
            repeated_rf.name = Some(format_repeat_name(
                cand.name_template.as_deref(),
                Some(&generated_id),
                idx,
                count,
            ));
            gen_named_field(&repeated_rf, protocol, region, dir, ctx, scope); // 仅登记，返回值丢弃
        }

        return match &cand.count_ref {
            // 变长：这次报文实际读几组，交给运行时 count_ref，
            // 跟上面已经注册满 count 个候选是两回事——一个管"能查到多少"，一个管"这次读多少"
            Some(count_ref) => FieldSpec::Repeat {
                count_ref: count_ref.clone(),
                element: Box::new(gen_field(element_rf, protocol, region, dir, ctx, scope)),
                name_template: cand.name_template.clone(),
                id_expr: Some(id_expr.clone()),
            },
            // 定长：没有 count_ref，说明协议里这组就是固定 count 个，
            // 编译期注册的这批本身就是实际要解析的全部，直接复用
            None => {
                let mut named = Vec::with_capacity(count);
                for idx in 0..count {
                    let generated_id = format_id_expr(id_expr, idx)
                        .unwrap_or_else(|e| panic!("id_expr 解析失败: {:?}", e));
                    let mut repeated_rf = (**element_rf).clone();
                    repeated_rf.id = Some(generated_id.clone());
                    repeated_rf.name = Some(format_repeat_name(
                        cand.name_template.as_deref(),
                        Some(&generated_id),
                        idx,
                        count,
                    ));
                    named.push(gen_named_field(
                        &repeated_rf,
                        protocol,
                        region,
                        dir,
                        ctx,
                        scope,
                    ));
                }
                FieldSpec::Container(named)
            }
        };
    }

    // `extended` 已弃用；如需在编译期注册候选 id，请使用 `candidate_ids`。

    let ty = rf.ty.clone().unwrap_or_else(|| {
        if rf.fields.is_some() {
            "container".to_string()
        } else {
            panic!(
                "字段 name={:?} id={:?} 既没有 type 也没有 fields，无法确定类型，raw={:?}",
                rf.name, rf.id, rf
            )
        }
    });

    match ty.as_str() {
        "bcd" | "bin" | "ascii" | "hex" | "time" => gen_fixed(rf, &ty),
        "bitfield" => gen_bitfield(rf),
        "bitpattern" => gen_bitpattern(rf),
        "switch" => gen_switch(rf, protocol, region, dir, ctx, scope),
        "repeat" => gen_repeat(rf, protocol, region, dir, ctx, scope),
        "template_ref" => {
            let tname = rf
                .ref_
                .clone()
                .unwrap_or_else(|| panic!("template_ref 字段 {:?} 缺少 ref", rf.name));
            gen_template_ref(&tname, protocol, region, dir, ctx, scope)
        }
        "external" => gen_external(rf),
        "di_sequence" => gen_di_sequence(rf, protocol, region, dir, ctx, scope),
        "dict_ref" => gen_dict_ref(rf),
        "custom" => gen_custom(rf),
        "container" => gen_container_from_fields(
            rf.fields
                .as_ref()
                .unwrap_or_else(|| panic!("container 字段 {:?} 缺少 fields", rf.name)),
            protocol,
            region,
            dir,
            ctx,
            scope,
        ),
        other => panic!("未知字段类型: {}（字段: {:?}）", other, rf.name),
    }
}

fn get_len(rf: &RawField) -> usize {
    match &rf.length {
        Some(serde_yaml::Value::Number(n)) => n
            .as_u64()
            .unwrap_or_else(|| panic!("字段 {:?} 的 length 必须是非负整数", rf.name))
            as usize,
        other => panic!("字段 {:?} 需要一个整数 length，实际是 {:?}", rf.name, other),
    }
}

fn resolve_field_length(rf: &RawField) -> FieldLength {
    if let Some(length_ref) = &rf.length_ref {
        return FieldLength::Ref(length_ref.clone());
    }
    if let Some(serde_yaml::Value::Number(_)) = &rf.length {
        let len = get_len(rf);
        return FieldLength::Fixed(len);
    }
    panic!(
        "字段 {:?} 需要一个整数 length 或 length_ref，实际是 {:?}",
        rf.name, rf.length
    );
}

fn resolve_endian(rf: &RawField) -> Endian {
    match rf.endian.as_deref() {
        Some("big") => Endian::Big,
        Some("little") | None => Endian::Little,
        Some(other) => panic!("未知 endian: {}（字段: {:?}）", other, rf.name),
    }
}

fn validate_id_ref(rf: &RawField, ref_name: &str, kind: &str, scope: &BuildScope) {
    if !scope.contains_ref_id(ref_name) {
        panic!(
            "字段 {} 的 {} 引用了未知 ref_id: {:?}，引用必须使用字段 ref_id",
            field_label(rf),
            kind,
            ref_name
        );
    }
}

fn validate_field_refs(rf: &RawField, scope: &BuildScope) {
    if let Some(length_ref) = &rf.length_ref {
        validate_id_ref(rf, length_ref, "length_ref", scope);
    }
    if let Some(count_ref) = &rf.count_ref {
        validate_id_ref(rf, count_ref, "count_ref", scope);
    }
    if let Some(on) = &rf.on {
        if on != "$remaining" && on != "$len" && on != "$length" {
            validate_id_ref(rf, on, "switch.on", scope);
        }
    }
    if let Some(ref_name) = &rf.ref_ {
        if rf.ty.as_deref() == Some("dict_ref") {
            validate_id_ref(rf, ref_name, "dict_ref.ref", scope);
        }
    }
    if let Some(serde_yaml::Value::String(s)) = &rf.length {
        if rf.ty.as_deref() == Some("external") && s != "remaining" {
            validate_id_ref(rf, s, "external length", scope);
        }
    }
    if let Some(cand) = &rf.candidate_ids {
        if let Some(count_ref) = &cand.count_ref {
            validate_id_ref(rf, count_ref, "candidate_ids.count_ref", scope);
        }
        if let Some(element_rf) = &cand.element {
            validate_field_refs(element_rf, scope);
        }
    }
}

fn gen_fixed(rf: &RawField, ty: &str) -> FieldSpec {
    let length = resolve_field_length(rf);
    let signed = rf.signed.unwrap_or(false);
    let encoding = match ty {
        "bin" => Encoding::Bin {
            endian: resolve_endian(rf),
            signed,
        },
        "bcd" => {
            let decimals = rf.decimal.unwrap_or(0);
            let endian = match rf.endian.as_deref() {
                Some("big") => Some(Endian::Big),
                Some("little") => Some(Endian::Little),
                None => None,
                Some(other) => panic!("未知 endian: {}（字段: {:?}）", other, rf.name),
            };
            Encoding::Bcd {
                decimals,
                signed,
                endian,
            }
        }
        "ascii" => Encoding::Ascii,
        "hex" => Encoding::Hex,
        "time" => {
            let fmt = rf
                .format
                .clone()
                .unwrap_or_else(|| panic!("time 字段 {:?} 缺少 format", rf.name));
            Encoding::Time { format: fmt }
        }
        _ => unreachable!(),
    };
    FieldSpec::Fixed {
        encoding,
        length,
        unit: rf.unit.clone(),
        enum_map: rf.enum_map.clone(),
    }
}

fn gen_bitfield(rf: &RawField) -> FieldSpec {
    let length = get_len(rf);
    let bits = rf
        .bits
        .as_ref()
        .unwrap_or_else(|| panic!("bitfield 字段 {:?} 缺少 bits", rf.name));
    let bits: Vec<BitSpec> = bits
        .iter()
        .map(|b| BitSpec {
            range: b.range,
            name: b.name.clone(),
            enum_map: b.enum_map.clone(),
        })
        .collect();
    FieldSpec::BitField { length, bits }
}

fn gen_bitpattern(rf: &RawField) -> FieldSpec {
    let length = get_len(rf);
    let bp = rf
        .bitpattern
        .as_ref()
        .unwrap_or_else(|| panic!("bitpattern 字段 {:?} 缺少 bitpattern 配置", rf.name));
    let bit_order = bp.bit_order.as_deref().unwrap_or("lsb");
    let mut bits_out: Vec<BitSpec> = Vec::new();
    if let Some(mask_hex) = &bp.byte_mask {
        // parse hex string into bytes
        let s = mask_hex.trim();
        let mut bytes: Vec<u8> = Vec::new();
        let mut i = 0;
        while i < s.len() {
            let hi = u8::from_str_radix(&s[i..i + 2], 16)
                .unwrap_or_else(|_| panic!("bitpattern byte_mask 不是合法的 hex: {}", s));
            bytes.push(hi);
            i += 2;
        }
        for (byte_idx, &b) in bytes.iter().enumerate() {
            for bit_in_byte in 0..8 {
                let mask_bit = if bit_order == "lsb" {
                    (b >> bit_in_byte) & 1
                } else {
                    (b >> (7 - bit_in_byte)) & 1
                };
                if mask_bit != 0 {
                    let bit_index = (byte_idx * 8) + bit_in_byte;
                    let name = if let Some(tmpl) = &bp.name_template {
                        tmpl.replace("{index0}", &bit_index.to_string())
                            .replace("{index}", &(bit_index + 1).to_string())
                    } else {
                        format!("bit{}", bit_index + 1)
                    };
                    bits_out.push(BitSpec {
                        range: (bit_index as u8, bit_index as u8),
                        name,
                        enum_map: bp.enum_map.clone(),
                    });
                }
            }
        }
    } else if let Some(count) = bp.count {
        let start = bp.start_bit.unwrap_or(0);
        for k in 0..count {
            let bit_index = start + k;
            let name = if let Some(tmpl) = &bp.name_template {
                tmpl.replace("{index0}", &k.to_string())
                    .replace("{index}", &(k + 1).to_string())
            } else {
                format!("bit{}", k + 1)
            };
            bits_out.push(BitSpec {
                range: (bit_index as u8, bit_index as u8),
                name,
                enum_map: bp.enum_map.clone(),
            });
        }
    } else {
        panic!(
            "bitpattern 字段 {:?} 需要 byte_mask 或 start_bit+count",
            rf.name
        );
    }
    // computed length in bytes: use declared length for safety
    FieldSpec::BitField {
        length,
        bits: bits_out,
    }
}

fn resolve_switch_target(
    target: &RawCaseTarget,
    switch_len: Option<usize>,
    protocol: &str,
    region: &str,
    dir: Option<&str>,
    ctx: &mut BuildCtx,
    scope: &mut BuildScope,
) -> FieldSpec {
    match target {
        RawCaseTarget::Field(field) => gen_field(field, protocol, region, dir, ctx, scope),
        RawCaseTarget::Name(name) => {
            if ctx.templates.contains_key(name) {
                gen_template_ref(name, protocol, region, dir, ctx, scope)
            } else if matches!(name.as_str(), "bcd" | "bin" | "ascii" | "hex") {
                let len = switch_len.unwrap_or_else(|| {
                    panic!(
                        "switch 字段 {:?} 使用内置类型 {:?} 时需要父节点 length",
                        name, name
                    )
                });
                let fake = RawField {
                    length: Some(serde_yaml::Value::Number(serde_yaml::Number::from(
                        len as u64,
                    ))),
                    ..Default::default()
                };
                gen_fixed(&fake, name)
            } else {
                panic!(
                    "switch 的 case/default 值 {:?} 既不是模板名也不是内置编码名",
                    name
                )
            }
        }
    }
}

fn gen_switch(
    rf: &RawField,
    protocol: &str,
    region: &str,
    dir: Option<&str>,
    ctx: &mut BuildCtx,
    scope: &mut BuildScope,
) -> FieldSpec {
    let on = rf
        .on
        .clone()
        .unwrap_or_else(|| panic!("switch 字段 {:?} 缺少 on", rf.name));
    let switch_len = rf.length.as_ref().and_then(|len| match len {
        serde_yaml::Value::Number(n) => Some(
            n.as_u64()
                .unwrap_or_else(|| panic!("switch 字段 {:?} 的 length 必须是非负整数", rf.name))
                as usize,
        ),
        _ => None,
    });
    let mut cases = rf
        .cases
        .clone()
        .unwrap_or_else(|| panic!("switch 字段 {:?} 缺少 cases", rf.name));
    let default_target = rf.default.clone().or_else(|| cases.remove("default"));

    let mut cases_map: HashMap<String, Box<FieldSpec>> = HashMap::new();
    for (key, target) in &cases {
        let target_spec =
            resolve_switch_target(target, switch_len, protocol, region, dir, ctx, scope);
        cases_map.insert(key.clone(), Box::new(target_spec));
    }
    let default = default_target.map(|target| {
        Box::new(resolve_switch_target(
            &target, switch_len, protocol, region, dir, ctx, scope,
        ))
    });

    FieldSpec::Switch {
        on,
        cases: cases_map,
        default,
    }
}

// note: repeat + id_expr 的编译期自动注册已移除。使用者可在顶层条目中
// 通过 `candidate_ids:` 明确列出要注册的 id，或保留 runtime 的 `repeat` + `count_ref`
// 以在运行时按报文中的 count 解析。

fn gen_repeat(
    rf: &RawField,
    protocol: &str,
    region: &str,
    dir: Option<&str>,
    ctx: &mut BuildCtx,
    scope: &mut BuildScope,
) -> FieldSpec {
    let element_rf = rf
        .element
        .as_ref()
        .unwrap_or_else(|| panic!("repeat 字段 {:?} 缺少 element", rf.name));
    let count = rf.count;
    let id_expr = rf.id_expr.clone();
    if count.is_some() && id_expr.is_some() && rf.count_ref.is_none() {
        panic!(
            "repeat 字段 {:?} 同时含有 count 和 id_expr，但未提供 count_ref。\n请在顶层条目使用 `candidate_ids` 注册编译期候选 id，或改为使用 `repeat` + `count_ref` 在运行时按报文解析",
            rf.name
        );
    }

    let element = gen_field(element_rf, protocol, region, dir, ctx, scope);
    if let (Some(count), Some(_id_expr)) = (count, id_expr.as_ref()) {
        // 不再在编译期自动基于 id_expr 展开注册（请使用 candidate_ids）
        if count == 0 {
            return FieldSpec::Container(Vec::new());
        }
    }
    if let Some(count) = count {
        if count == 0 && rf.count_ref.is_none() {
            return FieldSpec::Container(Vec::new());
        }
    }
    let count_ref = rf
        .count_ref
        .clone()
        .unwrap_or_else(|| panic!("repeat 字段 {:?} 缺少 count_ref", rf.name));
    let name_template = rf.name_template.clone();
    FieldSpec::Repeat {
        count_ref,
        element: Box::new(element),
        name_template,
        id_expr,
    }
}

// `extended` 功能已被移除。若需要在编译期注册一组候选 id，
// 请在对应条目使用 `candidate_ids: ["00000100", "00000200", ...]`。

fn gen_template_ref(
    tname: &str,
    protocol: &str,
    region: &str,
    dir: Option<&str>,
    ctx: &mut BuildCtx,
    scope: &mut BuildScope,
) -> FieldSpec {
    let template = ctx
        .templates
        .get(tname)
        .unwrap_or_else(|| panic!("找不到模板: {}", tname))
        .clone();
    gen_container_from_fields(&template.fields, protocol, region, dir, ctx, scope)
}

fn gen_container_from_fields(
    fields: &[RawField],
    protocol: &str,
    region: &str,
    dir: Option<&str>,
    ctx: &mut BuildCtx,
    scope: &mut BuildScope,
) -> FieldSpec {
    scope.push_scope();
    let mut named = Vec::new();
    for f in fields {
        let nf = gen_named_field(f, protocol, region, dir, ctx, scope);
        named.push(nf);
    }
    scope.pop_scope();
    FieldSpec::Container(named)
}

fn gen_external(rf: &RawField) -> FieldSpec {
    let protocol = rf
        .external_protocol
        .clone()
        .unwrap_or_else(|| panic!("external 字段 {:?} 缺少 external_protocol", rf.name));
    let length = match &rf.length {
        Some(serde_yaml::Value::String(s)) if s == "remaining" => ExternalLength::Remaining,
        Some(serde_yaml::Value::String(s)) => ExternalLength::Ref(s.clone()),
        Some(serde_yaml::Value::Number(n)) => ExternalLength::Fixed(
            n.as_u64()
                .unwrap_or_else(|| panic!("external 字段 {:?} 的 length 数值非法", rf.name))
                as usize,
        ),
        _ => panic!(
            "external 字段 {:?} 需要 length: remaining | <引用字段名> | <数值>",
            rf.name
        ),
    };
    FieldSpec::External { protocol, length }
}

/// `di_sequence` 引用的目标 DI，按 `(di_str, 当前protocol, 当前region)` 查找，
/// 查不到再回退 `(di_str, 当前protocol, DEFAULT_REGION)`——跟运行时
/// `lookup_di` 的回退顺序完全一致：region 允许回退，protocol 绝不跨协议
/// 查找。di_sequence 本质上是"把另一个 DI 的定义原样内联进来"，应该内联
/// "当前协议、当前 region 下那个 DI 该有的样子"。
fn lookup_raw<'a>(
    di_raw_map: &'a HashMap<(String, String, String, Option<String>), RawField>,
    di_str: &str,
    protocol: &str,
    region: &str,
    dir: Option<&str>,
) -> &'a RawField {
    let dir_owned = dir.map(|s| s.to_string());
    if let Some(rf) = di_raw_map.get(&(
        di_str.to_string(),
        protocol.to_string(),
        region.to_string(),
        dir_owned.clone(),
    )) {
        return rf;
    }
    if dir_owned.is_some() {
        if let Some(rf) = di_raw_map.get(&(
            di_str.to_string(),
            protocol.to_string(),
            region.to_string(),
            None,
        )) {
            return rf;
        }
    }
    if region != DEFAULT_REGION {
        if let Some(rf) = di_raw_map.get(&(
            di_str.to_string(),
            protocol.to_string(),
            DEFAULT_REGION.to_string(),
            dir_owned.clone(),
        )) {
            return rf;
        }
        if let Some(rf) = di_raw_map.get(&(
            di_str.to_string(),
            protocol.to_string(),
            DEFAULT_REGION.to_string(),
            None,
        )) {
            return rf;
        }
    }
    panic!(
        "di_sequence 引用了未知 DI: {}（protocol={:?}, region={:?}, dir={:?}，且该\n\
         protocol 下所有 region/dir 回退组合都没有定义）",
        di_str, protocol, region, dir
    );
}

fn gen_di_sequence(
    rf: &RawField,
    protocol: &str,
    region: &str,
    dir: Option<&str>,
    ctx: &mut BuildCtx,
    scope: &mut BuildScope,
) -> FieldSpec {
    let items = rf
        .items
        .clone()
        .unwrap_or_else(|| panic!("di_sequence 字段 {:?} 缺少 items", rf.name));
    let mut named = Vec::with_capacity(items.len());
    for di_str in &items {
        let raw = lookup_raw(&ctx.di_raw_map, di_str, protocol, region, dir).clone();
        named.push(gen_named_field(&raw, protocol, region, dir, ctx, scope));
    }
    FieldSpec::Container(named)
}

fn gen_dict_ref(rf: &RawField) -> FieldSpec {
    let di_ref = rf
        .ref_
        .clone()
        .unwrap_or_else(|| panic!("dict_ref 字段 {:?} 缺少 ref", rf.name));
    FieldSpec::DictRef { di_ref }
}

fn gen_custom(rf: &RawField) -> FieldSpec {
    let handler = rf
        .handler
        .clone()
        .unwrap_or_else(|| panic!("custom 字段 {:?} 缺少 handler", rf.name));
    FieldSpec::Custom(handler)
}
