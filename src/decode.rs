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
    // 简化实现：假设 format 是 "ssmmhhDDMMYY" 这类顺序
    if format == "ssmmhhDDMMYY" && raw.len() >= 6 {
        let ss = decode_bcd_u64(&[raw[0]]);
        let mm = decode_bcd_u64(&[raw[1]]);
        let hh = decode_bcd_u64(&[raw[2]]);
        let dd = decode_bcd_u64(&[raw[3]]);
        let mo = decode_bcd_u64(&[raw[4]]);
        let yy = decode_bcd_u64(&[raw[5]]);
        return format!(
            "20{:02}-{:02}-{:02} {:02}:{:02}:{:02}",
            yy, mo, dd, hh, mm, ss
        );
    }
    decode_hex(raw)
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
