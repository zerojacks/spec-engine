//! 解码函数 - 实现各种编码方式的解码

use super::Endian;

/// 解码 ASCII 字符串
pub fn decode_ascii(raw: &[u8]) -> String {
    raw.iter()
        .filter(|&&b| b >= 0x20 && b < 0x7F)
        .map(|&b| b as char)
        .collect()
}

/// 解码二进制为 u64（按字节序）
pub fn decode_bin_u64(raw: &[u8], endian: Endian) -> u64 {
    match endian {
        Endian::Little => raw.iter().rev().fold(0u64, |acc, &b| (acc << 8) | b as u64),
        Endian::Big => raw.iter().fold(0u64, |acc, &b| (acc << 8) | b as u64),
    }
}

/// 解码 BCD 为 u64
pub fn decode_bcd_u64(raw: &[u8]) -> u64 {
    let mut result = 0u64;
    for &b in raw {
        let high = (b >> 4) as u64;
        let low = (b & 0x0F) as u64;
        if high > 9 || low > 9 {
            return 0; // 非法 BCD
        }
        result = result * 100 + high * 10 + low;
    }
    result
}

/// 解码为十六进制字符串
pub fn decode_hex(raw: &[u8]) -> String {
    raw.iter().map(|b| format!("{:02X}", b)).collect()
}

/// 解码时间（简化实现，按格式解析）
pub fn decode_time(raw: &[u8], format: &str) -> String {
    // 支持按 format 字符串解析多种时间格式。格式由两字符的 token 组成，
    // 如 "YY","MM","DD","hh","mm","ss","ms","WW"，以及
    // 4 字符的特殊 token "xxxx"（表示两个字节的毫秒字段）。
    // 每个两字符 token 对应原始字节流中的 1 字节（BCD 或字节值），
    // 而 "xxxx" 对应 2 字节（高字节在前）。

    if raw.is_empty() || format.is_empty() {
        return decode_hex(raw);
    }

    // 将 format 拆分为 token 列表
    let mut tokens: Vec<&str> = Vec::new();
    let mut i = 0usize;
    while i < format.len() {
        if i + 4 <= format.len() && &format[i..i + 4] == "xxxx" {
            tokens.push(&format[i..i + 4]);
            i += 4;
        } else if i + 2 <= format.len() {
            tokens.push(&format[i..i + 2]);
            i += 2;
        } else {
            // 遇到单字符时跳过以保持健壮性
            i += 1;
        }
    }

    // 先解析到结构化字段，再按规范顺序输出
    let mut idx = 0usize;
    let mut cc_val: Option<u32> = None;
    let mut yy_val: Option<u32> = None;
    let mut year: Option<u32> = None;
    let mut month: Option<u32> = None;
    let mut day: Option<u32> = None;
    let mut hour: Option<u32> = None;
    let mut minute: Option<u32> = None;
    let mut second: Option<u32> = None;
    let mut msec: Option<u32> = None;
    let mut weekday_str: Option<String> = None;

    let weekday_map = ["天", "一", "二", "三", "四", "五", "六"];

    for &tok in &tokens {
        if tok == "xxxx" {
            if idx + 1 < raw.len() {
                let v = ((raw[idx] as u16) << 8) | raw[idx + 1] as u16;
                msec = Some(v as u32);
            }
            idx += 2;
            continue;
        }
        if idx >= raw.len() {
            break;
        }
        let b = raw[idx];
        match tok {
            "CC" => {
                cc_val = Some(decode_bcd_u64(&[b]) as u32);
            }
            "YY" => {
                yy_val = Some(decode_bcd_u64(&[b]) as u32);
            }
            "MM" => {
                month = Some(decode_bcd_u64(&[b]) as u32);
            }
            "DD" => {
                day = Some(decode_bcd_u64(&[b]) as u32);
            }
            "hh" => {
                hour = Some(decode_bcd_u64(&[b]) as u32);
            }
            "mm" => {
                minute = Some(decode_bcd_u64(&[b]) as u32);
            }
            "ss" => {
                second = Some(decode_bcd_u64(&[b]) as u32);
            }
            "ms" => {
                let v = decode_bcd_u64(&[b]) as u32;
                msec = Some(v * 10);
            }
            "WW" => {
                let i = (b as usize) % weekday_map.len();
                weekday_str = Some(weekday_map[i].to_string());
            }
            _ => { /* ignore unknown */ }
        }
        idx += 1;
    }

    // 计算年
    if let (Some(cc), Some(yy)) = (cc_val, yy_val) {
        year = Some(cc * 100 + yy);
    } else if let Some(yy) = yy_val {
        year = Some(2000 + yy);
    }

    // 组装输出：优先日期（YYYY年MM月DD日），再时间（HH:MM:SS[.ms]），最后可选星期
    let mut pieces: Vec<String> = Vec::new();
    if let Some(y) = year {
        pieces.push(format!("{}年", y));
    }
    if month.is_some() || day.is_some() {
        let m = month.unwrap_or(0);
        let d = day.unwrap_or(0);
        pieces.push(format!("{:02}月{:02}日", m, d));
    }

    let mut time_part = String::new();
    if hour.is_some() || minute.is_some() || second.is_some() {
        if let Some(h) = hour {
            time_part.push_str(&format!("{:02}", h));
        } else {
            time_part.push_str("00");
        }
        if let Some(mn) = minute {
            time_part.push_str(&format!(":{:02}", mn));
        } else if hour.is_some() {
            time_part.push_str(":00");
        }
        if let Some(s) = second {
            time_part.push_str(&format!(":{:02}", s));
        }
        if let Some(ms) = msec {
            time_part.push_str(&format!(".{:03}", ms % 1000));
        }
    }

    let mut result = String::new();
    if !pieces.is_empty() {
        result.push_str(&pieces.join(""));
    }
    if !time_part.is_empty() {
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(&time_part);
    }
    if result.is_empty() {
        if let Some(w) = weekday_str {
            return w;
        }
        return decode_hex(raw);
    }
    result
}

/// 解码带符号位的 bin（原码）
/// 返回 (是否为负, 清除符号位后的字节)
pub fn decode_signed_bin(raw: &[u8], endian: Endian) -> (bool, Vec<u8>) {
    let sign_byte_idx = match endian {
        Endian::Big => 0,
        Endian::Little => raw.len() - 1,
    };
    let is_negative = (raw[sign_byte_idx] & 0x80) != 0;
    let mut cleared = raw.to_vec();
    cleared[sign_byte_idx] &= 0x7F;
    (is_negative, cleared)
}

/// 解码带符号位的 bcd（原码）
/// 返回 (是否为负, 清除符号位后的字节)
pub fn decode_signed_bcd(raw: &[u8]) -> (bool, Vec<u8>) {
    let is_negative = (raw[0] & 0x80) != 0;
    let mut cleared = raw.to_vec();
    cleared[0] &= 0x7F;
    (is_negative, cleared)
}
