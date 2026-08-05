//! 代码生成器模块
//!
//! 负责将 YAML AST (RawField/RawTemplate/RawDict) 转换为运行时的 FieldSpec 结构。

use crate::ast::{RawField, RawTemplate, RawCaseTarget, RawFormat, DEFAULT_REGION};
use crate::context::{BuildCtx, BuildScope};
use crate::types::*;
use crate::repeat::{format_id_expr, format_repeat_name};
use std::collections::HashMap;

// ============================================================================
// 原始 ID 收集（用于构建 di_raw_map）
// ============================================================================

/// 递归收集字段树中的所有 id，建立 (id, protocol, region, dir) -> RawField 映射
pub fn collect_raw_ids(
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

// ============================================================================
// 辅助函数：字段属性解析
// ============================================================================


pub fn effective_protocol<'a>(rf: &'a RawField, parent_protocol: &'a str) -> &'a str {
    rf.protocol.as_deref().unwrap_or(parent_protocol)
}

pub fn effective_region<'a>(rf: &'a RawField, parent_region: &'a str) -> &'a str {
    rf.region
        .as_ref()
        .and_then(|regions| regions.first().map(|s| s.as_str()))
        .unwrap_or(parent_region)
}

pub fn effective_dir<'a>(rf: &'a RawField, parent_dir: Option<&'a str>) -> Option<&'a str> {
    rf.dir.as_deref().or(parent_dir)
}

pub fn field_label(rf: &RawField) -> String {
    rf.name
        .clone()
        .or_else(|| rf.id.clone())
        .unwrap_or_else(|| "<unnamed field>".to_string())
}

// 解析 format 字段（运行时用于展示格式），简单的表达式支持：
// - 支持裸类型名称（"hex"/"bcd"/"bin"）或者 key=value 列表，
//   用逗号分隔，key 可以是 type/group_bytes/separator/pad/endian/order
pub fn parse_format_string(s: &str) -> Option<FormatSpec> {
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

pub fn parse_format_spec(rf: &RawField) -> Option<FormatSpec> {
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

pub fn gen_named_field(
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

pub fn gen_field(
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
                count: None,
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

pub fn get_len(rf: &RawField) -> usize {
    match &rf.length {
        Some(serde_yaml::Value::Number(n)) => n
            .as_u64()
            .unwrap_or_else(|| panic!("字段 {:?} 的 length 必须是非负整数", rf.name))
            as usize,
        other => panic!("字段 {:?} 需要一个整数 length，实际是 {:?}", rf.name, other),
    }
}

pub fn resolve_field_length(rf: &RawField) -> FieldLength {
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

pub fn resolve_endian(rf: &RawField) -> Endian {
    match rf.endian.as_deref() {
        Some("big") => Endian::Big,
        Some("little") | None => Endian::Little,
        Some(other) => panic!("未知 endian: {}（字段: {:?}）", other, rf.name),
    }
}

pub fn validate_id_ref(rf: &RawField, ref_name: &str, kind: &str, scope: &BuildScope) {
    if !scope.contains_ref_id(ref_name) {
        panic!(
            "字段 {:?} 的 {} 引用了未知 ref_id: {:?}，引用必须使用字段 ref_id",
            rf,
            kind,
            ref_name
        );
    }
}

pub fn resolve_dict_ref(rf: &RawField) -> Option<String> {
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

pub fn validate_field_refs(rf: &RawField, scope: &BuildScope) {
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

pub fn gen_fixed(rf: &RawField, ty: &str) -> FieldSpec {
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
        "bin" => {
            let decimals = rf.decimals.unwrap_or(0);
            Encoding::Bin {
                endian: resolve_endian(rf),
                signed,
                decimals,
            }
        }
        "bcd" => {
            let decimals = rf.decimals.unwrap_or(0);
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

pub fn gen_bitfield(rf: &RawField) -> FieldSpec {
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

pub fn build_bit_specs(rf: &RawField, length: usize) -> Vec<BitSpec> {
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

pub fn gen_bitmask(
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

pub fn resolve_switch_target(
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

pub fn gen_switch(
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

pub fn gen_repeat(
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
    let count_value = rf.count;
    
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
        count: count_value,
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

pub fn lookup_template<'a>(
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

pub fn template_exists(
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

pub fn gen_template_ref(
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

pub fn gen_container_from_fields(
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

pub fn gen_external(rf: &RawField) -> FieldSpec {
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
pub fn lookup_raw<'a>(
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

pub fn gen_di_sequence(
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

pub fn gen_dict_ref(rf: &RawField) -> FieldSpec {
    let di_ref = resolve_dict_ref(rf).unwrap_or_else(|| {
        panic!(
            "dict_ref 字段 {:?} 缺少 dict_ref: {{ ref_id: ... }}",
            rf.name
        )
    });
    FieldSpec::DictRef { di_ref }
}

pub fn gen_custom(rf: &RawField) -> FieldSpec {
    let handler = rf
        .handler
        .clone()
        .unwrap_or_else(|| panic!("custom 字段 {:?} 缺少 handler", rf.name));
    FieldSpec::Custom(handler)
}

/// `info_point` —— 信息点标识 DA（6.1.3），固定2字节，无需任何配置项。
/// 如果 YAML 里写了 length，仅做一次健全性检查（必须是2），避免手滑写错
/// 长度却因为字段本身不读取 length 而悄悄被忽略。
pub fn gen_info_point(rf: &RawField) -> FieldSpec {
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
pub fn gen_di_code(rf: &RawField) -> FieldSpec {
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
