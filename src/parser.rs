//! 运行时解析器
//!
//! ## 协议（protocol）/ 省份（region）/ 方向（dir）
//!
//! DI 表的 key 是 `(protocol, di, region, dir)`：
//! - protocol 硬边界，绝不回退；
//! - region 允许回退到 DEFAULT_REGION；
//! - dir 允许回退到 None（跟方向无关的通用定义），只有显式写了 dir 的
//!   条目才要求查询方向精确匹配。

use super::{
    decode_ascii, decode_bcd_u64, decode_bin_u64, decode_hex, decode_signed_bcd, decode_signed_bin,
    decode_time, get_custom_handler, get_spec_catalog, get_external_parser, BitSpec, Context,
    DictError, Encoding, ExternalLength, FieldLength, FieldSpec, NamedField, Value,
};
use std::collections::HashMap;

fn format_repeat_name(template: &str, idx: usize, count: usize) -> String {
    let replaced_index0 = template.replace("{index0}", &idx.to_string());
    let replaced_index = replaced_index0.replace("{index}", &(idx + 1).to_string());
    replaced_index.replace("{count}", &count.to_string())
}

pub const DEFAULT_REGION: &str = "default";

pub fn parse_field(
    buf: &[u8],
    spec: &FieldSpec,
    ctx: &mut Context,
    protocol: &str,
    region: &str,
    dir: Option<&str>,
) -> Result<(Value, usize), DictError> {
    match spec {
        FieldSpec::Fixed {
            encoding,
            length,
            unit,
            enum_map,
        } => parse_fixed(buf, encoding, length, unit, enum_map, ctx),
        FieldSpec::BitField { length, bits } => parse_bitfield(buf, *length, bits),
        FieldSpec::Switch { on, cases, default } => {
            parse_switch(buf, on, cases, default, ctx, protocol, region, dir)
        }
        FieldSpec::Repeat {
            count_ref,
            element,
            name_template,
            id_expr,
        } => parse_repeat(
            buf,
            count_ref,
            element,
            name_template,
            id_expr,
            ctx,
            protocol,
            region,
            dir,
        ),
        FieldSpec::External { protocol, length } => parse_external(buf, protocol, length, ctx),
        FieldSpec::Container(fields) => parse_container(buf, fields, ctx, protocol, region, dir),
        FieldSpec::Custom(handler) => parse_custom(buf, handler),
        FieldSpec::DictRef { di_ref } => parse_dict_ref(buf, di_ref, ctx, protocol, region, dir),
    }
}

pub fn parse_di(
    protocol: &str,
    di: u32,
    region: &str,
    dir: Option<&str>,
    buf: &[u8],
) -> Result<(Value, usize), DictError> {
    let table = get_spec_catalog();
    let entry = lookup_di(table, protocol, di, region, dir)?;
    let mut ctx = Context::new();
    let (value, consumed) = parse_field(buf, &entry.spec, &mut ctx, protocol, region, dir)?;
    let name = if !entry.name.is_empty() {
        format!("{:08X}_{}", di, entry.name)
    } else if let Some(id) = &entry.id {
        id.clone()
    } else {
        format!("{:08X}", di)
    };
    Ok((
        Value::Node {
            name,
            raw: buf[..consumed].to_vec(),
            value: Box::new(value),
        },
        consumed,
    ))
}

/// 查找顺序：1.精确(region,dir) 2.保留region,dir回退None
/// 3.region回退default,保留dir 4.两者都回退。protocol 全程精确匹配。
fn lookup_di<'a>(
    table: &'a std::collections::HashMap<(String, u32, String, Option<String>), NamedField>,
    protocol: &str,
    di: u32,
    region: &str,
    dir: Option<&str>,
) -> Result<&'a NamedField, DictError> {
    let dir_owned = dir.map(|s| s.to_string());
    if let Some(spec) = table.get(&(
        protocol.to_string(),
        di,
        region.to_string(),
        dir_owned.clone(),
    )) {
        return Ok(spec);
    }
    if dir_owned.is_some() {
        if let Some(spec) = table.get(&(protocol.to_string(), di, region.to_string(), None)) {
            return Ok(spec);
        }
    }
    if region != DEFAULT_REGION {
        if let Some(spec) = table.get(&(
            protocol.to_string(),
            di,
            DEFAULT_REGION.to_string(),
            dir_owned.clone(),
        )) {
            return Ok(spec);
        }
        if let Some(spec) = table.get(&(protocol.to_string(), di, DEFAULT_REGION.to_string(), None))
        {
            return Ok(spec);
        }
    }
    Err(DictError::UnknownDi {
        protocol: protocol.to_string(),
        di,
    })
}

fn apply_enum_map(value: Value, raw: &str, enum_map: &Option<HashMap<String, String>>) -> Value {
    if let Some(map) = enum_map {
        if let Some(mapped) = map.get(raw).cloned() {
            return Value::Str(mapped);
        }
        let value_key = match &value {
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Str(s) => s.clone(),
            Value::Bytes(bytes) => decode_hex(bytes),
            Value::WithUnit { value, .. } | Value::Node { value, .. } => match value.as_ref() {
                Value::Int(i) => i.to_string(),
                Value::Float(f) => f.to_string(),
                Value::Str(s) => s.clone(),
                Value::Bytes(bytes) => decode_hex(bytes),
                _ => String::new(),
            },
            Value::List(_) | Value::Map(_) => String::new(),
        };
        if let Some(mapped) = map.get(&value_key).cloned() {
            return Value::Str(mapped);
        }
    }
    value
}

fn extract_bits(raw: &[u8], range: (u8, u8)) -> u64 {
    let (start, end) = range;
    if end < start {
        return 0;
    }
    let mut result = 0u64;
    for bit_index in start..=end {
        let byte_index = (bit_index / 8) as usize;
        let bit_in_byte = 7 - (bit_index % 8);
        if raw[byte_index] & (1 << bit_in_byte) != 0 {
            result |= 1 << (bit_index - start);
        }
    }
    result
}

fn parse_fixed(
    buf: &[u8],
    encoding: &Encoding,
    length: &FieldLength,
    unit: &Option<String>,
    enum_map: &Option<HashMap<String, String>>,
    ctx: &Context,
) -> Result<(Value, usize), DictError> {
    let length = match length {
        FieldLength::Fixed(len) => *len,
        FieldLength::Ref(name) => ctx
            .get_decoded(name)
            .and_then(|v| v.as_usize())
            .ok_or_else(|| DictError::MissingRef(name.clone()))?,
    };
    if buf.len() < length {
        return Err(DictError::UnexpectedEof {
            needed: length,
            available: buf.len(),
        });
    }
    let raw = &buf[..length];
    let raw_hex = decode_hex(raw);
    let value = match encoding {
        Encoding::Bin { endian, signed } => {
            let int = if *signed {
                let (negative, cleared) = decode_signed_bin(raw, *endian);
                let decoded = decode_bin_u64(&cleared, *endian) as i64;
                if negative {
                    -decoded
                } else {
                    decoded
                }
            } else {
                decode_bin_u64(raw, *endian) as i64
            };
            Value::Int(int)
        }
        Encoding::Bcd {
            decimals,
            signed,
            endian,
        } => {
            let ordered_raw: Vec<u8> = if endian.is_some() {
                raw.to_vec()
            } else {
                raw.iter().rev().cloned().collect()
            };
            let (negative, cleared) = if *signed {
                decode_signed_bcd(&ordered_raw)
            } else {
                (false, ordered_raw)
            };
            let decoded = decode_bcd_u64(&cleared);
            if *decimals == 0 {
                let int_value = if negative {
                    -(decoded as i64)
                } else {
                    decoded as i64
                };
                Value::Int(int_value)
            } else {
                let divisor = 10u64.pow(*decimals as u32) as f64;
                let float_value = decoded as f64 / divisor;
                let value = if negative { -float_value } else { float_value };
                Value::Float(value)
            }
        }
        Encoding::Ascii => Value::Str(decode_ascii(raw)),
        Encoding::Hex => Value::Str(raw_hex.clone()),
        Encoding::Time { format } => Value::Str(decode_time(raw, format)),
        Encoding::Raw => Value::Bytes(raw.to_vec()),
    };
    let parsed = apply_enum_map(value, &raw_hex, enum_map);
    if let Some(unit) = unit {
        Ok((
            Value::WithUnit {
                value: Box::new(parsed),
                unit: unit.clone(),
            },
            length,
        ))
    } else {
        Ok((parsed, length))
    }
}

fn parse_bitfield(
    buf: &[u8],
    length: usize,
    bits: &[BitSpec],
) -> Result<(Value, usize), DictError> {
    if buf.len() < length {
        return Err(DictError::UnexpectedEof {
            needed: length,
            available: buf.len(),
        });
    }
    let raw = &buf[..length];
    let total_bits = raw.len() * 8;
    let mut entries = Vec::with_capacity(bits.len());
    for bit in bits {
        if (bit.range.1 as usize) >= total_bits {
            return Err(DictError::UnexpectedEof {
                needed: (bit.range.1 as usize / 8) + 1,
                available: raw.len(),
            });
        }
        let extracted = extract_bits(raw, bit.range);
        let raw_key = extracted.to_string();
        let value = if let Some(enum_map) = &bit.enum_map {
            if let Some(mapped) = enum_map.get(&raw_key) {
                Value::Str(mapped.clone())
            } else {
                Value::Int(extracted as i64)
            }
        } else {
            Value::Int(extracted as i64)
        };
        // format range as single or start-end
        let range_str = if bit.range.0 == bit.range.1 {
            format!("{}", bit.range.0)
        } else {
            format!("{}-{}", bit.range.0, bit.range.1)
        };
        let formatted_name = format!("BIT({})_{}", range_str, bit.name);
        // expose numeric bit value alongside the semantic value
        let bit_numeric = Value::Int(extracted as i64);
        let semantic = value.clone();
        let mut sub = Vec::with_capacity(2);
        sub.push(("bit".to_string(), bit_numeric));
        sub.push(("value".to_string(), semantic));
        entries.push((formatted_name, Value::Map(sub)));
    }
    Ok((Value::Map(entries), length))
}

fn parse_external(
    buf: &[u8],
    protocol: &str,
    length: &ExternalLength,
    ctx: &mut Context,
) -> Result<(Value, usize), DictError> {
    let parser = get_external_parser(protocol)
        .ok_or_else(|| DictError::UnknownProtocol(protocol.to_string()))?;
    let raw = match length {
        ExternalLength::Remaining => buf,
        ExternalLength::Fixed(len) => {
            if buf.len() < *len {
                return Err(DictError::UnexpectedEof {
                    needed: *len,
                    available: buf.len(),
                });
            }
            &buf[..*len]
        }
        ExternalLength::Ref(field_name) => {
            let count = ctx
                .get_decoded(field_name)
                .and_then(|v| v.as_u32())
                .ok_or_else(|| DictError::MissingRef(field_name.to_string()))?
                as usize;
            if buf.len() < count {
                return Err(DictError::UnexpectedEof {
                    needed: count,
                    available: buf.len(),
                });
            }
            &buf[..count]
        }
    };
    parser(raw)
        .map(|value| (value, raw.len()))
        .map_err(DictError::ExternalParseError)
}

fn parse_custom(buf: &[u8], handler: &str) -> Result<(Value, usize), DictError> {
    let custom = get_custom_handler(handler)
        .ok_or_else(|| DictError::UnknownHandler(handler.to_string()))?;
    custom(buf)
}

fn parse_switch(
    buf: &[u8],
    on: &str,
    cases: &std::collections::HashMap<String, Box<FieldSpec>>,
    default: &Option<Box<FieldSpec>>,
    ctx: &mut Context,
    protocol: &str,
    region: &str,
    dir: Option<&str>,
) -> Result<(Value, usize), DictError> {
    let key = if on == "$remaining" || on == "$len" || on == "$length" {
        buf.len().to_string()
    } else {
        let raw = ctx
            .get_raw(on)
            .cloned()
            .ok_or_else(|| DictError::MissingRef(on.to_string()))?;
        decode_hex(&raw)
    };
    let chosen: &FieldSpec = cases
        .get(&key)
        .map(|b| b.as_ref())
        .or_else(|| default.as_deref())
        .ok_or_else(|| DictError::UnknownSwitchCase {
            on: on.to_string(),
            key: key.clone(),
        })?;
    parse_field(buf, chosen, ctx, protocol, region, dir)
}

fn parse_repeat(
    buf: &[u8],
    count_ref: &str,
    element: &FieldSpec,
    name_template: &Option<String>,
    _id_expr: &Option<String>,
    ctx: &mut Context,
    protocol: &str,
    region: &str,
    dir: Option<&str>,
) -> Result<(Value, usize), DictError> {
    let count = ctx
        .get_decoded(count_ref)
        .and_then(|v| v.as_usize())
        .ok_or_else(|| DictError::MissingRef(count_ref.to_string()))?;
    let mut offset = 0usize;
    let mut items = Vec::new();
    for idx in 0..count {
        let (v, consumed) = parse_field(&buf[offset..], element, ctx, protocol, region, dir)?;
        let item = if let Some(template) = name_template {
            Value::Node {
                name: format_repeat_name(template, idx, count),
                raw: buf[offset..offset + consumed].to_vec(),
                value: Box::new(v),
            }
        } else {
            v
        };
        items.push(item);
        offset += consumed;
    }
    Ok((Value::List(items), offset))
}

// parse_external 内容不变（不需要 dir，省略）

fn parse_container(
    buf: &[u8],
    fields: &[NamedField],
    ctx: &mut Context,
    protocol: &str,
    region: &str,
    dir: Option<&str>,
) -> Result<(Value, usize), DictError> {
    ctx.push_scope();
    let mut offset = 0usize;
    let mut entries = Vec::with_capacity(fields.len());
    for nf in fields {
        let (v, consumed) = match parse_field(&buf[offset..], &nf.spec, ctx, protocol, region, dir)
        {
            Ok(ok) => ok,
            Err(e) => {
                ctx.pop_scope();
                return Err(e);
            }
        };
        let raw_bytes = buf[offset..offset + consumed].to_vec();
        // 先 bind 后 parse 允许后续字段引用前面字段；如果同一作用域内出现同名字段，
        // 后者会覆盖前者，符合最近匹配原则。
        ctx.bind(&nf.name, raw_bytes.clone(), v.clone());
        if let Some(id) = &nf.id {
            ctx.bind(id, raw_bytes.clone(), v.clone());
        }
        if let Some(ref_id) = &nf.ref_id {
            ctx.bind(ref_id, raw_bytes.clone(), v.clone());
        }
        let node_name = if let Some(id) = &nf.id {
            format!("{}_{}", id, nf.name)
        } else {
            nf.name.clone()
        };
        let node = Value::Node {
            name: node_name,
            raw: raw_bytes.clone(),
            value: Box::new(v.clone()),
        };
        entries.push((nf.name.clone(), node));
        offset += consumed;
    }
    ctx.pop_scope();
    Ok((Value::Map(entries), offset))
}

// parse_custom 内容不变（省略）

fn parse_dict_ref(
    buf: &[u8],
    di_ref: &str,
    ctx: &mut Context,
    protocol: &str,
    region: &str,
    dir: Option<&str>,
) -> Result<(Value, usize), DictError> {
    let di = ctx
        .get_decoded(di_ref)
        .and_then(|v| v.as_u32())
        .ok_or_else(|| DictError::MissingRef(di_ref.to_string()))?;
    let table = get_spec_catalog();
    let target = lookup_di(table, protocol, di, region, dir)?;
    parse_field(buf, &target.spec, ctx, protocol, region, dir)
}
