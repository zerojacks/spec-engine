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
    range: (usize, usize),
    name: String,
    #[serde(default)]
    ref_id: Option<String>,
    #[serde(rename = "enum", default)]
    enum_map: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct RawCandidate {
    #[serde(default)]
    count: Option<usize>,
    #[serde(default)]
    count_ref: Option<String>,
    #[serde(default)]
    count_expr: Option<String>,
    #[serde(default)]
    id_expr: Option<String>,
    #[serde(default)]
    name_template: Option<String>,
    /// 内嵌的 element 定义，放在 `candidate_ids.element:` 下
    #[serde(default)]
    element: Option<Box<RawField>>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
enum RawCaseTarget {
    Name(String),
    Field(Box<RawField>),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
enum RawFormat {
    String(String),
    Object(FormatObject),
}

#[derive(Debug, Deserialize, Clone, Default)]
struct FormatObject {
    #[serde(rename = "type")]
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    group_bytes: Option<usize>,
    #[serde(default)]
    separator: Option<String>,
    #[serde(default)]
    pad: Option<bool>,
    #[serde(default)]
    endian: Option<String>,
    #[serde(default)]
    order: Option<String>,
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
    group_bytes: Option<usize>,
    #[serde(default)]
    separator: Option<String>,
    #[serde(default)]
    pad: Option<bool>,
    #[serde(default)]
    lengthrule: Option<String>,
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
    format: Option<RawFormat>,
    #[serde(default)]
    time: Option<String>,
    #[serde(default)]
    bits: Option<Vec<RawBit>>,
    #[serde(default)]
    on: Option<String>,
    #[serde(default)]
    cases: Option<HashMap<String, RawCaseTarget>>,
    #[serde(default)]
    default: Option<RawCaseTarget>,
    #[serde(default)]
    count_ref: Option<String>,
    #[serde(default)]
    count_expr: Option<String>,
    #[serde(default)]
    bits_ref: Option<String>,
    #[serde(default)]
    bit_direction: Option<String>,
    #[serde(default)]
    iterate_order: Option<String>,
    #[serde(default)]
    count: Option<usize>,
    #[serde(default)]
    name_template: Option<String>,
    #[serde(default)]
    element: Option<Box<RawField>>,
    #[serde(rename = "ref", default)]
    ref_: Option<String>,
    #[serde(default)]
    dict_ref: Option<serde_yaml::Value>,
    #[serde(default)]
    template_ref: Option<String>,
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

#[derive(Debug, Deserialize, Clone)]
struct RawTemplate {
    id: String,
    #[serde(default)]
    protocol: Option<String>,
    #[serde(default)]
    region: Option<Vec<String>>,
    #[serde(default)]
    dir: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    length: Option<usize>,
    fields: Vec<RawField>,
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

#[derive(Debug, Deserialize, Clone, Default)]
struct RawDict {
    #[serde(default)]
    templates: Vec<RawTemplate>,
    #[serde(default)]
    data_items: Vec<RawField>,
}

fn collect_raw_ids(
    rf: &RawField,
    protocol: &str,
    region: &str,
    dir: &Option<String>,
    di_raw_map: &mut HashMap<(String, String, String, Option<String>), RawField>,
) {
    if let Some(id) = &rf.id {
        di_raw_map
            .entry((id.clone(), protocol.to_string(), region.to_string(), dir.clone()))
            .or_insert_with(|| rf.clone());
    }
    if let Some(fields) = &rf.fields {
        for field in fields {
            collect_raw_ids(field, protocol, region, dir, di_raw_map);
        }
    }
    if let Some(cand) = &rf.candidate_ids {
        if let Some(element_rf) = &cand.element {
            for idx in 0..cand.count.unwrap_or(0) {
                if let Some(id_expr) = &cand.id_expr {
                    let generated_id = format_id_expr(id_expr, idx)
                        .unwrap_or_else(|e| panic!("id_expr 解析失败: {:?}", e));
                    let mut repeated_rf = (**element_rf).clone();
                    repeated_rf.id = Some(generated_id.clone());
                    repeated_rf.name = Some(format_repeat_name(
                        cand.name_template.as_deref(),
                        Some(&generated_id),
                        None,
                        None,
                        idx,
                        cand.count.unwrap_or(0),
                    ));
                    collect_raw_ids(&repeated_rf, protocol, region, dir, di_raw_map);
                }
            }
            collect_raw_ids(element_rf, protocol, region, dir, di_raw_map);
        }
    }
}

// ---------------------------------------------------------------------------
// 展开 / 数据构造上下文
// ---------------------------------------------------------------------------

struct BuildCtx {
    /// 模板是纯结构定义（不挂 DI 号）。按照 (id, protocol, region, dir) 进行查找，
    /// protocol/region/dir 与 data_items 的 id 查找规则一致。
    templates: HashMap<(String, String, String, Option<String>), RawTemplate>,
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
    ref_specs: Vec<HashMap<String, FieldSpec>>,
}

impl BuildScope {
    fn new() -> Self {
        Self {
            scopes: vec![HashSet::new()],
            ref_scopes: vec![HashSet::new()],
            ref_specs: vec![HashMap::new()],
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashSet::new());
        self.ref_scopes.push(HashSet::new());
        self.ref_specs.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
            self.ref_scopes.pop();
            self.ref_specs.pop();
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

    fn insert_ref_spec(&mut self, ref_id: String, spec: FieldSpec) {
        if let Some(scope) = self.ref_specs.last_mut() {
            scope.insert(ref_id, spec);
        }
    }

    fn get_ref_spec(&self, ref_id: &str) -> Option<&FieldSpec> {
        for scope in self.ref_specs.iter().rev() {
            if let Some(spec) = scope.get(ref_id) {
                return Some(spec);
            }
        }
        None
    }

    fn contains(&self, id: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(id))
    }

    fn contains_ref_id(&self, id: &str) -> bool {
        self.ref_scopes.iter().rev().any(|scope| scope.contains(id))
    }

    fn insert_field(&mut self, rf: &RawField, spec: &FieldSpec) {
        if let Some(id) = &rf.id {
            self.insert(id.clone());
        }
        if let Some(ref_id) = &rf.ref_id {
            self.insert(ref_id.clone());
            self.insert_ref(ref_id.clone());
            self.insert_ref_spec(ref_id.clone(), spec.clone());
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
            for template in &mut dict.templates {
                if template.protocol.is_none() {
                    template.protocol = Some(protocol_name.clone());
                }
            }

            combined.templates.extend(dict.templates);
            combined.data_items.extend(dict.data_items);
        }
    }

    // 在展开之前先做一遍完整的语义校验：region 格式、lengthrule 语法、
    // 是否存在未被引用的死模板、同一 id 在不相交 region 下是否语义可疑。
    // 这些问题过去只能靠肉眼审查 28000+ 行 YAML 才能发现（`region:
    // "南网,广东"` 这种把列表写成逗号拼接字符串的错误就是这么漏过去的：
    // 它不会导致任何 panic，只会让这条 DI 在广东/海南查不到，属于"运行时
    // 才会发现，而且大概率发现不了"的那类问题），提前到编译期报错能把
    // 排查成本从"生产环境查不到某条 DI"降到"改一行 YAML 重新 build"。
    validate_semantics(&combined);

    // 第一遍：建立 (id, protocol, region, dir) -> 原始定义 映射
    let mut di_raw_map = HashMap::new();
    for rf in &combined.data_items {
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
            collect_raw_ids(rf, &protocol, &region, &dir, &mut di_raw_map);
        }
    }

    let mut templates = HashMap::new();
    for template in combined.templates {
        let protocol = template
            .protocol
            .clone()
            .expect("模板的 protocol 到这里必须已经被目录名兜底过");
        let regions = template
            .region
            .clone()
            .unwrap_or_else(|| vec![DEFAULT_REGION.to_string()]);
        let dir = template.dir.clone();
        if template.id.is_empty() {
            panic!("templates 中的模板条目缺少 id");
        }
        for region in regions {
            let key = (template.id.clone(), protocol.clone(), region.clone(), dir.clone());
            if templates.contains_key(&key) {
                panic!("模板重复定义: {:?}", key);
            }
            let mut template_for_region = template.clone();
            template_for_region.region = Some(vec![region.clone()]);
            template_for_region.protocol = Some(protocol.clone());
            template_for_region.dir = dir.clone();
            templates.insert(key, template_for_region);
        }
    }

    let mut ctx = BuildCtx {
        templates,
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
            // 必须显式把这次要用的 rf 的 region 收窄成只含 top_region 的
            // 单元素列表，否则 gen_named_field -> effective_region 会优先
            // 采用 rf 自身完整的多元素 region 列表（取其 first()），导致
            // 不管这里循环到第几个 top_region，实际注册进去的永远是同一个
            // （列表里第一个）region——后面的 region 全部被静默丢弃、查
            // 不到。（templates 的展开循环在下面已经这么处理了，这里之前
            // 漏掉了，多 region 的顶层条目实际上从来没真正生效过多个
            // region，只是因为过去字典里几乎没人写超过一个 region 才没
            // 暴露出来。）
            let mut rf_for_region = rf.clone();
            rf_for_region.region = Some(vec![top_region.clone()]);
            let _ = gen_named_field(
                &rf_for_region,
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
// 编译期语义校验：在真正展开字段树之前，对整份合并后的字典做一遍静态检查。
// 这里的检查不依赖 BuildScope（还没进入递归展开阶段），只做"看得出来就是
// 错"的结构性校验；需要作用域信息的校验（ref_id 是否存在等）仍然放在
// `validate_field_refs` 里，在 `gen_named_field` 递归展开时做。
// ---------------------------------------------------------------------------

/// 校验一个 region 列表：每个元素必须是单个省份/局方/功能分组标识，不能是
/// 用逗号拼接的多个名字（这是最容易犯、也最难在运行时发现的错误——
/// `region: ["南网,广东"]` 不会让任何一次 build 失败，只会让这条 DI 在
/// "广东" 单独查询时查不到，因为它实际注册进去的 key 是字面量字符串
/// "南网,广东"，不是两个省份）。也顺带拦一下空字符串/纯空白。
fn validate_region_list(regions: &[String], context: &str) {
    for region in regions {
        if region.trim().is_empty() {
            panic!(
                "{}：region 列表中出现空字符串，应删掉这一项或补全省份名",
                context
            );
        }
        if region.contains(',') || region.contains('，') {
            panic!(
                "{}：region 写成了逗号拼接的单个字符串 {:?}，应该拆成 YAML 列表，\
                 例如 region: [\"南网\", \"广东\", \"海南\"]，而不是 \
                 region: [\"南网,广东,海南\"]（后者只会注册成一个谁都查不到的省份桶）",
                context, region
            );
        }
        if region != region.trim() {
            panic!(
                "{}：region {:?} 前后有多余空白，会导致精确匹配失败，请去除",
                context, region
            );
        }
    }
}

/// 静态校验 `lengthrule` 表达式的语法是否合法（不需要运行时数据，只检查
/// token 结构：只允许数字/0x十六进制/`ref(name)`/`index`/`index0`/四则运算/
/// 括号/空白）。与运行时 `eval_length_expr`（src/parser.rs）保持同一套文法，
/// 但这里只做语法检查、不求值，也不要求 ref 目标已注册（ref 是否存在由
/// `validate_field_refs` 在有 BuildScope 时再查）。
///
/// 过去这里只是在字符串里 `find("ref(")`——如果表达式压根不含 "ref(" 子串
/// （比如误写成裸字段名 `1 * 报文长度`），校验直接被跳过，编译期不会报错，
/// 只有实际解析到这条 DI 时才会在运行时报 `"非法表达式起始"` 错误。
fn validate_lengthrule_syntax(expr: &str, context: &str) {
    struct P<'a> {
        s: &'a str,
        pos: usize,
    }
    impl<'a> P<'a> {
        fn peek(&self) -> Option<char> {
            self.s[self.pos..].chars().next()
        }
        fn bump(&mut self) -> Option<char> {
            let c = self.peek()?;
            self.pos += c.len_utf8();
            Some(c)
        }
        fn skip_ws(&mut self) {
            while matches!(self.peek(), Some(c) if c.is_whitespace()) {
                self.bump();
            }
        }
        fn number(&mut self) -> Result<(), String> {
            let start = self.pos;
            if self.s[self.pos..].starts_with("0x") || self.s[self.pos..].starts_with("0X") {
                self.pos += 2;
                while matches!(self.peek(), Some(c) if c.is_ascii_hexdigit()) {
                    self.bump();
                }
                if self.pos == start + 2 {
                    return Err("0x 后缺少十六进制数字".to_string());
                }
            } else {
                while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                    self.bump();
                }
                if self.pos == start {
                    return Err("期望一个数字".to_string());
                }
            }
            Ok(())
        }
        fn factor(&mut self) -> Result<(), String> {
            self.skip_ws();
            if self.s[self.pos..].starts_with('(') {
                self.bump();
                self.expr()?;
                self.skip_ws();
                if self.bump() != Some(')') {
                    return Err("缺少匹配的 )".to_string());
                }
                Ok(())
            } else if self.s[self.pos..].starts_with("ref(") {
                self.pos += 4;
                let start = self.pos;
                while matches!(self.peek(), Some(c) if c != ')') {
                    self.bump();
                }
                if self.peek() != Some(')') {
                    return Err("ref(...) 未闭合".to_string());
                }
                if self.pos == start {
                    return Err("ref() 里缺少引用的 ref_id 名字".to_string());
                }
                self.bump();
                Ok(())
            } else if self.s[self.pos..].starts_with("index0") {
                self.pos += 6;
                Ok(())
            } else if self.s[self.pos..].starts_with("index") {
                self.pos += 5;
                Ok(())
            } else if matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.number()
            } else {
                let near_end = (self.pos + 12).min(self.s.len());
                Err(format!(
                    "非法的表达式起始 {:?}（附近内容: {:?}），只允许数字 / 0x十六进制 / \
                     ref(ref_id) / index / index0 / 四则运算 / 括号",
                    self.peek().unwrap_or('?'),
                    &self.s[self.pos..near_end]
                ))
            }
        }
        fn term(&mut self) -> Result<(), String> {
            self.factor()?;
            loop {
                self.skip_ws();
                match self.peek() {
                    Some('*') | Some('/') => {
                        self.bump();
                        self.factor()?;
                    }
                    _ => break,
                }
            }
            Ok(())
        }
        fn expr(&mut self) -> Result<(), String> {
            self.term()?;
            loop {
                self.skip_ws();
                match self.peek() {
                    Some('+') | Some('-') => {
                        self.bump();
                        self.term()?;
                    }
                    _ => break,
                }
            }
            Ok(())
        }
    }

    let mut p = P { s: expr, pos: 0 };
    if let Err(e) = p.expr() {
        panic!(
            "{}：lengthrule {:?} 语法非法：{}",
            context, expr, e
        );
    }
    p.skip_ws();
    if p.pos != expr.len() {
        panic!(
            "{}：lengthrule {:?} 有解析不完的多余内容: {:?}",
            context,
            expr,
            &expr[p.pos..]
        );
    }
}

/// 递归遍历一棵 RawField 树，对每个节点做结构性校验，并把遇到的
/// (protocol, id) -> (name, region 集合) 记录下来，供上层做"同一个 id 在
/// 不同 region 下语义是否可疑"的检测。`parent_protocol` 是继承规则用的
/// 生效 protocol（子字段没写 protocol 就沿用父级），跟 `effective_protocol`
/// 是同一套规则，只是这里在语义校验阶段单独跑一遍，不依赖后面展开阶段
/// 才建立的 BuildScope。
fn walk_validate_field(
    rf: &RawField,
    parent_protocol: &str,
    context: &str,
    id_name_index: &mut HashMap<(String, String), Vec<(String, Vec<String>)>>,
) {
    let protocol = rf.protocol.as_deref().unwrap_or(parent_protocol).to_string();
    let label = rf
        .id
        .clone()
        .or_else(|| rf.name.clone())
        .unwrap_or_else(|| "<unnamed>".to_string());
    let here = format!("{} > {}", context, label);

    if let Some(regions) = &rf.region {
        validate_region_list(regions, &here);
    }
    if let Some(rule) = &rf.lengthrule {
        validate_lengthrule_syntax(rule, &here);
    }
    if let (Some(id), Some(name)) = (&rf.id, &rf.name) {
        let regions = rf
            .region
            .clone()
            .unwrap_or_else(|| vec![DEFAULT_REGION.to_string()]);
        id_name_index
            .entry((protocol.clone(), id.clone()))
            .or_default()
            .push((name.clone(), regions));
    }

    if let Some(fields) = &rf.fields {
        for child in fields {
            walk_validate_field(child, &protocol, &here, id_name_index);
        }
    }
    if let Some(element) = &rf.element {
        walk_validate_field(element, &protocol, &here, id_name_index);
    }
    if let Some(cand) = &rf.candidate_ids {
        if let Some(element) = &cand.element {
            walk_validate_field(element, &protocol, &here, id_name_index);
        }
    }
    if let Some(cases) = &rf.cases {
        for (case_key, target) in cases {
            if let RawCaseTarget::Field(inner) = target {
                walk_validate_field(
                    inner,
                    &protocol,
                    &format!("{}[case {}]", here, case_key),
                    id_name_index,
                );
            }
        }
    }
    if let Some(default) = &rf.default {
        if let RawCaseTarget::Field(inner) = default {
            walk_validate_field(inner, &protocol, &format!("{}[default]", here), id_name_index);
        }
    }
}

/// 递归收集一棵 RawField 树里出现过的所有 template_ref。
fn collect_template_refs(rf: &RawField, refs: &mut HashSet<String>) {
    if let Some(r) = &rf.template_ref {
        refs.insert(r.clone());
    }
    if let Some(fields) = &rf.fields {
        for child in fields {
            collect_template_refs(child, refs);
        }
    }
    if let Some(element) = &rf.element {
        collect_template_refs(element, refs);
    }
    if let Some(cand) = &rf.candidate_ids {
        if let Some(element) = &cand.element {
            collect_template_refs(element, refs);
        }
    }
    if let Some(cases) = &rf.cases {
        for target in cases.values() {
            if let RawCaseTarget::Field(inner) = target {
                collect_template_refs(inner, refs);
            }
            if let RawCaseTarget::Name(name) = target {
                // cases 的值也可以直接写模板名（见设计文档 switch 一节）
                refs.insert(name.clone());
            }
        }
    }
    if let Some(default) = &rf.default {
        match default {
            RawCaseTarget::Field(inner) => collect_template_refs(inner, refs),
            RawCaseTarget::Name(name) => {
                refs.insert(name.clone());
            }
        }
    }
}

/// 死模板检测：定义了但从没被任何 data_item / 其它模板引用的模板，
/// 大概率是遗漏了引用、或者是已经不再需要的历史遗留定义，两种情况都
/// 值得人工看一眼，所以给 warning（不 panic —— 模板允许暂时定义好但还
/// 没接入使用，不应该阻塞构建）。
fn warn_unused_templates(combined: &RawDict) {
    let mut used: HashSet<String> = HashSet::new();
    for rf in &combined.data_items {
        collect_template_refs(rf, &mut used);
    }
    for t in &combined.templates {
        for f in &t.fields {
            collect_template_refs(f, &mut used);
        }
    }
    let mut defined: Vec<&str> = combined.templates.iter().map(|t| t.id.as_str()).collect();
    defined.sort();
    defined.dedup();
    for id in defined {
        if !used.contains(id) {
            println!(
                "cargo:warning=模板 {:?} 定义了但从未被任何 template_ref 引用，\
                 确认是否是死代码（可删除）或者遗漏了引用",
                id
            );
        }
    }
}

fn validate_semantics(combined: &RawDict) {
    warn_unused_templates(combined);

    // key 是 (protocol, id)——不同协议下允许出现同一个 DI 号且互不干扰
    // （`schema/csg16/csg16.yaml` 就特意构造了这种情况来验证协议隔离性），
    // 混在一起比对会把这种合法情况当成假冲突报出来。
    let mut id_name_index: HashMap<(String, String), Vec<(String, Vec<String>)>> = HashMap::new();

    for rf in &combined.data_items {
        let top_protocol = rf
            .protocol
            .clone()
            .expect("顶层条目的 protocol 到这里必须已经被目录名兜底过");
        let label = rf
            .id
            .clone()
            .or_else(|| rf.name.clone())
            .unwrap_or_else(|| "<unnamed data_item>".to_string());
        walk_validate_field(
            rf,
            &top_protocol,
            &format!("data_items[{}]", label),
            &mut id_name_index,
        );
    }
    for t in &combined.templates {
        let top_protocol = t
            .protocol
            .clone()
            .expect("模板的 protocol 到这里必须已经被目录名兜底过");
        if let Some(regions) = &t.region {
            validate_region_list(regions, &format!("templates[{}]", t.id));
        }
        for f in &t.fields {
            walk_validate_field(
                f,
                &top_protocol,
                &format!("templates[{}]", t.id),
                &mut id_name_index,
            );
        }
    }

    // 同一个 (protocol, id) 出现了不同的 name：这在协议里是合法用法（不同
    // 省份用同一个 DI 号表达同一件事、但措辞略有差异），但也是最典型的
    // "复制粘贴时 id 打错了、其实是两个完全不相关字段" 的信号（这次实际
    // 就是用这个模式抓到了 E0001210/E1800034 两处真实冲突，两处的 region
    // 还都是互不重叠的，所以特意不按 region 是否重叠做过滤）。
    //
    // 注意：完全相同的 (name, region) 重复出现（比如同一个共用子结构被
    // 多个不同的父容器各自内联引用一次）是设计上允许的正常情况，只要
    // name 没有分歧就不应该报——所以这里只统计"不同的 name 有几种"，
    // 不对"出现了几次"本身做任何判断。
    let mut keys: Vec<&(String, String)> = id_name_index.keys().collect();
    keys.sort();
    for key in keys {
        let occurrences = &id_name_index[key];
        let mut distinct_names: Vec<&str> = occurrences.iter().map(|(n, _)| n.as_str()).collect();
        distinct_names.sort();
        distinct_names.dedup();
        if distinct_names.len() > 1 {
            println!(
                "cargo:warning=protocol {:?} 下 id {:?} 在不同 region 下出现了 {} 种不同的 \
                 name（{:?}），请人工确认是否是复制粘贴 id 打错了，而不是同一字段的多省份\
                 措辞差异",
                key.0,
                key.1,
                distinct_names.len(),
                distinct_names
            );
        }
    }
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

// 解析 format 字段（运行时用于展示格式），简单的表达式支持：
// - 支持裸类型名称（"hex"/"bcd"/"bin"）或者 key=value 列表，
//   用逗号分隔，key 可以是 type/group_bytes/separator/pad/endian/order
fn parse_format_string(s: &str) -> Option<FormatSpec> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let mut spec = FormatSpec {
        ftype: FormatType::Hex,
        group_bytes: None,
        separator: None,
        pad: None,
        byte_order: None,
        order: None,
    };
    // bare type
    if s == "hex" || s == "bcd" || s == "bin" {
        spec.ftype = match s {
            "hex" => FormatType::Hex,
            "bcd" => FormatType::Bcd,
            _ => FormatType::Bin,
        };
        return Some(spec);
    }
    let inner = if s.starts_with('(') && s.ends_with(')') {
        &s[1..s.len() - 1]
    } else if let Some(idx) = s.find('(') {
        if s.ends_with(')') {
            &s[idx + 1..s.len() - 1]
        } else {
            s
        }
    } else {
        s
    };
    for part in inner.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(eq) = part.find('=') {
            let key = part[..eq].trim();
            let val = part[eq + 1..].trim().trim_matches('"');
            match key {
                "type" => {
                    spec.ftype = match val {
                        "hex" => FormatType::Hex,
                        "bcd" => FormatType::Bcd,
                        "bin" => FormatType::Bin,
                        other => panic!("未知 format type: {}", other),
                    }
                }
                "group_bytes" => {
                    if let Ok(n) = val.parse::<usize>() {
                        spec.group_bytes = Some(n);
                    }
                }
                "separator" | "sep" => {
                    spec.separator = Some(val.to_string());
                }
                "pad" => {
                    if val == "true" {
                        spec.pad = Some(true)
                    } else if val == "false" {
                        spec.pad = Some(false)
                    }
                }
                "endian" => {
                    spec.byte_order = match val {
                        "big" => Some(Endian::Big),
                        "little" => Some(Endian::Little),
                        other => panic!("未知 format endian: {}", other),
                    }
                }
                "order" => {
                    spec.order = match val {
                        "normal" => Some(FormatOrder::Normal),
                        "reverse" => Some(FormatOrder::Reverse),
                        other => panic!("未知 format order: {}", other),
                    }
                }
                _ => {}
            }
        } else {
            if part == "hex" {
                spec.ftype = FormatType::Hex;
            }
        }
    }
    Some(spec)
}

fn parse_format_spec(rf: &RawField) -> Option<FormatSpec> {
    if let Some(format) = &rf.format {
        match format {
            RawFormat::String(s) => parse_format_string(s),
            RawFormat::Object(o) => {
                let has_any_field = o.kind.is_some()
                    || o.group_bytes.is_some()
                    || o.separator.is_some()
                    || o.pad.is_some()
                    || o.endian.is_some()
                    || o.order.is_some();
                if !has_any_field {
                    return None;
                }
                if let Some(group_bytes) = o.group_bytes {
                    if group_bytes == 0 {
                        panic!(
                            "format.group_bytes 不能为 0（字段: {:?}）",
                            rf.name
                        );
                    }
                }

                let ftype = match o.kind.as_deref().or(rf.ty.as_deref()).unwrap_or("hex") {
                    "hex" => FormatType::Hex,
                    "bcd" => FormatType::Bcd,
                    "bin" => FormatType::Bin,
                    other => panic!("未知 format type: {}", other),
                };
                let byte_order = match o.endian.as_deref() {
                    Some("big") => Some(Endian::Big),
                    Some("little") => Some(Endian::Little),
                    None => None,
                    Some(other) => panic!("未知 format endian: {}", other),
                };
                let order = match o.order.as_deref() {
                    Some("normal") | None => Some(FormatOrder::Normal),
                    Some("reverse") => Some(FormatOrder::Reverse),
                    Some(other) => panic!("未知 format order: {}", other),
                };
                Some(FormatSpec {
                    ftype,
                    group_bytes: o.group_bytes,
                    separator: o.separator.clone(),
                    pad: o.pad,
                    byte_order,
                    order,
                })
            }
        }
    } else if rf.group_bytes.is_some() || rf.separator.is_some() || rf.pad.is_some() {
        let ftype = match rf.ty.as_deref().unwrap_or("hex") {
            "hex" => FormatType::Hex,
            "bcd" => FormatType::Bcd,
            "bin" => FormatType::Bin,
            _ => FormatType::Hex,
        };
        let byte_order = match rf.endian.as_deref() {
            Some("big") => Some(Endian::Big),
            Some("little") => Some(Endian::Little),
            None => None,
            Some(other) => panic!("未知 endian: {}（字段: {:?}）", other, rf.name),
        };
        Some(FormatSpec {
            ftype,
            group_bytes: rf.group_bytes,
            separator: rf.separator.clone(),
            pad: rf.pad,
            byte_order,
            order: Some(FormatOrder::Normal),
        })
    } else {
        None
    }
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

    let format_spec = parse_format_spec(rf);

    if let Some(id_str) = &rf.id {
        let raw_key = (
            id_str.clone(),
            protocol.clone(),
            region.clone(),
            dir.clone(),
        );
        ctx.di_raw_map.entry(raw_key).or_insert_with(|| rf.clone());

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
                        format: format_spec.clone(),
                    },
                ));
            }
        }
    }

    // candidate_ids 相关的编译期生成由子字段（fields 中的 candidate_ids 条目）处理，
    // 不在这里重复处理。

    scope.insert_field(rf, &spec);
    // candidate_ids 的注册已在上面完成（通过 gen_named_field），无需额外向作用域直接插入原始字符串
    NamedField {
        id: rf.id.clone(),
        ref_id: rf.ref_id.clone(),
        name,
        spec,
        format: format_spec,
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
        let count = cand.count;
        let count_expr = cand.count_expr.clone();
        if cand.count_ref.is_some() && cand.count_expr.is_some() {
            panic!(
                "candidate_ids 在字段 {} 中不能同时指定 count_ref 和 count_expr",
                field_label(rf)
            );
        }
        let id_expr = cand
            .id_expr
            .as_ref()
            .unwrap_or_else(|| panic!("candidate_ids 在字段 {} 中缺少 id_expr", field_label(rf)));
        let element_rf = cand
            .element
            .as_ref()
            .unwrap_or_else(|| panic!("candidate_ids 在字段 {} 中缺少 element", field_label(rf)));

        // 如果提供了 count，则按 count 注册全部候选，供以后单独寻址；
        // 如果没有 count，但提供了 count_expr，则只能在运行时解析时计算实际数量。
        if let Some(count) = count {
            for idx in 0..count {
                let generated_id = format_id_expr(id_expr, idx)
                    .unwrap_or_else(|e| panic!("id_expr 解析失败: {:?}", e));
                let mut repeated_rf = (**element_rf).clone();
                repeated_rf.id = Some(generated_id.clone());
                repeated_rf.name = Some(format_repeat_name(
                    cand.name_template.as_deref(),
                    Some(&generated_id),
                    None,
                    None,
                    idx,
                    count,
                ));
                // 生成的 candidate_ids 也要进入 di_raw_map，供 later di_sequence 查找。
                ctx.di_raw_map.insert(
                    (
                        generated_id.clone(),
                        protocol.to_string(),
                        region.to_string(),
                        dir.map(|s| s.to_string()),
                    ),
                    repeated_rf.clone(),
                );
                gen_named_field(&repeated_rf, protocol, region, dir, ctx, scope); // 仅登记，返回值丢弃
            }
        }

        return if cand.count_ref.is_some() || cand.count_expr.is_some() {
            let count_ref = cand.count_ref.clone();
            FieldSpec::Repeat {
                count_ref,
                count_expr,
                bits_ref: None,
                bit_direction: None,
                iterate_order: None,
                bit_specs: None,
                element: Box::new(gen_field(element_rf, protocol, region, dir, ctx, scope)),
                name_template: cand.name_template.clone(),
                id_expr: Some(id_expr.clone()),
            }
        } else {
            let count = count.unwrap_or_else(|| panic!("candidate_ids 在字段 {} 中缺少 count", field_label(rf)));
            let mut named = Vec::with_capacity(count);
            for idx in 0..count {
                let generated_id = format_id_expr(id_expr, idx)
                    .unwrap_or_else(|e| panic!("id_expr 解析失败: {:?}", e));
                let mut repeated_rf = (**element_rf).clone();
                repeated_rf.id = Some(generated_id.clone());
                repeated_rf.name = Some(format_repeat_name(
                    cand.name_template.as_deref(),
                    Some(&generated_id),
                    None,
                    None,
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
    let ty = ty.to_lowercase();

    match ty.as_str() {
        "bcd" | "bin" | "ascii" | "hex" | "time" => gen_fixed(rf, &ty),
        "fixed" | "NORMAL" | "normal" => FieldSpec::Fixed {
            encoding: Encoding::Raw,
            length: resolve_field_length(rf),
            unit: None,
            enum_map: None,
            format: None,
        },
        "bitfield" => gen_bitfield(rf),
        "switch" => gen_switch(rf, protocol, region, dir, ctx, scope),
        "repeat" => gen_repeat(rf, protocol, region, dir, ctx, scope),
        "bitmask" => gen_bitmask(rf, protocol, region, dir, ctx, scope),
        "skip" => FieldSpec::Skip,
        "template" => {
            let tname = rf
                .template_ref
                .clone()
                .or_else(|| rf.ref_.clone())
                .unwrap_or_else(|| panic!("template 字段 {:?} 缺少 template_ref", rf.name));
            gen_template_ref(&tname, protocol, region, dir, ctx, scope)
        }
        "external" => gen_external(rf),
        "di_sequence" => gen_di_sequence(rf, protocol, region, dir, ctx, scope),
        "dict_ref" => gen_dict_ref(rf),
        "custom" => gen_custom(rf),
        "info_point" => gen_info_point(rf),
        "di_code" => gen_di_code(rf),
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
        other => panic!("未知字段类型: {}（字段: {:?}）", other, rf),
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
    } else if let Some(serde_yaml::Value::Number(_)) = &rf.length {
        let len = get_len(rf);
        return FieldLength::Fixed(len);
    } else if let Some(rule) = &rf.lengthrule {
        return FieldLength::Expr(rule.clone());
    } else {
        panic!(
            "字段 {:?} 需要一个整数 length、length_ref 或 lengthrule，实际是 {:?} / {:?}",
            rf.name, rf.length, rf.lengthrule
        );
    }
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
            "字段 {:?} 的 {} 引用了未知 ref_id: {:?}，引用必须使用字段 ref_id",
            rf,
            kind,
            ref_name
        );
    }
}

fn resolve_dict_ref(rf: &RawField) -> Option<String> {
    if let Some(dict_ref) = &rf.dict_ref {
        match dict_ref {
            serde_yaml::Value::Mapping(map) => {
                if let Some(key) = map.get(&serde_yaml::Value::String("ref_id".to_string())) {
                    if let serde_yaml::Value::String(s) = key {
                        return Some(s.clone());
                    }
                }
                panic!(
                    "dict_ref 字段 {:?} 的 dict_ref 对象必须包含 ref_id 字段",
                    rf.name
                );
            }
            other => panic!(
                "dict_ref 字段 {:?} 的 dict_ref 必须是对象 {{ ref_id: ... }}，实际是 {:?}",
                rf.name, other
            ),
        }
    }
    None
}

fn validate_field_refs(rf: &RawField, scope: &BuildScope) {
    if let Some(length_ref) = &rf.length_ref {
        if length_ref != "$remaining" && length_ref != "$len" && length_ref != "$length" {
            validate_id_ref(rf, length_ref, "length_ref", scope);
        }
    }
    if let Some(length_rule) = &rf.lengthrule {
        // 验证表达式里用到的 ref(...) 引用是否存在
        let mut start = 0usize;
        while let Some(pos) = length_rule[start..].find("ref(") {
            let abs = start + pos + 4; // 指向 '(' 后的起始
            if let Some(end_pos) = length_rule[abs..].find(')') {
                let name = &length_rule[abs..abs + end_pos];
                validate_id_ref(rf, name, "lengthrule ref", scope);
                start = abs + end_pos + 1;
            } else {
                panic!("字段 {:?} 的 lengthrule 包含未闭合的 ref(...)", rf.name)
            }
        }
    }
    if let Some(count_ref) = &rf.count_ref {
        validate_id_ref(rf, count_ref, "count_ref", scope);
    }
    if let Some(bits_ref) = &rf.bits_ref {
        validate_id_ref(rf, bits_ref, "bits_ref", scope);
    }
    if let Some(on) = &rf.on {
        if on != "$remaining" && on != "$len" && on != "$length" && !on.starts_with('$') {
            validate_id_ref(rf, on, "switch.on", scope);
        }
    }
    if let Some(ref_name) = &rf.ref_ {
        if rf.ty.as_deref() == Some("dict_ref") {
            panic!(
                "dict_ref 字段 {:?} 不支持 ref: {:?}，请改用 dict_ref: {{ ref_id: ... }}",
                rf.name, ref_name
            );
        }
    }
    if let Some(dict_ref_name) = resolve_dict_ref(rf) {
        if rf.ty.as_deref() == Some("dict_ref") {
            validate_id_ref(rf, &dict_ref_name, "dict_ref.ref", scope);
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
    let mut format_spec = parse_format_spec(rf);
    let format_endian = match rf.endian.as_deref() {
        Some("big") => Some(Endian::Big),
        Some("little") => Some(Endian::Little),
        None => None,
        Some(other) => panic!("未知 endian: {}（字段: {:?}）", other, rf.name),
    };
    if let Some(spec) = format_spec.as_mut() {
        if spec.byte_order.is_none() {
            spec.byte_order = format_endian;
        }
    }
    if rf.ty.as_deref() == Some("hex") {
        if let Some(spec) = format_spec.as_mut() {
            if spec.byte_order.is_none() {
                spec.byte_order = format_endian;
            }
        } else if format_endian.is_some() {
            format_spec = Some(FormatSpec {
                ftype: FormatType::Hex,
                group_bytes: None,
                separator: Some(String::new()),
                pad: Some(true),
                byte_order: format_endian,
                order: Some(FormatOrder::Normal),
            });
        }
    }

    // 检查是否有 time（表示这是时间字段，编码方式由 type 指定）
    if let Some(time_fmt) = &rf.time {
        let encoding = match ty {
            "bin" => Encoding::Time {
                format: time_fmt.clone(),
                encoding: TimeEncoding::Bin {
                    endian: resolve_endian(rf),
                },
            },
            "bcd" => Encoding::Time {
                format: time_fmt.clone(),
                encoding: TimeEncoding::Bcd,
            },
            other => panic!(
                "字段 {:?} 的 time 只能配合 type: bcd 或 type: bin，实际是 {}",
                rf.name,
                other
            ),
        };
        return FieldSpec::Fixed {
            encoding,
            length,
            unit: rf.unit.clone(),
            enum_map: rf.enum_map.clone(),
            format: format_spec,
        };
    }
    
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
        _ => unreachable!(),
    };
    FieldSpec::Fixed {
        encoding,
        length,
        unit: rf.unit.clone(),
        enum_map: rf.enum_map.clone(),
        format: format_spec,
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
            ref_id: b.ref_id.clone(),
            enum_map: b.enum_map.clone(),
        })
        .collect();
    FieldSpec::BitField { length, bits }
}

fn build_bit_specs(rf: &RawField, length: usize) -> Vec<BitSpec> {
    if let Some(bits) = &rf.bits {
        bits.iter()
            .map(|b| BitSpec {
                range: b.range,
                name: b.name.clone(),
                ref_id: b.ref_id.clone(),
                enum_map: b.enum_map.clone(),
            })
            .collect()
    } else {
        let bit_count = length * 8;
        (0..bit_count)
            .map(|bit_index| BitSpec {
                range: (bit_index, bit_index),
                name: format!("bit{}", bit_index + 1),
                ref_id: None,
                enum_map: None,
            })
            .collect()
    }
}

fn gen_bitmask(
    rf: &RawField,
    protocol: &str,
    region: &str,
    dir: Option<&str>,
    ctx: &mut BuildCtx,
    scope: &mut BuildScope,
) -> FieldSpec {
    let length = get_len(rf);
    let bit_specs = build_bit_specs(rf, length);
    let element_rf = rf
        .element
        .as_ref()
        .unwrap_or_else(|| panic!("bitmask 字段 {:?} 缺少 element", rf.name));
    let element = Box::new(gen_field(element_rf, protocol, region, dir, ctx, scope));
    let name_template = rf.name_template.clone();
    let bit_direction = rf
        .bit_direction
        .clone();
    let iterate_order = rf
        .iterate_order
        .clone();
    FieldSpec::BitMask {
        length,
        bit_direction,
        iterate_order,
        bit_specs,
        element,
        name_template,
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
            if template_exists(&ctx.templates, name, protocol, region, dir) {
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
    let mut case_names_map: HashMap<String, String> = HashMap::new();
    for (key, target) in &cases {
        let target_spec =
            resolve_switch_target(target, switch_len, protocol, region, dir, ctx, scope);
        cases_map.insert(key.clone(), Box::new(target_spec));
        if let RawCaseTarget::Field(field) = target {
            if let Some(n) = &field.name {
                case_names_map.insert(key.clone(), n.clone());
            }
        }
    }
    let default = default_target.map(|target| {
        Box::new(resolve_switch_target(
            &target, switch_len, protocol, region, dir, ctx, scope,
        ))
    });

    let case_names = if case_names_map.is_empty() {
        None
    } else {
        Some(case_names_map)
    };

    FieldSpec::Switch {
        on,
        cases: cases_map,
        case_names,
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
    if rf.count_ref.is_some() && rf.bits_ref.is_some() {
        panic!(
            "repeat 字段 {:?} 不能同时含有 count_ref 和 bits_ref，请选择一种驱动方式",
            rf.name
        );
    }
    if rf.bits_ref.is_some() && id_expr.is_some() {
        panic!(
            "repeat 字段 {:?} 使用 bits_ref 时不支持 id_expr",
            rf.name
        );
    }
    if rf.bits_ref.is_some() && rf.count.is_some() {
        panic!(
            "repeat 字段 {:?} 使用 bits_ref 时不应同时指定 count",
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

    let bit_specs = if let Some(bits_ref) = &rf.bits_ref {
        let bitfield_spec = scope
            .get_ref_spec(bits_ref)
            .unwrap_or_else(|| {
                panic!(
                    "repeat 字段 {:?} 的 bits_ref {:?} 未绑定到 bitfield",
                    rf.name, bits_ref
                )
            });
        let specs = match bitfield_spec {
            FieldSpec::BitField { bits, .. } => bits.clone(),
            _ => panic!(
                "repeat 字段 {:?} 的 bits_ref {:?} 必须引用一个 bitfield 字段",
                rf.name, bits_ref
            ),
        };
        Some(specs)
    } else {
        None
    };

    let count_ref = rf.count_ref.clone();
    let count_expr = rf.count_expr.clone();
    if rf.count_ref.is_some() && rf.count_expr.is_some() {
        panic!(
            "repeat 字段 {:?} 不能同时指定 count_ref 和 count_expr，请取其一",
            rf.name
        );
    }
    // 如果 repeat 本身没有明确的 name_template，而 element 的定义里有 name，
    // 我们在编译期选择性地把 element.name 作为 name_template 继承下来，
    // 但仅在 element 不是 Container（即解析结果为非映射/非子字段集合）时才继承，
    // 以避免把多字段的 container 类型重复项包一层额外的 node，破坏既有
    // 对容器重复的解析结构（测试依赖）。
    let name_template = rf.name_template.clone().or_else(|| element_rf.name.clone());
    let bit_direction = rf
        .bit_direction
        .clone();
    let iterate_order = rf
        .iterate_order
        .clone();
    let bits_ref = rf.bits_ref.clone();
    FieldSpec::Repeat {
        count_ref,
        count_expr,
        bits_ref,
        bit_direction,
        iterate_order,
        bit_specs,
        element: Box::new(element),
        name_template,
        id_expr,
    }
}

// `extended` 功能已被移除。若需要在编译期注册一组候选 id，
// 请在对应条目使用 `candidate_ids: ["00000100", "00000200", ...]`。

fn lookup_template<'a>(
    templates: &'a HashMap<(String, String, String, Option<String>), RawTemplate>,
    id: &str,
    protocol: &str,
    region: &str,
    dir: Option<&str>,
) -> &'a RawTemplate {
    let dir_owned = dir.map(|s| s.to_string());
    if let Some(template) = templates.get(&(id.to_string(), protocol.to_string(), region.to_string(), dir_owned.clone())) {
        return template;
    }
    if dir_owned.is_some() {
        if let Some(template) = templates.get(&(id.to_string(), protocol.to_string(), region.to_string(), None)) {
            return template;
        }
    }
    if region != DEFAULT_REGION {
        if let Some(template) = templates.get(&(id.to_string(), protocol.to_string(), DEFAULT_REGION.to_string(), dir_owned.clone())) {
            return template;
        }
        if let Some(template) = templates.get(&(id.to_string(), protocol.to_string(), DEFAULT_REGION.to_string(), None)) {
            return template;
        }
    }
    panic!(
        "找不到模板: {}（protocol={:?}, region={:?}, dir={:?}，且该 protocol 下所有 region/dir 回退组合都没有定义）",
        id, protocol, region, dir
    );
}

fn template_exists(
    templates: &HashMap<(String, String, String, Option<String>), RawTemplate>,
    id: &str,
    protocol: &str,
    region: &str,
    dir: Option<&str>,
) -> bool {
    let dir_owned = dir.map(|s| s.to_string());
    if templates.contains_key(&(id.to_string(), protocol.to_string(), region.to_string(), dir_owned.clone())) {
        return true;
    }
    if dir_owned.is_some() {
        if templates.contains_key(&(id.to_string(), protocol.to_string(), region.to_string(), None)) {
            return true;
        }
    }
    if region != DEFAULT_REGION {
        if templates.contains_key(&(id.to_string(), protocol.to_string(), DEFAULT_REGION.to_string(), dir_owned.clone())) {
            return true;
        }
        if templates.contains_key(&(id.to_string(), protocol.to_string(), DEFAULT_REGION.to_string(), None)) {
            return true;
        }
    }
    false
}

fn gen_template_ref(
    tname: &str,
    protocol: &str,
    region: &str,
    dir: Option<&str>,
    ctx: &mut BuildCtx,
    scope: &mut BuildScope,
) -> FieldSpec {
    let template = lookup_template(&ctx.templates, tname, protocol, region, dir).clone();
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
    let length = if let Some(length_ref) = &rf.length_ref {
        ExternalLength::Ref(length_ref.clone())
    } else {
        match &rf.length {
            Some(serde_yaml::Value::String(s)) if s == "remaining" => ExternalLength::Remaining,
            Some(serde_yaml::Value::String(s)) => ExternalLength::Ref(s.clone()),
            Some(serde_yaml::Value::Number(n)) => ExternalLength::Fixed(
                n.as_u64()
                    .unwrap_or_else(|| panic!("external 字段 {:?} 的 length 数值非法", rf.name))
                    as usize,
            ),
            _ => panic!(
                "external 字段 {:?} 需要 length: remaining | <引用字段名> | <数值>，或使用 length_ref: <引用字段名>",
                rf.name
            ),
        }
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
    let di_ref = resolve_dict_ref(rf).unwrap_or_else(|| {
        panic!(
            "dict_ref 字段 {:?} 缺少 dict_ref: {{ ref_id: ... }}",
            rf.name
        )
    });
    FieldSpec::DictRef { di_ref }
}

fn gen_custom(rf: &RawField) -> FieldSpec {
    let handler = rf
        .handler
        .clone()
        .unwrap_or_else(|| panic!("custom 字段 {:?} 缺少 handler", rf.name));
    FieldSpec::Custom(handler)
}

/// `info_point` —— 信息点标识 DA（6.1.3），固定2字节，无需任何配置项。
/// 如果 YAML 里写了 length，仅做一次健全性检查（必须是2），避免手滑写错
/// 长度却因为字段本身不读取 length 而悄悄被忽略。
fn gen_info_point(rf: &RawField) -> FieldSpec {
    if let Some(serde_yaml::Value::Number(n)) = &rf.length {
        let len = n.as_u64().unwrap_or(0);
        if len != 2 {
            panic!(
                "info_point 字段 {:?} 的 length 必须是 2（DA1+DA2），实际写了 {}",
                rf.name, len
            );
        }
    }
    FieldSpec::InfoPoint
}

/// `di_code` —— 数据标识编码 DI（6.1.4），固定4字节，无需任何配置项。
/// 如果 YAML 里写了 length，仅做健全性检查（必须是4）。
fn gen_di_code(rf: &RawField) -> FieldSpec {
    if let Some(serde_yaml::Value::Number(n)) = &rf.length {
        let len = n.as_u64().unwrap_or(0);
        if len != 4 {
            panic!(
                "di_code 字段 {:?} 的 length 必须是 4（DI0..DI3），实际写了 {}",
                rf.name, len
            );
        }
    }
    FieldSpec::DiCode
}
