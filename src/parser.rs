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
    decode_time, get_custom_handler, get_spec_catalog, get_external_parser,
    BitSpec, Context, DictError, Encoding, Endian, ExternalLength, FieldLength, FieldSpec,
    FormatSpec, NamedField, Value,
};
use crate::repeat::{eval_id_expr, format_id_expr, format_repeat_name};
use std::collections::HashMap;

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
            format,
        } => parse_fixed(buf, encoding, length, unit, enum_map, format, ctx),
        FieldSpec::BitField { length, bits } => parse_bitfield(buf, *length, bits),
        FieldSpec::Switch { on, cases, default } => {
            parse_switch(buf, on, cases, default, ctx, protocol, region, dir)
        }
        FieldSpec::Repeat {
            count_ref,
            bits_ref,
            bit_order,
            bit_specs,
            element,
            name_template,
            id_expr,
        } => parse_repeat(
            buf,
            count_ref,
            bits_ref,
            bit_order,
            bit_specs,
            element,
            name_template,
            id_expr,
            ctx,
            protocol,
            region,
            dir,
        ),
        FieldSpec::BitMask {
            length,
            bit_order,
            bit_specs,
            element,
            name_template,
        } => parse_bitmask(
            buf,
            *length,
            bit_order,
            bit_specs,
            element,
            name_template,
            ctx,
            protocol,
            region,
            dir,
        ),
        FieldSpec::Skip => parse_skip(),
        FieldSpec::External { protocol, length } => parse_external(buf, protocol, length, ctx),
        FieldSpec::Container(fields) => parse_container(buf, fields, ctx, protocol, region, dir),
        FieldSpec::Custom(handler) => parse_custom(buf, handler),
        FieldSpec::DictRef { di_ref } => parse_dict_ref(buf, di_ref, ctx, protocol, region, dir),
        FieldSpec::InfoPoint => parse_info_point(buf),
        FieldSpec::DiCode => parse_di_code(buf, protocol, region, dir),
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
            Value::Pn(i) => i.to_string(),
            Value::Skip => String::new(),
            Value::WithUnit { value, .. } | Value::Node { value, .. } => match value.as_ref() {
                Value::Int(i) => i.to_string(),
                Value::Float(f) => f.to_string(),
                Value::Str(s) => s.clone(),
                Value::Bytes(bytes) => decode_hex(bytes),
                Value::Pn(i) => i.to_string(),
                Value::Skip => String::new(),
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

fn extract_bits(raw: &[u8], range: (usize, usize)) -> u64 {
    let (start, end) = range;
    if end < start {
        return 0;
    }
    let mut result = 0u64;
    for bit_index in start..=end {
        let byte_index = bit_index / 8;
        let bit_in_byte = 7 - (bit_index % 8);
        if raw[byte_index] & (1 << bit_in_byte) != 0 {
            result |= 1 << (bit_index - start);
        }
    }
    result
}

fn extract_bits_ordered(raw: &[u8], range: (usize, usize), bit_order: Option<&str>) -> u64 {
    let (start, end) = range;
    if end < start {
        return 0;
    }
    let mut result = 0u64;
    for bit_index in start..=end {
        let byte_index = bit_index / 8;
        let bit_in_byte = bit_index % 8;
        let bit_position = if bit_order == Some("lsb") {
            bit_in_byte
        } else {
            7 - bit_in_byte
        };
        if raw[byte_index] & (1 << bit_position) != 0 {
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
    format: &Option<FormatSpec>,
    ctx: &Context,
) -> Result<(Value, usize), DictError> {
    let length = match length {
        FieldLength::Fixed(len) => *len,
        FieldLength::Ref(name) => ctx
            .get_decoded(name)
            .and_then(|v| v.as_usize())
            .ok_or_else(|| DictError::MissingRef(name.clone()))?,
        FieldLength::Expr(expr) => {
            // evaluate expression which may contain ref(<ref_id>) and optional index/index0
            let eval = eval_length_expr(expr, ctx, None).map_err(|e| {
                DictError::ExternalParseError(format!("length expr eval error: {}", e))
            })?;
            eval as usize
        }
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
            if let Some(spec) = format {
                return Ok((Value::Str(crate::types::format_bytes_with_spec(raw, spec)), length));
            }
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
        Encoding::Hex => {
            if let Some(spec) = format {
                Value::Str(crate::types::format_bytes_with_spec(raw, spec))
            } else {
                Value::Str(raw_hex.clone())
            }
        }
        Encoding::Time { format, encoding } => Value::Str(decode_time(raw, format, *encoding)),
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
        if bit.range.1 >= total_bits {
            return Err(DictError::UnexpectedEof {
                needed: (bit.range.1 / 8) + 1,
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

fn parse_skip() -> Result<(Value, usize), DictError> {
    Ok((Value::Skip, 0))
}

fn parse_bitmask(
    buf: &[u8],
    length: usize,
    bit_order: &Option<String>,
    bit_specs: &[BitSpec],
    element: &FieldSpec,
    name_template: &Option<String>,
    ctx: &mut Context,
    protocol: &str,
    region: &str,
    dir: Option<&str>,
) -> Result<(Value, usize), DictError> {
    if buf.len() < length {
        return Err(DictError::UnexpectedEof {
            needed: length,
            available: buf.len(),
        });
    }
    let raw = &buf[..length];
    let mut items = Vec::with_capacity(bit_specs.len());
    let mut bit_specs_sorted: Vec<&BitSpec> = bit_specs.iter().collect();
    if bit_order.as_deref() == Some("msb") {
        bit_specs_sorted.sort_by_key(|bit| std::cmp::Reverse(bit.range.0));
    } else {
        bit_specs_sorted.sort_by_key(|bit| bit.range.0);
    }
    let bit_count = bit_specs_sorted.len();
    let mut offset = 0usize;
    for (idx, bit_spec) in bit_specs_sorted.iter().enumerate() {
        let bit_value = extract_bits_ordered(raw, bit_spec.range, bit_order.as_deref());
        ctx.push_scope();
        ctx.bind(
            "bit_value",
            vec![bit_value as u8],
            Value::Int(bit_value as i64),
        );
        ctx.bind(
            "bit_index",
            bit_spec.range.0.to_le_bytes().to_vec(),
            Value::Int(bit_spec.range.0 as i64),
        );
        ctx.bind(
            "bit_name",
            bit_spec.name.as_bytes().to_vec(),
            Value::Str(bit_spec.name.clone()),
        );
        if let Some(bit_ref) = &bit_spec.ref_id {
            ctx.bind(
                "bit_ref",
                bit_ref.as_bytes().to_vec(),
                Value::Str(bit_ref.clone()),
            );
        }

        let instantiated_element = instantiate_field_spec(element, idx, bit_count);
        let (v, consumed) = parse_field(&buf[offset..], &instantiated_element, ctx, protocol, region, dir)?;
        ctx.pop_scope();
        if let Value::Skip = v {
            offset += consumed;
            continue;
        }

        let item = if let Some(template) = name_template {
            Value::Node {
                name: format_repeat_name(
                    Some(template.as_str()),
                    None,
                    Some(bit_spec.name.as_str()),
                    bit_spec.ref_id.as_deref(),
                    idx,
                    bit_count,
                ),
                raw: buf[offset..offset + consumed].to_vec(),
                value: Box::new(v),
            }
        } else {
            v
        };
        items.push(item);
        offset += consumed;
    }
    Ok((Value::List(items), length))
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

/// 信息点标识 DA（6.1.3）：2字节，DA1(测量点位掩码)+DA2(测量点组号)。
///
/// - DA1=00H 且 DA2=00H → 终端测量点 p0
/// - DA1=FFH 且 DA2=FFH → 除终端测量点外的所有测量点
/// - 其余情况：DA1 的第 i 位(D0..D7, i=0..7，LSB在前)对应
///   p((DA2-1)*8 + i + 1)；同一个DA里可以同时命中多个测量点
///   （如 DA2=01H,DA1=03H → p1、p2 同时命中），因此解析结果始终是
///   `Value::List`（命中0个也是空 List，不是错误——理论上不该出现但
///   不必因此拒绝解析，交给上层按需校验）。
fn parse_info_point(buf: &[u8]) -> Result<(Value, usize), DictError> {
    if buf.len() < 2 {
        return Err(DictError::UnexpectedEof {
            needed: 2,
            available: buf.len(),
        });
    }
    let da1 = buf[0];
    let da2 = buf[1];

    let value = if da1 == 0x00 && da2 == 0x00 {
        Value::Str("p0（终端测量点）".to_string())
    } else if da1 == 0xFF && da2 == 0xFF {
        Value::Str("除终端测量点外的所有测量点".to_string())
    } else {
        let points: Vec<Value> = (0..8u32)
            .filter(|i| (da1 as u32 >> i) & 1 == 1)
            .map(|i| {
                let n = ((da2 as i64 - 1) * 8) + i as i64 + 1;
                Value::Pn(n)
            })
            .collect();
        Value::List(points)
    };

    Ok((value, 2))
}

/// 数据标识编码 DI（6.1.4）：固定4字节，传输顺序 DI0,DI1,DI2,DI3（小端），
/// 按小端读出32位数值后即为字典里 `id:` 对应的 DI 码，去同协议 DI 字典查
/// 名称。只解析"标识本身"，不递归解析该 DI 的数据内容——含数据内容的场景
/// （上行报文 DI 后面紧跟 DATA）不适用这个类型，那种场景每个 DA/DI 在报文
/// 里各自独立出现，应该用 `dict_ref` 或显式拼两个字段表达，不要复用这里。
///
/// 找不到对应 DI 时不中断整体解析——枚举出现未登记的 DI 在实际报文里是
/// 正常情况（字典总有没收录全的时候），返回 `"DI码_未知数据标识"` 而不是
/// 报错，避免一个陌生 DI 拖垮整条 BASETASK 的解析。
fn parse_di_code(
    buf: &[u8],
    protocol: &str,
    region: &str,
    dir: Option<&str>,
) -> Result<(Value, usize), DictError> {
    if buf.len() < 4 {
        return Err(DictError::UnexpectedEof {
            needed: 4,
            available: buf.len(),
        });
    }
    let di = decode_bin_u64(&buf[..4], Endian::Little) as u32;
    let table = get_spec_catalog();
    let label = match lookup_di(table, protocol, di, region, dir) {
        Ok(entry) if !entry.name.is_empty() => format!("{:08X}_{}", di, entry.name),
        Ok(entry) => entry
            .id
            .clone()
            .unwrap_or_else(|| format!("{:08X}", di)),
        Err(_) => format!("{:08X}_未知数据标识", di),
    };
    Ok((Value::Str(label), 4))
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
    } else if on.starts_with('$') {
        let var_name = &on[1..];
        let value = ctx
            .get_decoded(var_name)
            .ok_or_else(|| DictError::MissingRef(var_name.to_string()))?;
        match value {
            Value::Int(i) => i.to_string(),
            Value::Str(s) => s.clone(),
            Value::WithUnit { value, .. } => value.as_str().map(|s| s.to_string()).unwrap_or_else(|| {
                value.as_int().map(|i| i.to_string()).unwrap_or_default()
            }),
            Value::Node { value, .. } => value.as_str().map(|s| s.to_string()).unwrap_or_else(|| {
                value.as_int().map(|i| i.to_string()).unwrap_or_default()
            }),
            Value::Bytes(bytes) => decode_hex(bytes),
            _ => {
                return Err(DictError::UnknownSwitchCase {
                    on: on.to_string(),
                    key: String::new(),
                })
            }
        }
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

fn format_repeat_template(template: &str, idx: usize, count: usize) -> String {
    template
        .replace("{index0}", &idx.to_string())
        .replace("{index}", &(idx + 1).to_string())
        .replace("{count}", &count.to_string())
}

fn eval_length_expr(expr: &str, ctx: &Context, idx_opt: Option<usize>) -> Result<u32, String> {
    struct Parser<'a> {
        input: &'a str,
        pos: usize,
        idx_opt: Option<usize>,
        ctx: &'a Context,
    }

    impl<'a> Parser<'a> {
        fn new(input: &'a str, idx_opt: Option<usize>, ctx: &'a Context) -> Self {
            Parser { input, pos: 0, idx_opt, ctx }
        }

        fn peek(&self) -> Option<char> {
            self.input[self.pos..].chars().next()
        }

        fn next(&mut self) -> Option<char> {
            if let Some(ch) = self.peek() {
                self.pos += ch.len_utf8();
                Some(ch)
            } else {
                None
            }
        }

        fn skip_ws(&mut self) {
            while matches!(self.peek(), Some(ch) if ch.is_whitespace()) {
                self.next();
            }
        }

        fn parse_number(&mut self) -> Result<u32, String> {
            self.skip_ws();
            let start = self.pos;
            if self.input[self.pos..].starts_with("0x") || self.input[self.pos..].starts_with("0X") {
                self.pos += 2;
                while matches!(self.peek(), Some(ch) if ch.is_ascii_hexdigit()) {
                    self.next();
                }
                let token = &self.input[start + 2..self.pos];
                u32::from_str_radix(token, 16).map_err(|_| format!("非法十六进制数: {}", token))
            } else {
                while matches!(self.peek(), Some(ch) if ch.is_ascii_digit()) {
                    self.next();
                }
                let token = &self.input[start..self.pos];
                token
                    .parse::<u32>()
                    .map_err(|_| format!("非法十进制数: {}", token))
            }
        }

        fn parse_variable_or_ref(&mut self) -> Result<u32, String> {
            self.skip_ws();
            // detect ref(name)
            if self.input[self.pos..].starts_with("ref(") {
                self.pos += 4; // skip "ref("
                let start = self.pos;
                while matches!(self.peek(), Some(ch) if ch != ')') {
                    self.next();
                }
                if self.peek() != Some(')') {
                    return Err("未闭合的 ref(...)".to_string());
                }
                let name = &self.input[start..self.pos];
                self.next(); // consume ')'
                // lookup in ctx
                let v = self
                    .ctx
                    .get_decoded(name)
                    .ok_or_else(|| format!("缺少引用字段: {}", name))?;
                v.as_u32().ok_or_else(|| format!("ref({}) 不是可用的非负整数", name))
            } else {
                // variable like index or index0
                let start = self.pos;
                while matches!(self.peek(), Some(ch) if ch.is_ascii_alphanumeric() || ch == '_') {
                    self.next();
                }
                let token = &self.input[start..self.pos];
                match token {
                    "index" => {
                        if let Some(idx) = self.idx_opt {
                            Ok((idx + 1) as u32)
                        } else {
                            Err("index 仅在 repeat 上下文可用".to_string())
                        }
                    }
                    "index0" => {
                        if let Some(idx) = self.idx_opt {
                            Ok(idx as u32)
                        } else {
                            Err("index0 仅在 repeat 上下文可用".to_string())
                        }
                    }
                    _ => Err(format!("非法变量或 ref: {}", token)),
                }
            }
        }

        fn parse_factor(&mut self) -> Result<u32, String> {
            self.skip_ws();
            if self.input[self.pos..].starts_with('(') {
                self.next();
                let value = self.parse_expr()?;
                self.skip_ws();
                if self.next() != Some(')') {
                    return Err("缺少 )".to_string());
                }
                Ok(value)
            } else if self.input[self.pos..].starts_with("ref(") {
                self.parse_variable_or_ref()
            } else if self.input[self.pos..].starts_with("index0") || self.input[self.pos..].starts_with("index") {
                self.parse_variable_or_ref()
            } else if matches!(self.peek(), Some(ch) if ch.is_ascii_digit()) {
                self.parse_number()
            } else {
                Err(format!("非法表达式起始: {}", self.input[self.pos..].chars().next().unwrap_or('?')))
            }
        }

        fn parse_term(&mut self) -> Result<u32, String> {
            let mut value = self.parse_factor()? as u64;
            loop {
                self.skip_ws();
                match self.peek() {
                    Some('*') => {
                        self.next();
                        let rhs = self.parse_factor()? as u64;
                        value = value.checked_mul(rhs).ok_or_else(|| "溢出".to_string())?;
                    }
                    Some('/') => {
                        self.next();
                        let rhs = self.parse_factor()? as u64;
                        if rhs == 0 {
                            return Err("除以零".to_string());
                        }
                        value = value.checked_div(rhs).ok_or_else(|| "溢出".to_string())?;
                    }
                    _ => break,
                }
            }
            Ok(value as u32)
        }

        fn parse_expr(&mut self) -> Result<u32, String> {
            let mut value = self.parse_term()? as i64;
            loop {
                self.skip_ws();
                match self.peek() {
                    Some('+') => {
                        self.next();
                        let rhs = self.parse_term()? as i64;
                        value = value.checked_add(rhs).ok_or_else(|| "溢出".to_string())?;
                    }
                    Some('-') => {
                        self.next();
                        let rhs = self.parse_term()? as i64;
                        value = value.checked_sub(rhs).ok_or_else(|| "溢出".to_string())?;
                    }
                    _ => break,
                }
            }
            if value < 0 || value > u32::MAX as i64 {
                Err("表达式结果越界".to_string())
            } else {
                Ok(value as u32)
            }
        }
    }

    let mut parser = Parser::new(expr, idx_opt, ctx);
    let result = parser.parse_expr()?;
    parser.skip_ws();
    if parser.peek().is_some() {
        return Err(format!("未处理的表达式尾部: {}", &expr[parser.pos..]));
    }
    Ok(result)
}

fn instantiate_named_field(nf: &NamedField, idx: usize, count: usize) -> NamedField {
    NamedField {
        id: nf.id.as_ref().map(|s| format_repeat_template(s, idx, count)),
        ref_id: nf
            .ref_id
            .as_ref()
            .map(|s| format_repeat_template(s, idx, count)),
        name: format_repeat_template(&nf.name, idx, count),
        spec: instantiate_field_spec(&nf.spec, idx, count),
        format: nf.format.clone(),
    }
}

fn instantiate_field_spec(spec: &FieldSpec, idx: usize, count: usize) -> FieldSpec {
    match spec {
        FieldSpec::Fixed {
            encoding,
            length,
            unit,
            enum_map,
            format,
        } => FieldSpec::Fixed {
            encoding: encoding.clone(),
            length: length.clone(),
            unit: unit.clone(),
            enum_map: enum_map.clone(),
            format: format.clone(),
        },
        FieldSpec::BitField { length, bits } => FieldSpec::BitField {
            length: *length,
            bits: bits.clone(),
        },
        FieldSpec::Switch { on, cases, default } => FieldSpec::Switch {
            on: format_repeat_template(on, idx, count),
            cases: cases
                .iter()
                .map(|(k, v)| {
                    (
                        format_repeat_template(k, idx, count),
                        Box::new(instantiate_field_spec(v, idx, count)),
                    )
                })
                .collect(),
            default: default.as_ref().map(|v| Box::new(instantiate_field_spec(v, idx, count))),
        },
        FieldSpec::Repeat {
            count_ref,
            bits_ref,
            bit_order,
            bit_specs,
            element,
            name_template,
            id_expr,
        } => FieldSpec::Repeat {
            count_ref: count_ref.as_ref().map(|s| format_repeat_template(s, idx, count)),
            bits_ref: bits_ref.clone(),
            bit_order: bit_order.clone(),
            bit_specs: bit_specs.clone(),
            element: Box::new(instantiate_field_spec(element, idx, count)),
            name_template: name_template
                .as_ref()
                .map(|tmpl| format_repeat_template(tmpl, idx, count)),
            id_expr: id_expr.clone(),
        },
        FieldSpec::External { protocol, length } => FieldSpec::External {
            protocol: protocol.clone(),
            length: length.clone(),
        },
        FieldSpec::BitMask {
            length,
            bit_order,
            bit_specs,
            element,
            name_template,
        } => FieldSpec::BitMask {
            length: *length,
            bit_order: bit_order.clone(),
            bit_specs: bit_specs.clone(),
            element: Box::new(instantiate_field_spec(element, idx, count)),
            name_template: name_template
                .as_ref()
                .map(|tmpl| format_repeat_template(tmpl, idx, count)),
        },
        FieldSpec::Skip => FieldSpec::Skip,
        FieldSpec::Container(fields) => FieldSpec::Container(
            fields
                .iter()
                .map(|nf| instantiate_named_field(nf, idx, count))
                .collect(),
        ),
        FieldSpec::Custom(handler) => FieldSpec::Custom(handler.clone()),
        FieldSpec::DictRef { di_ref } => FieldSpec::DictRef {
            di_ref: format_repeat_template(di_ref, idx, count),
        },
        // 无字段的单元变体，repeat 展开时原样复制即可，不涉及任何模板替换
        FieldSpec::InfoPoint => FieldSpec::InfoPoint,
        FieldSpec::DiCode => FieldSpec::DiCode,
    }
}

fn parse_repeat(
    buf: &[u8],
    count_ref: &Option<String>,
    bits_ref: &Option<String>,
    bit_order: &Option<String>,
    bit_specs: &Option<Vec<BitSpec>>,
    element: &FieldSpec,
    name_template: &Option<String>,
    id_expr: &Option<String>,
    ctx: &mut Context,
    protocol: &str,
    region: &str,
    dir: Option<&str>,
) -> Result<(Value, usize), DictError> {
    let mut offset = 0usize;
    let mut items = Vec::new();

    if let Some(count_ref) = count_ref {
        let count = ctx
            .get_decoded(count_ref)
            .and_then(|v| v.as_usize())
            .ok_or_else(|| DictError::MissingRef(count_ref.to_string()))?;
        for idx in 0..count {
            let instantiated_element = instantiate_field_spec(element, idx, count);
            let (v, consumed) = parse_field(&buf[offset..], &instantiated_element, ctx, protocol, region, dir)?;
            if let Value::Skip = v {
                offset += consumed;
                continue;
            }
            let item = if let Some(template) = name_template {
                let id_value = id_expr
                    .as_ref()
                    .and_then(|expr| eval_id_expr(expr, idx).ok())
                    .map(|id| format!("{:08X}", id));
                Value::Node {
                    name: format_repeat_name(
                        Some(template.as_str()),
                        id_value.as_deref(),
                        None,
                        None,
                        idx,
                        count,
                    ),
                    raw: buf[offset..offset + consumed].to_vec(),
                    value: Box::new(v),
                }
            } else if let Some(expr) = id_expr {
                let name = format_id_expr(expr, idx).unwrap_or_else(|e| {
                    panic!("id_expr 解析失败: {} (idx={})", e, idx)
                });
                Value::Node {
                    name,
                    raw: buf[offset..offset + consumed].to_vec(),
                    value: Box::new(v),
                }
            } else {
                v
            };
            items.push(item);
            offset += consumed;
        }
        return Ok((Value::List(items), offset));
    }

    let bits_ref = bits_ref
        .as_ref()
        .ok_or_else(|| DictError::MissingRef("repeat 字段缺少 count_ref 或 bits_ref".to_string()))?;
    let specs = bit_specs.as_ref().ok_or_else(|| {
        DictError::MissingRef(format!(
            "repeat 字段 {:?} 的 bits_ref {:?} 未提供 bit_specs",
            bits_ref, bits_ref
        ))
    })?;
    let raw = ctx
        .get_raw(bits_ref)
        .cloned()
        .ok_or_else(|| DictError::MissingRef(bits_ref.to_string()))?;
    let mut bit_specs_sorted: Vec<&BitSpec> = specs.iter().collect();
    if bit_order.as_deref() == Some("desc") {
        bit_specs_sorted.sort_by_key(|bit| std::cmp::Reverse(bit.range.0));
    } else {
        bit_specs_sorted.sort_by_key(|bit| bit.range.0);
    }

    let bit_count = bit_specs_sorted.len();
    for (idx, bit_spec) in bit_specs_sorted.iter().enumerate() {
        let bit_value = extract_bits(&raw, bit_spec.range);
        ctx.push_scope();
        ctx.bind(
            "bit_value",
            vec![bit_value as u8],
            Value::Int(bit_value as i64),
        );
        ctx.bind(
            "bit_index",
            bit_spec.range.0.to_le_bytes().to_vec(),
            Value::Int(bit_spec.range.0 as i64),
        );
        ctx.bind(
            "bit_name",
            bit_spec.name.as_bytes().to_vec(),
            Value::Str(bit_spec.name.clone()),
        );
        if let Some(bit_ref) = &bit_spec.ref_id {
            ctx.bind(
                "bit_ref",
                bit_ref.as_bytes().to_vec(),
                Value::Str(bit_ref.clone()),
            );
        }

        let instantiated_element = instantiate_field_spec(element, idx, bit_count);
        let (v, consumed) = parse_field(&buf[offset..], &instantiated_element, ctx, protocol, region, dir)?;
        if let Value::Skip = v {
            ctx.pop_scope();
            offset += consumed;
            continue;
        }
        // 如果该 case 长度为 0（consumed == 0），则视为不需要产生解析结果（例如 switch 的 "0" 分支），
        // 此时不创建节点也不加入 items，只弹出作用域并继续下一个 bit。
        if consumed == 0 {
            ctx.pop_scope();
            continue;
        }

        let item = if let Some(template) = name_template {
            Value::Node {
                name: format_repeat_name(
                    Some(template.as_str()),
                    None,
                    Some(bit_spec.name.as_str()),
                    bit_spec.ref_id.as_deref(),
                    idx,
                    bit_specs_sorted.len(),
                ),
                raw: buf[offset..offset + consumed].to_vec(),
                value: Box::new(v),
            }
        } else {
            v
        };
        ctx.pop_scope();
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
        if let Value::Skip = v {
            offset += consumed;
            continue;
        }
        ctx.bind(&nf.name, raw_bytes.clone(), v.clone());
        if let Some(id) = &nf.id {
            ctx.bind(id, raw_bytes.clone(), v.clone());
        }
        if let Some(ref_id) = &nf.ref_id {
            ctx.bind(ref_id, raw_bytes.clone(), v.clone());
        }
        if nf.name.is_empty() {
            match v {
                Value::List(items) => {
                    for item in items {
                        if let Value::Node { name, .. } = &item {
                            entries.push((name.clone(), item));
                        } else {
                            entries.push((String::new(), item));
                        }
                    }
                    offset += consumed;
                    continue;
                }
                Value::Node { ref name, .. } => {
                    entries.push((name.clone(), v));
                    offset += consumed;
                    continue;
                }
                _ => {}
            }
        }

        let node_name = if let Some(id) = &nf.id {
            format!("{}_{}", id, nf.name)
        } else {
            nf.name.clone()
        };
        let node = Value::Node {
            name: node_name.clone(),
            raw: raw_bytes.clone(),
            value: Box::new(v.clone()),
        };
        entries.push((node_name, node));
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

#[cfg(test)]
mod info_point_tests {
    use super::*;

    #[test]
    fn single_point_in_first_group() {
        // DA2=01H, DA1=01H → p1
        let (value, consumed) = parse_info_point(&[0x01, 0x01]).unwrap();
        assert_eq!(consumed, 2);
        assert_eq!(value, Value::List(vec![Value::Pn(1)]));
    }

    #[test]
    fn multiple_points_in_same_group() {
        // 原文举例：DA2=01H, DA1=03H → p1、p2 同时命中
        let (value, consumed) = parse_info_point(&[0x03, 0x01]).unwrap();
        assert_eq!(consumed, 2);
        assert_eq!(value, Value::List(vec![Value::Pn(1), Value::Pn(2)]));
    }

    #[test]
    fn point_in_second_group_offsets_by_eight() {
        // DA2=02H, DA1=01H → p9 (D0 对应 p((2-1)*8+1) = p9)
        let (value, _) = parse_info_point(&[0x01, 0x02]).unwrap();
        assert_eq!(value, Value::List(vec![Value::Pn(9)]));
    }

    #[test]
    fn all_zero_means_terminal_point_p0() {
        let (value, _) = parse_info_point(&[0x00, 0x00]).unwrap();
        assert_eq!(value, Value::Str("p0（终端测量点）".to_string()));
    }

    #[test]
    fn all_ff_means_all_points_except_terminal() {
        let (value, _) = parse_info_point(&[0xFF, 0xFF]).unwrap();
        assert_eq!(value, Value::Str("除终端测量点外的所有测量点".to_string()));
    }

    #[test]
    fn rejects_short_buffer() {
        let err = parse_info_point(&[0x01]).unwrap_err();
        assert!(matches!(
            err,
            DictError::UnexpectedEof { needed: 2, available: 1 }
        ));
    }
}

#[cfg(test)]
mod di_code_tests {
    use super::*;

    #[test]
    fn resolves_known_di_to_name() {
        // di = 0x00010001，传输顺序(小端) DI0,DI1,DI2,DI3 = 01 00 01 00
        let (value, consumed) =
            parse_di_code(&[0x01, 0x00, 0x01, 0x00], "csg13", "南网", None).unwrap();
        assert_eq!(consumed, 4);
        match value {
            Value::Str(s) => assert_eq!(s, "00010001_月冻结正向有功总电能"),
            other => panic!("unexpected value shape: {other:?}"),
        }
    }

    #[test]
    fn falls_back_gracefully_for_unknown_di() {
        let (value, consumed) =
            parse_di_code(&[0xEF, 0xBE, 0xAD, 0xDE], "csg13", "南网", None).unwrap();
        assert_eq!(consumed, 4);
        match value {
            Value::Str(s) => assert!(s.contains("未知数据标识")),
            other => panic!("unexpected value shape: {other:?}"),
        }
    }

    #[test]
    fn rejects_short_buffer() {
        let err = parse_di_code(&[0x01, 0x02], "csg13", "南网", None).unwrap_err();
        assert!(matches!(
            err,
            DictError::UnexpectedEof { needed: 4, available: 2 }
        ));
    }
}
