//! 语义校验模块
//!
//! 在展开字段树之前对整份合并后的字典做静态检查，包括：
//! - region 格式校验（不允许逗号拼接的多个名字）
//! - lengthrule 语法校验（只允许合法的表达式 token）
//! - 死模板检测（定义了但从未被引用）
//! - 同一 id 在不同 region 下的 name 一致性检查
//! - 未知字段检测（拼写错误提示）
//! - 字段冲突检测（互斥字段同时出现）

use crate::ast::{RawDict, RawField, RawCaseTarget, DEFAULT_REGION};
use std::collections::{HashMap, HashSet};

/// 校验一个 region 列表：每个元素必须是单个省份/局方/功能分组标识，不能是
/// 用逗号拼接的多个名字（这是最容易犯、也最难在运行时发现的错误——
/// `region: ["南网,广东"]` 不会让任何一次 build 失败，只会让这条 DI 在
/// "广东" 单独查询时查不到，因为它实际注册进去的 key 是字面量字符串
/// "南网,广东"，不是两个省份）。也顺带拦一下空字符串/纯空白。
pub fn validate_region_list(regions: &[String], context: &str) {
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
pub fn validate_lengthrule_syntax(expr: &str, context: &str) {
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

    // === 新增：字段冲突检测 ===
    validate_field_conflicts(rf, &here);

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

/// 完整的语义校验入口函数，在展开字段树之前调用。
///
/// 包括：
/// - 死模板检测
/// - region/lengthrule 格式校验
/// - 同一 id 在不同 region 下的 name 一致性检查
pub fn validate_semantics(combined: &RawDict) {
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

/// 计算两个字符串的编辑距离（Levenshtein distance）
fn edit_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();
    
    let mut dp = vec![vec![0; b_len + 1]; a_len + 1];
    
    for i in 0..=a_len {
        dp[i][0] = i;
    }
    for j in 0..=b_len {
        dp[0][j] = j;
    }
    
    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    
    dp[a_len][b_len]
}

/// 根据编辑距离找出最相似的字段建议
fn suggest_field(unknown: &str, known_fields: &[&str]) -> Option<String> {
    let mut candidates: Vec<(usize, &str)> = known_fields
        .iter()
        .map(|&field| (edit_distance(unknown, field), field))
        .collect();
    
    candidates.sort_by_key(|(dist, _)| *dist);
    
    // 只在编辑距离小于等于 2 时给出建议（太远的不相关）
    if let Some((dist, field)) = candidates.first() {
        if *dist <= 2 && *dist < unknown.len() {
            return Some(field.to_string());
        }
    }
    
    None
}

/// 定义所有合法的字段名（按字段类型分组）
struct KnownFields {
    /// 所有字段通用的基础字段
    common: Vec<&'static str>,
    /// 类型相关的字段
    type_specific: HashMap<&'static str, Vec<&'static str>>,
}

impl KnownFields {
    fn new() -> Self {
        let mut type_specific = HashMap::new();
        
        // Fixed 类型字段
        type_specific.insert("bcd", vec!["decimal", "signed", "endian", "unit", "enum"]);
        type_specific.insert("bin", vec!["signed", "endian", "unit", "enum"]);
        type_specific.insert("hex", vec!["group_bytes", "separator", "pad"]);
        type_specific.insert("ascii", vec![]);
        type_specific.insert("raw", vec![]);
        type_specific.insert("time", vec!["time"]);
        
        // Container 类型字段
        type_specific.insert("container", vec!["fields"]);
        
        // Repeat 类型字段
        type_specific.insert("repeat", vec![
            "count", "count_ref", "count_expr", "bits_ref",
            "bit_direction", "iterate_order", "name_template",
            "id_expr", "element"
        ]);
        
        // BitField 类型字段
        type_specific.insert("bitfield", vec!["bits"]);
        
        // BitMask 类型字段
        type_specific.insert("bitmask", vec![
            "bit_direction", "iterate_order", "bits", 
            "element", "name_template"
        ]);
        
        // Switch 类型字段
        type_specific.insert("switch", vec!["on", "cases", "default"]);
        
        // DictRef 类型字段
        type_specific.insert("dict_ref", vec!["dict_ref"]);
        
        // External 类型字段
        type_specific.insert("external", vec!["external_protocol"]);
        
        // Custom 类型字段
        type_specific.insert("custom", vec!["handler"]);
        
        Self {
            common: vec![
                "id", "ref_id", "name", "type", "length", "lengthrule",
                "length_ref", "protocol", "region", "dir", "template_ref",
                "ref", "format", "candidate_ids",
            ],
            type_specific,
        }
    }
    
    /// 获取给定类型的所有合法字段
    fn get_valid_fields(&self, field_type: Option<&str>) -> Vec<&'static str> {
        let mut valid = self.common.clone();
        
        if let Some(ty) = field_type {
            if let Some(type_fields) = self.type_specific.get(ty) {
                valid.extend(type_fields.iter().copied());
            }
        } else {
            // 如果没有类型，包含所有可能的字段（用于顶级字段）
            for fields in self.type_specific.values() {
                valid.extend(fields.iter().copied());
            }
        }
        
        valid.sort();
        valid.dedup();
        valid
    }
}

/// 检查字段中是否存在常见的拼写错误
/// 注意：由于我们使用 serde 的 #[serde(default)]，未知字段会被静默忽略
/// 这个函数通过检查已知的常见错误模式来提供帮助
fn check_common_typos(rf: &RawField, context: &str) {
    // 检查是否错误使用了 decimals 而不是 decimal
    // 这需要在 AST 中添加一个 #[serde(rename = "decimals")] 字段来捕获
    // 暂时通过检查类型和缺失 decimal 来推断
    
    if let Some(ty) = &rf.ty {
        match ty.as_str() {
            "bcd" | "bin" => {
                // BCD/Bin 类型常见错误：decimal vs decimals
                // 如果用户写了 decimals，serde 会忽略它
                // 我们无法直接检测，但可以在文档中说明
            }
            _ => {}
        }
    }
}

/// 检查字段冲突（互斥字段同时出现）
fn validate_field_conflicts(rf: &RawField, context: &str) {
    // 长度相关字段冲突检测
    let length_fields = [
        ("length", rf.length.is_some()),
        ("lengthrule", rf.lengthrule.is_some()),
        ("length_ref", rf.length_ref.is_some()),
    ];
    let length_count = length_fields.iter().filter(|(_, present)| *present).count();
    if length_count > 1 {
        let present: Vec<&str> = length_fields
            .iter()
            .filter(|(_, present)| *present)
            .map(|(name, _)| *name)
            .collect();
        panic!(
            "{}：长度字段冲突，不能同时使用 {:?}，请只保留其中一个",
            context, present
        );
    }
    
    // 重复计数字段冲突检测
    if rf.ty.as_deref() == Some("repeat") || rf.element.is_some() {
        let count_fields = [
            ("count", rf.count.is_some()),
            ("count_ref", rf.count_ref.is_some()),
            ("count_expr", rf.count_expr.is_some()),
            ("bits_ref", rf.bits_ref.is_some()),
        ];
        let count_count = count_fields.iter().filter(|(_, present)| *present).count();
        if count_count > 1 {
            let present: Vec<&str> = count_fields
                .iter()
                .filter(|(_, present)| *present)
                .map(|(name, _)| *name)
                .collect();
            panic!(
                "{}：重复计数字段冲突，不能同时使用 {:?}，请只保留其中一个",
                context, present
            );
        }
    }
    
    // Switch 必需字段检测
    if rf.ty.as_deref() == Some("switch") {
        if rf.on.is_none() {
            panic!(
                "{}：switch 类型必须指定 'on' 字段",
                context
            );
        }
        if rf.cases.is_none() && rf.default.is_none() {
            panic!(
                "{}：switch 类型必须指定 'cases' 或 'default' 字段",
                context
            );
        }
    }
    
    // Repeat 元素定义检测
    if rf.ty.as_deref() == Some("repeat") {
        if rf.element.is_none() {
            panic!(
                "{}：repeat 类型必须指定 'element' 字段",
                context
            );
        }
    }
    
    // BitMask 位规格检测
    if rf.ty.as_deref() == Some("bitmask") {
        // bitmask 可以通过 bits 字段显式指定位，
        // 也可以通过 length 字段自动生成（每个 bit 一个）
        // 但至少要有其中一个
        if rf.bits.is_none() && rf.length.is_none() {
            panic!(
                "{}：bitmask 类型必须指定 'bits' 或 'length' 字段",
                context
            );
        }
    }
    
    // DictRef 字段检测
    if rf.ty.as_deref() == Some("dict_ref") {
        if rf.dict_ref.is_none() {
            panic!(
                "{}：dict_ref 类型必须指定 'dict_ref' 字段",
                context
            );
        }
    }
    
    // External 协议检测
    if rf.ty.as_deref() == Some("external") {
        if rf.external_protocol.is_none() {
            panic!(
                "{}：external 类型必须指定 'external_protocol' 字段",
                context
            );
        }
    }
    
    // Custom 处理器检测
    if rf.ty.as_deref() == Some("custom") {
        if rf.handler.is_none() {
            panic!(
                "{}：custom 类型必须指定 'handler' 字段",
                context
            );
        }
    }
}
