//! 重复结构和表达式求值工具
//!
//! 本模块提供处理 `candidate_ids`、`count_expr`、`id_expr` 等重复结构的工具函数。
//! 这些函数在编译期用于展开重复定义，在运行时用于生成字段名称。
//!
//! # 主要功能
//!
//! - [`eval_id_expr`]: 计算 `id_expr` 表达式，生成 DI ID
//! - [`format_id_expr`]: 计算并格式化 DI ID 为8位十六进制字符串
//! - [`format_repeat_name`]: 根据模板生成重复字段的名称
//!
//! # 表达式语法
//!
//! ## 变量
//!
//! - `index`: 1-based 索引（从1开始）
//! - `index0`: 0-based 索引（从0开始）
//!
//! ## 运算符
//!
//! - 算术：`+`, `-`, `*`, `/`
//! - 括号：`( )`
//!
//! ## 数字格式
//!
//! - 十进制：`123`
//! - 十六进制：`0x1234` 或 `0X1234`
//!
//! # 示例
//!
//! ## 计算 DI ID 表达式
//!
//! ```rust,no_run
//! use spec_compiler::repeat::eval_id_expr;
//!
//! // 费率1（index0=0）的 DI ID
//! let di = eval_id_expr("0x00010100 + index0*0x0100", 0).unwrap();
//! assert_eq!(di, 0x00010100);
//!
//! // 费率2（index0=1）的 DI ID
//! let di = eval_id_expr("0x00010100 + index0*0x0100", 1).unwrap();
//! assert_eq!(di, 0x00010200);
//!
//! // 使用 1-based 索引
//! let di = eval_id_expr("0x00000100*index", 3).unwrap();
//! assert_eq!(di, 0x00000300);
//! ```
//!
//! ## 格式化 DI ID
//!
//! ```rust
//! use spec_compiler::repeat::format_id_expr;
//!
//! let id_str = format_id_expr("0x00010100 + index0*0x0100", 5).unwrap();
//! assert_eq!(id_str, "00010600");
//! ```
//!
//! ## 生成重复字段名称
//!
//! ```rust
//! use spec_compiler::repeat::format_repeat_name;
//!
//! // 使用 name_template
//! let name = format_repeat_name(
//!     Some("费率{index}电能"),
//!     None,
//!     None,
//!     None,
//!     2,  // index0=2
//!     10, // count=10
//! );
//! assert_eq!(name, "费率3电能");
//!
//! // 使用 id 占位符
//! let name = format_repeat_name(
//!     Some("{id}_数据"),
//!     Some("00010200"),
//!     None,
//!     None,
//!     1,
//!     10,
//! );
//! assert_eq!(name, "00010200_数据");
//!
//! // 使用 bit_name
//! let name = format_repeat_name(
//!     Some("第{index}项({bit_name})"),
//!     None,
//!     Some("温度告警"),
//!     None,
//!     0,
//!     8,
//! );
//! assert_eq!(name, "第1项(温度告警)");
//! ```
//!
//! # YAML 配置示例
//!
//! ```yaml
//! candidate_ids:
//!   count: 63
//!   id_expr: "0x00010100 + index0*0x0100"
//!   name_template: "费率{index}电能"
//!   element:
//!     length: 4
//!     type: bcd
//!     decimals: 2
//!     unit: "kWh"
//! ```
//!
//! 上述配置会展开为63个 DI 条目：
//! - `00010100`: 费率1电能
//! - `00010200`: 费率2电能
//! - ...
//! - `00013F00`: 费率63电能

/// 格式化 DI ID 表达式为8位十六进制字符串
///
/// 计算 `id_expr` 表达式的值，并格式化为8位大写十六进制字符串（不带 `0x` 前缀）。
///
/// # 参数
///
/// - `expr`: ID 表达式，如 `"0x00010100 + index0*0x0100"`
/// - `idx`: 0-based 索引（`index0` 的值）
///
/// # 返回值
///
/// - `Ok(String)`: 格式化的8位十六进制字符串，如 `"00010200"`
/// - `Err(String)`: 表达式求值错误信息
///
/// # 示例
///
/// ```rust
/// use spec_compiler::repeat::format_id_expr;
///
/// let id_str = format_id_expr("0x00010100 + index0*0x0100", 5).unwrap();
/// assert_eq!(id_str, "00010600");
/// ```
pub fn format_id_expr(expr: &str, idx: usize) -> Result<String, String> {
    let value = eval_id_expr(expr, idx)?;
    Ok(format!("{:08X}", value))
}

/// 根据模板格式化重复字段名称
///
/// 根据 `name_template` 中的占位符生成重复字段的名称。支持多种占位符和回退逻辑。
///
/// # 参数
///
/// - `template`: 名称模板，如 `"费率{index}电能"`
/// - `id_value`: DI ID 字符串（可选），用于 `{id}` 占位符
/// - `bit_name`: 位名称（可选），用于 `{bit_name}` 占位符
/// - `bit_ref`: 位引用 ID（可选），用于 `{bit_ref}` 占位符
/// - `idx`: 0-based 索引
/// - `count`: 总数量（用于 `{count}` 占位符）
///
/// # 占位符说明
///
/// - `{index0}`: 0-based 索引（0, 1, 2, ...）
/// - `{index}`: 1-based 索引（1, 2, 3, ...）
/// - `{count}`: 总数量
/// - `{id}`: DI ID 字符串（如果提供且模板中未使用，会作为前缀添加）
/// - `{bit_name}`: 位名称（用于位图重复）
/// - `{bit_ref}`: 位引用 ID（用于位图重复）
///
/// # 返回值
///
/// 格式化后的字段名称。如果没有提供模板，则：
/// - 如果有 `id_value`，返回 ID
/// - 否则返回空字符串
///
/// # 示例
///
/// ```rust
/// use spec_compiler::repeat::format_repeat_name;
///
/// // 使用 {index}
/// let name = format_repeat_name(Some("费率{index}电能"), None, None, None, 2, 10);
/// assert_eq!(name, "费率3电能");
///
/// // 使用 {index0}
/// let name = format_repeat_name(Some("数据[{index0}]"), None, None, None, 5, 10);
/// assert_eq!(name, "数据[5]");
///
/// // 使用 {id}（在模板中）
/// let name = format_repeat_name(
///     Some("{id}_值"),
///     Some("00010200"),
///     None,
///     None,
///     1,
///     10,
/// );
/// assert_eq!(name, "00010200_值");
///
/// // 使用 {id}（不在模板中，自动添加为前缀）
/// let name = format_repeat_name(
///     Some("电能"),
///     Some("00010200"),
///     None,
///     None,
///     1,
///     10,
/// );
/// assert_eq!(name, "00010200_电能");
///
/// // 无模板但有 ID
/// let name = format_repeat_name(None, Some("00010200"), None, None, 1, 10);
/// assert_eq!(name, "00010200");
/// ```
pub fn format_repeat_name(
    template: Option<&str>,
    id_value: Option<&str>,
    bit_name: Option<&str>,
    bit_ref: Option<&str>,
    idx: usize,
    count: usize,
) -> String {
    if let Some(template) = template {
        let mut s = template.replace("{index0}", &idx.to_string());
        s = s.replace("{index}", &(idx + 1).to_string());
        s = s.replace("{count}", &count.to_string());
        if let Some(bit_name_str) = bit_name {
            s = s.replace("{bit_name}", bit_name_str);
        }
        if let Some(bit_ref_str) = bit_ref {
            s = s.replace("{bit_ref}", bit_ref_str);
        }
        if let Some(id_str) = id_value {
            if s.contains("{id}") {
                s = s.replace("{id}", id_str);
            } else {
                s = format!("{}_{}", id_str, s);
            }
        }
        s
    } else if let Some(id_str) = id_value {
        id_str.to_string()
    } else {
        String::new()
    }
}

/// 计算 ID 表达式的值
///
/// 解析并求值 `id_expr` 表达式，支持算术运算和变量替换。
///
/// # 支持的语法
///
/// ## 变量
///
/// - `index`: 1-based 索引（`idx + 1`）
/// - `index0`: 0-based 索引（`idx`）
///
/// ## 运算符（按优先级从高到低）
///
/// 1. 括号：`( )`
/// 2. 乘除：`*`, `/`
/// 3. 加减：`+`, `-`
///
/// ## 数字
///
/// - 十进制：`123`、`456`
/// - 十六进制：`0x1234`、`0XABCD`
///
/// # 参数
///
/// - `expr`: ID 表达式字符串
/// - `idx`: 0-based 索引值（替换 `index0` 变量）
///
/// # 返回值
///
/// - `Ok(u32)`: 计算结果
/// - `Err(String)`: 错误信息（语法错误、溢出、除零等）
///
/// # 示例
///
/// ```rust
/// use spec_compiler::repeat::eval_id_expr;
///
/// // 简单算术
/// assert_eq!(eval_id_expr("0x100 + 0x200", 0).unwrap(), 0x300);
///
/// // 使用 index0
/// assert_eq!(eval_id_expr("0x00010100 + index0*0x0100", 0).unwrap(), 0x00010100);
/// assert_eq!(eval_id_expr("0x00010100 + index0*0x0100", 1).unwrap(), 0x00010200);
/// assert_eq!(eval_id_expr("0x00010100 + index0*0x0100", 10).unwrap(), 0x00010B00);
///
/// // 使用 index (1-based)
/// assert_eq!(eval_id_expr("index * 0x100", 0).unwrap(), 0x100);
/// assert_eq!(eval_id_expr("index * 0x100", 1).unwrap(), 0x200);
/// assert_eq!(eval_id_expr("index * 0x100", 9).unwrap(), 0xA00);
///
/// // 复杂表达式
/// assert_eq!(eval_id_expr("(0x1000 + index0*0x10) * 2", 5).unwrap(), 0x20A0);
///
/// // 错误处理
/// assert!(eval_id_expr("1 / 0", 0).is_err());  // 除零
/// assert!(eval_id_expr("invalid", 0).is_err());  // 非法变量
/// ```
///
/// # 错误
///
/// 返回错误的情况：
/// - 语法错误（非法字符、缺少括号等）
/// - 非法变量名（除 `index` 和 `index0` 外）
/// - 算术溢出（结果超过 `u32::MAX`）
/// - 除零错误
/// - 负数结果
pub fn eval_id_expr(expr: &str, idx: usize) -> Result<u32, String> {
    struct Parser<'a> {
        input: &'a str,
        pos: usize,
        idx: usize,
    }

    impl<'a> Parser<'a> {
        fn new(input: &'a str, idx: usize) -> Self {
            Parser { input, pos: 0, idx }
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

        fn parse_variable(&mut self) -> Result<u32, String> {
            self.skip_ws();
            let start = self.pos;
            while matches!(self.peek(), Some(ch) if ch.is_ascii_alphanumeric() || ch == '_') {
                self.next();
            }
            let token = &self.input[start..self.pos];
            match token {
                "index" => Ok((self.idx + 1) as u32),
                "index0" => Ok(self.idx as u32),
                _ => Err(format!("非法变量: {}", token)),
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
            } else if self.input[self.pos..].starts_with("index0") {
                self.parse_variable()
            } else if self.input[self.pos..].starts_with("index") {
                self.parse_variable()
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

    let mut parser = Parser::new(expr, idx);
    let result = parser.parse_expr()?;
    parser.skip_ws();
    if parser.peek().is_some() {
        return Err(format!("未处理的表达式尾部: {}", &expr[parser.pos..]));
    }
    Ok(result)
}
