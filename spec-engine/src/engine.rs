//! 解析引擎核心模块
//!
//! 提供 `Engine` 类型和 `Catalog` 相关实现，支持不可变配置快照和原子切换。

use super::{
    decode_ascii, decode_bcd_u64, decode_bin_u64, decode_hex, decode_signed_bcd, decode_signed_bin,
    decode_time, get_custom_handler, get_external_parser, BitSpec,
    Encoding, Endian, ExternalLength, FieldLength, FormatSpec, get_spec_catalog,
};
use crate::context::Context;
use crate::dynamic_loader::{DiKey, Layer, DEFAULT_REGION};
use crate::error::DictError;
use crate::repeat::{eval_id_expr, format_id_expr, format_repeat_name};
use spec_compiler::types::{FieldSpec, NamedField, Value};
use std::collections::HashMap;
use std::sync::Arc;
/// 不可变的字典快照
///
/// 包含静态字典和动态层的完整配置。一旦创建就不可修改，保证线程安全的无锁读取。
#[derive(Clone)]
struct CatalogSnapshot {
    /// 静态字典（编译期嵌入）
    static_dict: &'static HashMap<DiKey, NamedField>,
    /// 动态层（按加载顺序）
    dynamic_layers: Arc<Vec<Layer>>,
}

impl CatalogSnapshot {
    /// 创建只包含静态字典的快照
    fn new_static(static_dict: &'static HashMap<DiKey, NamedField>) -> Self {
        Self {
            static_dict,
            dynamic_layers: Arc::new(Vec::new()),
        }
    }

    /// 创建包含动态层的快照
    fn new_with_layers(
        static_dict: &'static HashMap<DiKey, NamedField>,
        layers: Vec<Layer>,
    ) -> Self {
        Self {
            static_dict,
            dynamic_layers: Arc::new(layers),
        }
    }

    /// 查找 DI 定义（带层级和 region 回退）
    fn lookup(
        &self,
        protocol: &str,
        di: u32,
        region: &str,
        dir: Option<&str>,
    ) -> Option<&NamedField> {
        // 从最新的动态层开始查找
        for layer in self.dynamic_layers.iter().rev() {
            if let Some(field) = layer.lookup(protocol, di, region, dir) {
                return Some(field);
            }
        }

        // 回退到静态字典
        self.lookup_in_static(protocol, di, region, dir)
    }

    /// 在静态字典中查找（带 region 回退）
    fn lookup_in_static(
        &self,
        protocol: &str,
        di: u32,
        region: &str,
        dir: Option<&str>,
    ) -> Option<&NamedField> {
        // 1. 精确匹配：(protocol, di, region, dir)
        if let Some(field) = self.static_dict.get(&(
            protocol.to_string(),
            di,
            region.to_string(),
            dir.map(|s| s.to_string()),
        )) {
            return Some(field);
        }

        // 2. 忽略 dir：(protocol, di, region, None)
        if dir.is_some() {
            if let Some(field) =
                self.static_dict
                    .get(&(protocol.to_string(), di, region.to_string(), None))
            {
                return Some(field);
            }
        }

        // 3. 默认 region + dir：(protocol, di, DEFAULT_REGION, dir)
        if region != DEFAULT_REGION {
            if let Some(field) = self.static_dict.get(&(
                protocol.to_string(),
                di,
                DEFAULT_REGION.to_string(),
                dir.map(|s| s.to_string()),
            )) {
                return Some(field);
            }

            // 4. 默认 region：(protocol, di, DEFAULT_REGION, None)
            if let Some(field) =
                self.static_dict
                    .get(&(protocol.to_string(), di, DEFAULT_REGION.to_string(), None))
            {
                return Some(field);
            }
        }

        None
    }

    /// 获取动态层数量
    fn layer_count(&self) -> usize {
        self.dynamic_layers.len()
    }

    /// 获取动态层名称列表
    fn layer_names(&self) -> Vec<String> {
        self.dynamic_layers.iter().map(|l| l.name.clone()).collect()
    }
}

/// 解析引擎
///
/// 核心解析器类型，封装字典配置和解析逻辑。
///
/// # 设计特点
///
/// - **不可变**：一旦创建，配置不可修改（需要热更新时创建新 Engine）
/// - **轻量 Clone**：内部使用 `Arc`，Clone 只是增加引用计数
/// - **线程安全**：配置快照不可变，可安全地在多线程间共享
/// - **无锁读取**：解析时完全无锁，性能极致
///
/// # 使用示例
///
/// ## 基础使用（只用静态字典）
///
/// ```rust,ignore
/// use spec_engine::Engine;
///
/// let engine = Engine::new();
/// let (value, consumed) = engine.parse_simple("csg13", 0x00010000, &data)?;
/// ```
///
/// ## 使用配置创建
///
/// ```rust,ignore
/// use spec_engine::{Engine, EngineConfig};
///
/// let engine = EngineConfig::new()
///     .yaml_dir("/etc/app/config")
///     .build()?;
///
/// let (value, consumed) = engine.parse_simple("csg13", 0x00010000, &data)?;
/// ```
///
/// ## Clone 和共享
///
/// ```rust
/// # use spec_engine::Engine;
/// let engine = Engine::new_default();
/// let engine_clone = engine.clone();  // 轻量 Clone（只是 Arc::clone）
///
/// // 可以在多线程间共享
/// std::thread::spawn(move || {
///     // engine_clone 在新线程中使用
/// });
/// ```
#[derive(Clone)]
pub struct Engine {
    catalog: Arc<CatalogSnapshot>,
}

impl Engine {
    /// 创建使用内置静态字典的引擎
    ///
    /// 使用编译期嵌入的静态字典，不加载任何动态层。
    /// 这是最简单的使用方式。
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// use spec_engine::Engine;
    ///
    /// let engine = Engine::new_default();
    /// let (value, consumed) = engine.parse("csg13", 0x00010000, "南网", None, &data)?;
    /// ```
    pub fn new_default() -> Self {
        Self::new(get_spec_catalog())
    }

    /// 创建只使用静态字典的引擎
    ///
    /// 使用提供的静态字典，不加载任何动态层。
    ///
    /// # 参数
    ///
    /// - `static_dict`: 静态字典引用（通常来自 `get_spec_catalog()`）
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// use spec_engine::Engine;
    ///
    /// // 内部使用
    /// let engine = Engine::new(static_dict);
    /// ```
    pub fn new(static_dict: &'static HashMap<DiKey, NamedField>) -> Self {
        Self {
            catalog: Arc::new(CatalogSnapshot::new_static(static_dict)),
        }
    }

    /// 使用静态字典和动态层创建引擎
    ///
    /// # 参数
    ///
    /// - `layers`: 动态层列表（按加载顺序，索引越大优先级越高）
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// use spec_engine::{Engine, Layer};
    ///
    /// let layers = vec![
    ///     Layer::from_yaml_dir("custom".into(), "config/custom")?,
    /// ];
    ///
    /// let engine = Engine::with_layers(layers);
    /// ```
    pub fn with_layers(layers: Vec<Layer>) -> Self {
        let static_dict = get_spec_catalog();
        Self {
            catalog: Arc::new(CatalogSnapshot::new_with_layers(static_dict, layers)),
        }
    }

    /// 解析 DI 数据（完整版）
    ///
    /// 根据协议、DI 码、区域和方向查找定义，并解析字节数据。
    ///
    /// # 参数
    ///
    /// - `protocol`: 协议名称（如 "csg13", "dlt645-2007"）
    /// - `di`: DI 标识（32位无符号整数）
    /// - `region`: 区域/省份（如 "南网", "广东"）
    /// - `dir`: 方向（`None` 表示通用，`Some("0")` 下行，`Some("1")` 上行）
    /// - `data`: 待解析的字节数据
    ///
    /// # 返回值
    ///
    /// - `Ok((Value, usize))`: 解析成功，返回值和消耗的字节数
    /// - `Err(DictError)`: 解析失败
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// use spec_engine::Engine;
    ///
    /// let engine = Engine::new();
    /// let data = vec![0x34, 0x12, 0x00, 0x00];
    ///
    /// let (value, consumed) = engine.parse(
    ///     "csg13",
    ///     0x00010000,
    ///     "南网",
    ///     None,
    ///     &data,
    /// )?;
    ///
    /// println!("解析结果: {:?}", value);
    /// println!("消耗字节: {}", consumed);
    /// ```
    pub fn parse(
        &self,
        protocol: &str,
        di: u32,
        region: &str,
        dir: Option<&str>,
        data: &[u8],
    ) -> Result<(Value, usize), DictError> {
        // 查找字段定义
        let field = self
            .catalog
            .lookup(protocol, di, region, dir)
            .ok_or_else(|| DictError::UnknownDi {
                protocol: protocol.to_string(),
                di,
                region: region.to_string(),
                dir: crate::error::DirectionDisplay(
                    dir.and_then(spec_compiler::types::Direction::from_str),
                ),
            })?;

        // 使用内部解析方法
        let mut ctx = Context::new();
        self.parse_field(data, &field.spec, &mut ctx, protocol, region, dir)
    }

    /// 查找 DI 定义（不解析数据）
    ///
    /// 只查找字段定义，不进行数据解析。返回 owned 值以避免生命周期问题。
    ///
    /// # 参数
    ///
    /// - `protocol`: 协议名称
    /// - `di`: DI 标识
    /// - `region`: 区域/省份
    /// - `dir`: 方向
    ///
    /// # 返回值
    ///
    /// - `Some(NamedField)`: 找到定义
    /// - `None`: 未找到
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let engine = Engine::new();
    ///
    /// if let Some(field) = engine.lookup("csg13", 0x00010000, "南网", None) {
    ///     println!("字段名: {}", field.name);
    ///     println!("规格: {:?}", field.spec);
    /// }
    /// ```
    pub fn lookup(
        &self,
        protocol: &str,
        di: u32,
        region: &str,
        dir: Option<&str>,
    ) -> Option<NamedField> {
        self.catalog.lookup(protocol, di, region, dir).cloned()
    }

    /// 获取动态层数量
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let engine = Engine::new();
    /// println!("动态层数: {}", engine.layer_count());
    /// ```
    pub fn layer_count(&self) -> usize {
        self.catalog.layer_count()
    }

    /// 获取所有动态层的名称
    ///
    /// # 返回值
    ///
    /// 层名称的向量，按加载顺序排列。
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let engine = Engine::new();
    /// let layers = engine.layer_names();
    /// println!("已加载的层: {:?}", layers);
    /// ```
    pub fn layer_names(&self) -> Vec<String> {
        self.catalog.layer_names()
    }

    /// 列出所有支持的协议
    ///
    /// 遍历静态字典和动态层，返回所有协议名称及其统计信息。
    ///
    /// # 返回值
    ///
    /// 协议名称到统计信息的向量：`(protocol, di_count, regions)`
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let engine = Engine::new();
    /// let protocols = engine.list_protocols();
    /// for (protocol, count, regions) in protocols {
    ///     println!("{}: {} DIs, regions: {:?}", protocol, count, regions);
    /// }
    /// ```
    pub fn list_protocols(&self) -> Vec<(String, usize, Vec<String>)> {
        use std::collections::{HashMap, HashSet};

        let mut protocol_stats: HashMap<String, (usize, HashSet<String>)> = HashMap::new();

        // 统计静态字典
        for (protocol, _di, region, _dir) in self.catalog.static_dict.keys() {
            let (count, regions) = protocol_stats
                .entry(protocol.clone())
                .or_insert((0, HashSet::new()));
            *count += 1;
            regions.insert(region.clone());
        }

        // 统计动态层
        for layer in self.catalog.dynamic_layers.iter() {
            for (protocol, _di, region, _dir) in layer.table.keys() {
                let (count, regions) = protocol_stats
                    .entry(protocol.clone())
                    .or_insert((0, HashSet::new()));
                *count += 1;
                regions.insert(region.clone());
            }
        }

        // 转换为输出格式
        let mut result: Vec<_> = protocol_stats
            .into_iter()
            .map(|(protocol, (count, regions))| {
                let mut region_vec: Vec<_> = regions.into_iter().collect();
                region_vec.sort();
                (protocol, count, region_vec)
            })
            .collect();

        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    /// 搜索 DI 定义
    ///
    /// 根据关键词搜索 DI 名称，支持协议和区域过滤。
    ///
    /// # 参数
    ///
    /// - `keyword`: 搜索关键词（不区分大小写，匹配 DI 名称）
    /// - `protocol_filter`: 可选的协议过滤
    /// - `region_filter`: 可选的区域过滤
    /// - `limit`: 最大返回结果数量
    ///
    /// # 返回值
    ///
    /// 匹配的 DI 定义向量：`(protocol, di, region, name)`
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let engine = Engine::new();
    /// let results = engine.search_di("电压", None, None, 50);
    /// for (protocol, di, region, name) in results {
    ///     println!("{:08X} [{}] {}: {}", di, protocol, region, name);
    /// }
    /// ```
    pub fn search_di(
        &self,
        keyword: &str,
        protocol_filter: Option<&str>,
        region_filter: Option<&str>,
        limit: usize,
    ) -> Vec<(String, u32, String, String)> {
        let keyword_lower = keyword.to_lowercase();
        let mut results = Vec::new();

        // 搜索静态字典
        for ((protocol, di, region, _dir), field) in self.catalog.static_dict.iter() {
            if results.len() >= limit {
                break;
            }

            // 协议过滤
            if let Some(pf) = protocol_filter {
                if protocol != pf {
                    continue;
                }
            }

            // 区域过滤
            if let Some(rf) = region_filter {
                if region != rf {
                    continue;
                }
            }

            // 关键词匹配
            if field.name.to_lowercase().contains(&keyword_lower) {
                results.push((
                    protocol.clone(),
                    *di,
                    region.clone(),
                    field.name.clone(),
                ));
            }
        }

        // 搜索动态层（如果还没达到限制）
        if results.len() < limit {
            for layer in self.catalog.dynamic_layers.iter() {
                for ((protocol, di, region, _dir), field) in layer.table.iter() {
                    if results.len() >= limit {
                        break;
                    }

                    // 协议过滤
                    if let Some(pf) = protocol_filter {
                        if protocol != pf {
                            continue;
                        }
                    }

                    // 区域过滤
                    if let Some(rf) = region_filter {
                        if region != rf {
                            continue;
                        }
                    }

                    // 关键词匹配
                    if field.name.to_lowercase().contains(&keyword_lower) {
                        // 避免重复（静态字典已包含的）
                        if !results.iter().any(|(p, d, r, _)| p == protocol && *d == *di && r == region) {
                            results.push((
                                protocol.clone(),
                                *di,
                                region.clone(),
                                field.name.clone(),
                            ));
                        }
                    }
                }
            }
        }

        results
    }

    pub fn parse_field(
        &self,
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
            } => self.parse_fixed(buf, encoding, length, unit, enum_map, format, ctx),
            FieldSpec::BitField { length, bits } => self.parse_bitfield(buf, *length, bits),
            FieldSpec::Switch {
                on,
                cases,
                case_names,
                default,
            } => self.parse_switch(
                buf, on, cases, case_names, default, ctx, protocol, region, dir,
            ),
            FieldSpec::Repeat {
                count,
                count_ref,
                count_expr,
                bits_ref,
                bit_direction,
                iterate_order,
                bit_specs,
                element,
                name_template,
                id_expr,
            } => self.parse_repeat(
                buf,
                count,
                count_ref,
                count_expr,
                bits_ref,
                bit_direction,
                iterate_order,
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
                bit_direction,
                iterate_order,
                bit_specs,
                element,
                name_template,
            } => self.parse_bitmask(
                buf,
                *length,
                bit_direction,
                iterate_order,
                bit_specs,
                element,
                name_template,
                ctx,
                protocol,
                region,
                dir,
            ),
            FieldSpec::Skip => self.parse_skip(),
            FieldSpec::External { protocol, length } => self.parse_external(buf, protocol, length, ctx),
            FieldSpec::Container(fields) => {
                self.parse_container(buf, fields, ctx, protocol, region, dir)
            }
            FieldSpec::Custom(handler) => self.parse_custom(buf, handler),
            FieldSpec::DictRef { di_ref } => {
                self.parse_dict_ref(buf, di_ref, ctx, protocol, region, dir)
            }
            FieldSpec::InfoPoint => self.parse_info_point(buf),
            FieldSpec::DiCode => self.parse_di_code(buf, protocol, region, dir),
        }
    }

    pub fn parse_di(
        &self,
        protocol: &str,
        di: u32,
        region: &str,
        dir: Option<&str>,
        buf: &[u8],
    ) -> Result<(Value, usize), DictError> {
        let entry = self.lookup(protocol, di, region, dir)
            .ok_or_else(|| DictError::UnknownDi {
                protocol: protocol.to_string(),
                di,
                region: region.to_string(),
                dir: crate::error::DirectionDisplay(
                    dir.and_then(spec_compiler::types::Direction::from_str),
                ),
            })?;
        
        let mut ctx = Context::new();
        let (value, consumed) = self.parse_field(buf, &entry.spec, &mut ctx, protocol, region, dir)?;
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

    fn apply_enum_map(
        &self,
        value: Value,
        raw: &str,
        enum_map: &Option<HashMap<String, String>>,
    ) -> Value {
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
                Value::Invalid { .. } => String::new(),
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
                Value::Bit { value, .. } => value
                    .as_deref()
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .or_else(|| {
                        value
                            .as_deref()
                            .and_then(|v| v.as_int().map(|i| i.to_string()))
                    })
                    .unwrap_or_default(),
            };
            if let Some(mapped) = map.get(&value_key).cloned() {
                return Value::Str(mapped);
            }
        }
        value
    }

    fn extract_bits(&self, raw: &[u8], range: (usize, usize)) -> u64 {
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

    fn extract_bits_ordered(&self, raw: &[u8], range: (usize, usize), bit_direction: Option<&str>) -> u64 {
        let (start, end) = range;
        if end < start {
            return 0;
        }
        let mut result = 0u64;
        for bit_index in start..=end {
            let byte_index = bit_index / 8;
            let bit_in_byte = bit_index % 8;
            let bit_position = if bit_direction == Some("lsb") {
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
        &self,
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
                let eval = self.eval_length_expr(expr, ctx, None, buf.len()).map_err(|e| {
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
                    return Ok((
                        Value::Str(spec_compiler::types::format_bytes_with_spec(raw, spec)),
                        length,
                    ));
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
                match decode_bcd_u64(&cleared) {
                    Ok(decoded) => {
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
                    Err(err) => Value::Invalid {
                        reason: format!("BCD decode failed: {}", err),
                    },
                }
            }
            Encoding::Ascii => Value::Str(decode_ascii(raw)),
            Encoding::Hex => {
                if let Some(spec) = format {
                    Value::Str(spec_compiler::types::format_bytes_with_spec(raw, spec))
                } else {
                    Value::Str(raw_hex.clone())
                }
            }
            Encoding::Time { format, encoding } => Value::Str(decode_time(raw, format, *encoding)),
            Encoding::Raw => Value::Bytes(raw.to_vec()),
        };
        let parsed = self.apply_enum_map(value, &raw_hex, enum_map);
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
        &self,
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
        let mut items = Vec::with_capacity(bits.len());
        for bit in bits {
            if bit.range.1 >= total_bits {
                return Err(DictError::UnexpectedEof {
                    needed: (bit.range.1 / 8) + 1,
                    available: raw.len(),
                });
            }
            let extracted = self.extract_bits(raw, bit.range);
            let raw_key = extracted.to_string();
            let semantic = if let Some(enum_map) = &bit.enum_map {
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
            // compute source byte(s) for this bit range (take the first byte where the range starts)
            let bit_start = bit.range.0;
            let byte_index = bit_start / 8;
            let bit_source = if byte_index < raw.len() {
                vec![raw[byte_index]]
            } else {
                Vec::new()
            };

            // present each bit as Value::Bit wrapped in a Node so consumers can
            // iterate in-order and preserve range semantics (supporting ranges)
            let bit_item = Value::Bit {
                bit_start: bit.range.0,
                bit_end: bit.range.1,
                bit_value: extracted,
                bit_byte: bit_source.clone(),
                value: Some(Box::new(semantic.clone())),
            };
            let node = Value::Node {
                name: formatted_name,
                raw: Vec::new(),
                value: Box::new(bit_item),
            };
            items.push(node);
        }
        Ok((Value::List(items), length))
    }

    fn parse_skip(&self) -> Result<(Value, usize), DictError> {
        Ok((Value::Skip, 0))
    }

    fn parse_bitmask(
        &self,
        buf: &[u8],
        length: usize,
        bit_direction: &Option<String>,
        iterate_order: &Option<String>,
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
        if iterate_order.as_deref() == Some("desc") {
            bit_specs_sorted.sort_by_key(|bit| std::cmp::Reverse(bit.range.0));
        } else {
            bit_specs_sorted.sort_by_key(|bit| bit.range.0);
        }
        let bit_count = bit_specs_sorted.len();
        let mut offset = 0usize;
        for (idx, bit_spec) in bit_specs_sorted.iter().enumerate() {
            let bit_value = self.extract_bits_ordered(&raw, bit_spec.range, bit_direction.as_deref());
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

            let instantiated_element = self.instantiate_field_spec(element, idx, bit_count);
            let (v, consumed) = self.parse_field(
                &buf[offset..],
                &instantiated_element,
                ctx,
                protocol,
                region,
                dir,
            )?;
            ctx.pop_scope();

            // compute source byte(s) for this bit range (take the first byte where the range starts)
            let bit_start = bit_spec.range.0;
            let byte_index = bit_start / 8;
            let bit_source = if byte_index < raw.len() {
                vec![raw[byte_index]]
            } else {
                Vec::new()
            };

            // if the instantiated element indicates Skip, still advance offset and continue
            if let Value::Skip = v {
                offset += consumed;
                continue;
            }

            // build the list item name from template or bit name
            let entry_name = if let Some(template) = name_template {
                format_repeat_name(
                    Some(template.as_str()),
                    None,
                    Some(bit_spec.name.as_str()),
                    bit_spec.ref_id.as_deref(),
                    idx,
                    bit_count,
                )
            } else {
                bit_spec.name.clone()
            };

            let bit_value_node = Value::Bit {
                bit_start: bit_spec.range.0,
                bit_end: bit_spec.range.1,
                bit_value,
                bit_byte: bit_source,
                value: Some(Box::new(v)),
            };

            let node = Value::Node {
                name: entry_name,
                raw: Vec::new(),
                value: Box::new(bit_value_node),
            };
            items.push(node);
            offset += consumed;
        }
        Ok((Value::List(items), length))
    }

    fn parse_external(
        &self,
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

    fn parse_custom(&self, buf: &[u8], handler: &str) -> Result<(Value, usize), DictError> {
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
    fn parse_info_point(&self, buf: &[u8]) -> Result<(Value, usize), DictError> {
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
        &self,
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
        let label = match self.lookup(protocol, di, region, dir) {
            Some(entry) if !entry.name.is_empty() => format!("{:08X}_{}", di, entry.name),
            Some(entry) => entry.id.clone().unwrap_or_else(|| format!("{:08X}", di)),
            None => format!("{:08X}_未知数据标识", di),
        };
        Ok((Value::Str(label), 4))
    }

    fn parse_switch(
        &self,
        buf: &[u8],
        on: &str,
        cases: &std::collections::HashMap<String, Box<FieldSpec>>,
        case_names: &Option<HashMap<String, String>>,
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
                Value::WithUnit { value, .. } => value
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| value.as_int().map(|i| i.to_string()).unwrap_or_default()),
                Value::Node { value, .. } => value
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| value.as_int().map(|i| i.to_string()).unwrap_or_default()),
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
        let (value, consumed) = self.parse_field(buf, chosen, ctx, protocol, region, dir)?;
        if let Value::Skip = value {
            return Ok((Value::Skip, consumed));
        }
        if let Some(names) = case_names {
            if let Some(case_name) = names.get(&key) {
                if consumed == 0 {
                    return Ok((Value::Str(case_name.clone()), consumed));
                }
                return Ok((
                    Value::Node {
                        name: case_name.clone(),
                        raw: buf[..consumed].to_vec(),
                        value: Box::new(value),
                    },
                    consumed,
                ));
            }
        }
        Ok((value, consumed))
    }

    fn format_repeat_template(&self, template: &str, idx: usize, count: usize) -> String {
        template
            .replace("{index0}", &idx.to_string())
            .replace("{index}", &(idx + 1).to_string())
            .replace("{count}", &count.to_string())
    }

    fn eval_length_expr(
        &self,
        expr: &str,
        ctx: &Context,
        idx_opt: Option<usize>,
        len: usize,
    ) -> Result<u32, String> {
        struct Parser<'a> {
            input: &'a str,
            pos: usize,
            idx_opt: Option<usize>,
            ctx: &'a Context,
            len: usize,
        }

        impl<'a> Parser<'a> {
            fn new(input: &'a str, idx_opt: Option<usize>, ctx: &'a Context, len: usize) -> Self {
                Parser {
                    input,
                    pos: 0,
                    idx_opt,
                    ctx,
                    len,
                }
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
                if self.input[self.pos..].starts_with("0x")
                    || self.input[self.pos..].starts_with("0X")
                {
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
                if self.input[self.pos..].starts_with('$') {
                    self.next();
                }
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
                    v.as_u32()
                        .ok_or_else(|| format!("ref({}) 不是可用的非负整数", name))
                } else {
                    // variable like index, index0, remaining, or other ref_id
                    let start = self.pos;
                    while matches!(self.peek(), Some(ch) if ch.is_ascii_alphanumeric() || ch == '_')
                    {
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
                        "remaining" | "len" | "length" => self
                            .len
                            .try_into()
                            .map_err(|_| "剩余长度超过 u32 上限".to_string()),
                        _ => {
                            let v = self
                                .ctx
                                .get_decoded(token)
                                .ok_or_else(|| format!("缺少引用字段: {}", token))?;
                            v.as_u32()
                                .ok_or_else(|| format!("{} 不是可用的非负整数", token))
                        }
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
                } else if matches!(self.peek(), Some(ch) if ch == '$' || ch.is_ascii_alphabetic()) {
                    self.parse_variable_or_ref()
                } else if matches!(self.peek(), Some(ch) if ch.is_ascii_digit()) {
                    self.parse_number()
                } else {
                    Err(format!(
                        "非法表达式起始: {}",
                        self.input[self.pos..].chars().next().unwrap_or('?')
                    ))
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

        let mut parser = Parser::new(expr, idx_opt, ctx, len);
        let result = parser.parse_expr()?;
        parser.skip_ws();
        if parser.peek().is_some() {
            return Err(format!("未处理的表达式尾部: {}", &expr[parser.pos..]));
        }
        Ok(result)
    }

    fn instantiate_named_field(&self, nf: &NamedField, idx: usize, count: usize) -> NamedField {
        NamedField {
            id: nf
                .id
                .as_ref()
                .map(|s| self.format_repeat_template(s, idx, count)),
            ref_id: nf
                .ref_id
                .as_ref()
                .map(|s| self.format_repeat_template(s, idx, count)),
            name: self.format_repeat_template(&nf.name, idx, count),
            spec: self.instantiate_field_spec(&nf.spec, idx, count),
            format: nf.format.clone(),
        }
    }

    fn instantiate_field_spec(&self, spec: &FieldSpec, idx: usize, count: usize) -> FieldSpec {
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
            FieldSpec::Switch {
                on,
                cases,
                case_names,
                default,
            } => FieldSpec::Switch {
                on: self.format_repeat_template(on, idx, count),
                cases: cases
                    .iter()
                    .map(|(k, v)| {
                        (
                            self.format_repeat_template(k, idx, count),
                            Box::new(self.instantiate_field_spec(v, idx, count)),
                        )
                    })
                    .collect(),
                case_names: case_names.as_ref().map(|m| {
                    m.iter()
                        .map(|(k, v)| {
                            (
                                self.format_repeat_template(k, idx, count),
                                self.format_repeat_template(v, idx, count),
                            )
                        })
                        .collect()
                }),
                default: default
                    .as_ref()
                    .map(|v| Box::new(self.instantiate_field_spec(v, idx, count))),
            },
            FieldSpec::Repeat {
                count: count_field,
                count_ref,
                count_expr,
                bits_ref,
                bit_direction,
                iterate_order,
                bit_specs,
                element,
                name_template,
                id_expr,
            } => FieldSpec::Repeat {
                count: *count_field,
                count_ref: count_ref
                    .as_ref()
                    .map(|s| self.format_repeat_template(s, idx, count)),
                count_expr: count_expr
                    .as_ref()
                    .map(|s| self.format_repeat_template(s, idx, count)),
                bits_ref: bits_ref.clone(),
                bit_direction: bit_direction.clone(),
                iterate_order: iterate_order.clone(),
                bit_specs: bit_specs.clone(),
                element: Box::new(self.instantiate_field_spec(element, idx, count)),
                name_template: name_template
                    .as_ref()
                    .map(|tmpl| self.format_repeat_template(tmpl, idx, count)),
                id_expr: id_expr.clone(),
            },
            FieldSpec::External { protocol, length } => FieldSpec::External {
                protocol: protocol.clone(),
                length: length.clone(),
            },
            FieldSpec::BitMask {
                length,
                bit_direction,
                iterate_order,
                bit_specs,
                element,
                name_template,
            } => FieldSpec::BitMask {
                length: *length,
                bit_direction: bit_direction.clone(),
                iterate_order: iterate_order.clone(),
                bit_specs: bit_specs.clone(),
                element: Box::new(self.instantiate_field_spec(element, idx, count)),
                name_template: name_template
                    .as_ref()
                    .map(|tmpl| self.format_repeat_template(tmpl, idx, count)),
            },
            FieldSpec::Skip => FieldSpec::Skip,
            FieldSpec::Container(fields) => FieldSpec::Container(
                fields
                    .iter()
                    .map(|nf| self.instantiate_named_field(nf, idx, count))
                    .collect(),
            ),
            FieldSpec::Custom(handler) => FieldSpec::Custom(handler.clone()),
            FieldSpec::DictRef { di_ref } => FieldSpec::DictRef {
                di_ref: self.format_repeat_template(di_ref, idx, count),
            },
            // 无字段的单元变体，repeat 展开时原样复制即可，不涉及任何模板替换
            FieldSpec::InfoPoint => FieldSpec::InfoPoint,
            FieldSpec::DiCode => FieldSpec::DiCode,
        }
    }

    fn parse_repeat(
        &self,
        buf: &[u8],
        count_fixed: &Option<usize>,
        count_ref: &Option<String>,
        count_expr: &Option<String>,
        bits_ref: &Option<String>,
        bit_direction: &Option<String>,
        iterate_order: &Option<String>,
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

        let count = if let Some(fixed) = count_fixed {
            *fixed
        } else if let Some(count_ref) = count_ref {
            ctx.get_decoded(count_ref)
                .and_then(|v| v.as_usize())
                .ok_or_else(|| DictError::MissingRef(count_ref.to_string()))?
        } else if let Some(expr) = count_expr {
            self.eval_length_expr(expr, ctx, None, buf.len())
                .map_err(|e| DictError::ExternalParseError(format!("count_expr 解析失败: {}", e)))?
                as usize
        } else {
            usize::MAX
        };

        if count != usize::MAX {
            for idx in 0..count {
                let instantiated_element = self.instantiate_field_spec(element, idx, count);
                let (v, consumed) = self.parse_field(
                    &buf[offset..],
                    &instantiated_element,
                    ctx,
                    protocol,
                    region,
                    dir,
                )?;
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
                    let name = format_id_expr(expr, idx)
                        .unwrap_or_else(|e| panic!("id_expr 解析失败: {} (idx={})", e, idx));
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

        let bits_ref = bits_ref.as_ref().ok_or_else(|| {
            DictError::MissingRef("repeat 字段缺少 count_ref 或 bits_ref".to_string())
        })?;
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
        if iterate_order.as_deref() == Some("desc") {
            bit_specs_sorted.sort_by_key(|bit| std::cmp::Reverse(bit.range.0));
        } else {
            bit_specs_sorted.sort_by_key(|bit| bit.range.0);
        }

        let bit_count = bit_specs_sorted.len();
        for (idx, bit_spec) in bit_specs_sorted.iter().enumerate() {
            let bit_value = self.extract_bits_ordered(&raw, bit_spec.range, bit_direction.as_deref());
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

            let instantiated_element = self.instantiate_field_spec(element, idx, bit_count);
            let (v, consumed) = self.parse_field(
                &buf[offset..],
                &instantiated_element,
                ctx,
                protocol,
                region,
                dir,
            )?;
            ctx.pop_scope();
            if let Value::Skip = v {
                offset += consumed;
                continue;
            }
            // 如果该 case 长度为 0（consumed == 0），则视为不需要产生解析结果（例如 switch 的 "0" 分支），
            // 此时不创建节点也不加入 items，只弹出作用域并继续下一个 bit。
            if consumed == 0 {
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
        Ok((Value::List(items), offset))
    }

    // parse_external 内容不变（不需要 dir，省略）

    fn parse_container(
        &self,
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
            let (v, consumed) =
                match self.parse_field(&buf[offset..], &nf.spec, ctx, protocol, region, dir) {
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
        &self,
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
        let target = self.lookup(protocol, di, region, dir)
            .ok_or_else(|| DictError::UnknownDi {
                protocol: protocol.to_string(),
                di,
                region: region.to_string(),
                dir: crate::error::DirectionDisplay(
                    dir.and_then(spec_compiler::types::Direction::from_str),
                ),
            })?;
        self.parse_field(buf, &target.spec, ctx, protocol, region, dir)
    }
}

impl Default for Engine {
    fn default() -> Self {
        // 注意：这里需要静态字典，实际使用时需要从 lib.rs 获取
        // 这里暂时使用空的 HashMap，lib.rs 中会提供正确的实现
        use std::sync::OnceLock;
        static EMPTY_DICT: OnceLock<HashMap<DiKey, NamedField>> = OnceLock::new();
        let dict = EMPTY_DICT.get_or_init(HashMap::new);
        Self::new(dict)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spec_compiler::types::{Encoding, FieldLength, FieldSpec};
    use std::collections::HashMap;

    fn create_test_dict() -> HashMap<DiKey, NamedField> {
        let mut dict = HashMap::new();

        // 添加一个测试 DI
        let field = NamedField {
            id: Some("00010000".to_string()),
            ref_id: None,
            name: "测试字段".to_string(),
            spec: FieldSpec::Fixed {
                encoding: Encoding::Raw,
                length: FieldLength::Fixed(4),
                unit: None,
                enum_map: None,
                format: None,
            },
            format: None,
        };

        dict.insert(
            ("test".to_string(), 0x00010000, "南网".to_string(), None),
            field,
        );

        dict
    }

    #[test]
    fn test_engine_new() {
        use std::sync::OnceLock;
        static TEST_DICT: OnceLock<HashMap<DiKey, NamedField>> = OnceLock::new();
        let dict = TEST_DICT.get_or_init(create_test_dict);

        let engine = Engine::new(dict);
        assert_eq!(engine.layer_count(), 0);
    }

    #[test]
    fn test_engine_lookup() {
        use std::sync::OnceLock;
        static TEST_DICT: OnceLock<HashMap<DiKey, NamedField>> = OnceLock::new();
        let dict = TEST_DICT.get_or_init(create_test_dict);

        let engine = Engine::new(dict);
        let field = engine.lookup("test", 0x00010000, "南网", None);

        assert!(field.is_some());
        assert_eq!(field.unwrap().name, "测试字段");
    }

    #[test]
    fn test_engine_clone() {
        use std::sync::OnceLock;
        static TEST_DICT: OnceLock<HashMap<DiKey, NamedField>> = OnceLock::new();
        let dict = TEST_DICT.get_or_init(create_test_dict);

        let engine = Engine::new(dict);
        let engine_clone = engine.clone();

        // 验证 clone 后可以独立使用
        assert_eq!(engine.layer_count(), engine_clone.layer_count());
    }
}

#[cfg(test)]
mod info_point_tests {
    use super::*;

    fn get_engine() -> Engine {
        use std::sync::OnceLock;
        use crate::get_spec_catalog;
        static TEST_DICT: OnceLock<HashMap<DiKey, NamedField>> = OnceLock::new();
        let dict = TEST_DICT.get_or_init(|| HashMap::new());
        Engine::new(dict)
    }

    #[test]
    fn single_point_in_first_group() {
        // DA2=01H, DA1=01H → p1
        let engine = get_engine();
        let (value, consumed) = engine.parse_info_point(&[0x01, 0x01]).unwrap();
        assert_eq!(consumed, 2);
        assert_eq!(value, Value::List(vec![Value::Pn(1)]));
    }

    #[test]
    fn multiple_points_in_same_group() {
        // 原文举例：DA2=01H, DA1=03H → p1、p2 同时命中
        let engine = get_engine();
        let (value, consumed) = engine.parse_info_point(&[0x03, 0x01]).unwrap();
        assert_eq!(consumed, 2);
        assert_eq!(value, Value::List(vec![Value::Pn(1), Value::Pn(2)]));
    }

    #[test]
    fn point_in_second_group_offsets_by_eight() {
        // DA2=02H, DA1=01H → p9 (D0 对应 p((2-1)*8+1) = p9)
        let engine = get_engine();
        let (value, _) = engine.parse_info_point(&[0x01, 0x02]).unwrap();
        assert_eq!(value, Value::List(vec![Value::Pn(9)]));
    }

    #[test]
    fn all_zero_means_terminal_point_p0() {
        let engine = get_engine();
        let (value, _) = engine.parse_info_point(&[0x00, 0x00]).unwrap();
        assert_eq!(value, Value::Str("p0（终端测量点）".to_string()));
    }

    #[test]
    fn all_ff_means_all_points_except_terminal() {
        let engine = get_engine();
        let (value, _) = engine.parse_info_point(&[0xFF, 0xFF]).unwrap();
        assert_eq!(value, Value::Str("除终端测量点外的所有测量点".to_string()));
    }

    #[test]
    fn rejects_short_buffer() {
        let engine = get_engine();
        let err = engine.parse_info_point(&[0x01]).unwrap_err();
        assert!(matches!(
            err,
            DictError::UnexpectedEof {
                needed: 2,
                available: 1
            }
        ));
    }
}

#[cfg(test)]
mod di_code_tests {
    use super::*;

    fn get_engine() -> Engine {
        Engine::new_default()
    }

    #[test]
    fn resolves_known_di_to_name() {
        // di = 0x00010001，传输顺序(小端) DI0,DI1,DI2,DI3 = 01 00 01 00
        let engine = get_engine();
        let (value, consumed) =
            engine.parse_di_code(&[0x01, 0x00, 0x01, 0x00], "csg13", "南网", None).unwrap();
        assert_eq!(consumed, 4);
        match value {
            Value::Str(s) => assert_eq!(s, "00010001_月冻结正向有功总电能"),
            other => panic!("unexpected value shape: {other:?}"),
        }
    }

    #[test]
    fn falls_back_gracefully_for_unknown_di() {
        let engine = get_engine();
        let (value, consumed) =
            engine.parse_di_code(&[0xEF, 0xBE, 0xAD, 0xDE], "csg13", "南网", None).unwrap();
        assert_eq!(consumed, 4);
        match value {
            Value::Str(s) => assert!(s.contains("未知数据标识")),
            other => panic!("unexpected value shape: {other:?}"),
        }
    }

    #[test]
    fn rejects_short_buffer() {
        let engine = get_engine();
        let err = engine.parse_di_code(&[0x01, 0x02], "csg13", "南网", None).unwrap_err();
        assert!(matches!(
            err,
            DictError::UnexpectedEof {
                needed: 4,
                available: 2
            }
        ));
    }
}
