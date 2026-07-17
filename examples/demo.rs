//! 端到端测试：对字典里覆盖到的每一种组合类型构造真实字节，跑一遍 parse_di，
//! 并将解析结果直接输出为 JSON。

use serde_json::to_string_pretty;
use spec_engine::{init_registries, parse_di};

/// 默认协议名称
const DEFAULT_PROTOCOL: &str = "csg13";
const DEFAULT_REGION: &str = "南网";
const DEFAULT_DIR: Option<&str> = None;

fn show_json_case(label: &str, di: u32, buf: &[u8]) {
    match parse_di(DEFAULT_PROTOCOL, di, DEFAULT_REGION, DEFAULT_DIR, buf) {
        Ok((value, consumed)) => {
            println!(
                "[OK] {label} (DI=0x{di:08X}, consumed={consumed}/{len})",
                len = buf.len()
            );
            let json = to_string_pretty(&value).expect("序列化 JSON 失败");
            println!("{json}\n");
        }
        Err(e) => {
            println!("[ERR] {label} (DI=0x{di:08X}, region={DEFAULT_REGION}): {e}");
            panic!("解析失败: {label}: {e}");
        }
    }
}

fn main() {
    init_registries();
    println!("=== DI 字典解析 —— JSON 输出示例 ===\n");

    show_json_case("月冻结正向有功总电能(bcd,南网)", 0x00010001, &[0x01, 0x00, 0x00, 0x00]);
    show_json_case("月冻结正向有功总电能(bcd,南网,另一值)", 0x00010001, &[0x23, 0x01, 0x00, 0x00]);
    show_json_case("月冻结反向有功总电能(bcd,南网)", 0x00020001, &[0x00, 0x05, 0x00, 0x00]);
    show_json_case("运行状态字1(bitfield,南网)", 0x04000501, &[0x00, 0x02]);

    let mut container_buf = vec![0x00u8, 0x02];
    container_buf.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    container_buf.extend_from_slice(&[0; 8]);
    container_buf.extend_from_slice(&[0; 4]);
    show_json_case("运行状态字数据块(容器,040005FF)", 0x040005FF, &container_buf);

    show_json_case(
        "组合无功1总电能(子字段,南网)",
        0x00030000,
        &[0x00, 0x12, 0x34, 0x56],
    );
    show_json_case(
        "组合无功2总电能(子字段,南网)",
        0x00040000,
        &[0x00, 0x12, 0x34, 0x56],
    );

    show_json_case(
        "00000100_(当前)组合有功费率1电能",
        0x00000100,
        &[0x23, 0x01, 0x00, 0x00],
    );
    show_json_case(
        "00030400_(当前)组合有功费率4电能",
        0x00030400,
        &[0x23, 0x01, 0x00, 0x00],
    );

    let mut ff00_buf = vec![0x06u8];
    ff00_buf.extend_from_slice(&[0x00, 0x12, 0x34, 0x56]);
    let ff00_rates: [[u8; 4]; 6] = [
        [0x23, 0x01, 0x00, 0x00],
        [0x34, 0x02, 0x00, 0x00],
        [0x45, 0x03, 0x00, 0x00],
        [0x56, 0x04, 0x00, 0x00],
        [0x67, 0x05, 0x00, 0x00],
        [0x78, 0x06, 0x00, 0x00],
    ];
    for r in &ff00_rates {
        ff00_buf.extend_from_slice(r);
    }
    show_json_case(
        "(当前)正向有功电能数据块(0001FF00) 6 费率",
        0x0001FF00,
        &ff00_buf,
    );

    show_json_case(
        "终端APP列表信息(repeat)",
        0xE1800032,
        &[
            0x01, b'A', b'P', b'P', b'N', b'A', b'M', b'E', b'A', b'V', b'1', b'.', b'0', b'.',
            b'0', b' ', b' ', b'A', b'B', b'0', b'1', b'0', b'1', b'0', b'0', b'0', b'0', b'1',
            b'0', b'0', b'1', b'0', b'0', b'0',
        ],
    );

    show_json_case("终端登录消息(switch)", 0xE0001001, &[]);

    let mut ff00_0101_buf = vec![0x03u8];
    ff00_0101_buf.extend_from_slice(&[0x12, 0x00, 0x00]);
    ff00_0101_buf.extend_from_slice(&[0x01, 0x01, 0x01, 0x01, 0x20]);
    let entries: [[u8; 8]; 3] = [
        [0x11, 0x00, 0x00, 0x02, 0x01, 0x01, 0x01, 0x20],
        [0x22, 0x00, 0x00, 0x03, 0x02, 0x02, 0x02, 0x20],
        [0x33, 0x00, 0x00, 0x04, 0x03, 0x03, 0x03, 0x20],
    ];
    for e in &entries {
        ff00_0101_buf.extend_from_slice(e);
    }
    show_json_case(
        "(当前)正向有功最大需量及发生时间数据块(0101FF00)",
        0x0101FF00,
        &ff00_0101_buf,
    );

    println!("=== demo 运行完成 ===");
}
