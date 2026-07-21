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

    // Examples for 主动上报状态字 (04001501) driven by bits_ref
    // single bit set (first bit) -> one element follows
    let mut single = vec![0x01u8];
    single.extend_from_slice(&[0u8; 11]);
    single.push(0x03);
    show_json_case("主动上报状态字_single_bit", 0x04001501, &single);

    // multiple bits set (first two bits) -> two elements follow
    let mut multi = vec![0x03u8];
    multi.extend_from_slice(&[0u8; 11]);
    multi.push(0x05);
    multi.push(0x07);
    show_json_case("主动上报状态字_multiple_bits", 0x04001501, &multi);

    // no bits set -> no repeat elements
    let none = vec![0u8; 12];
    show_json_case("主动上报状态字_no_bits", 0x04001501, &none);

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

    // 用户示例：终端以太网 MAC 地址
    // - 以太网接口数量: 1 byte, bcd
    // - 每个 MAC 地址: 6 bytes
    let mut mac_buf = Vec::new();
    mac_buf.push(0x02u8); // 两个接口
    // 第一个 MAC
    mac_buf.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    // 第二个 MAC
    mac_buf.extend_from_slice(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
    show_json_case("终端以太网MAC地址示例", 0xE0000B09, &mac_buf);

    // 用户示例：终端坐标信息 (E0000B12)
    // 结构：longitude(5 bytes: seconds(2 BCD, dec=2) + minutes(1 BCD) + degrees(1 BCD) + dir(1 BCD))
    //         latitude (same)
    //         altitude (4 bytes BCD, decimal=2, signed)
    let mut coord_buf = Vec::new();
    // longitude: 12.34s, 5', 30°, dir=1
    coord_buf.extend_from_slice(&[0x12, 0x34, 0x05, 0x30, 0x01]);
    // latitude: 56.78s, 6', 20°, dir=2
    coord_buf.extend_from_slice(&[0x56, 0x78, 0x06, 0x20, 0x02]);
    // altitude: 1.23 km -> bytes: 00 00 01 23 (BCD, decimal=2)
    coord_buf.extend_from_slice(&[0x00, 0x00, 0x01, 0x23]);
    show_json_case("终端坐标信息示例", 0xE0000B12, &coord_buf);

    // 用户示例：拓扑关系详细信息 (E1800023)
    let mut topology_buf = Vec::new();
    topology_buf.push(0x01); // 总记录条数
    topology_buf.push(0x01); // 本帧记录数(node_count)
    topology_buf.push(0x01); // 起始记录序号
    // 节点信息：一个节点，节点地址 6字节 BCD
    topology_buf.extend_from_slice(&[0x12, 0x34, 0x56, 0x78, 0x90, 0x12]);
    topology_buf.push(0x01); // 子节点数量 child_count
    // 子节点信息：长度按具体协议而定，示例这里给一个 2 字节 hex 值
    topology_buf.extend_from_slice(&[0xAA, 0xBB, 0x11, 0x22, 0x33, 0x43]);
    show_json_case("拓扑关系详细信息示例", 0xE1800023, &topology_buf);

    let mut basetask_buf = vec![0x01]; // 有效性标志: 有效
    basetask_buf.extend_from_slice(&[0x00, 0x00, 0x01, 0x01, 0x26]); // 上报基准时间 mmhhDDMMYY
    basetask_buf.push(0x00); // 定时上报周期单位: 分
    basetask_buf.push(0x05); // 定时上报周期
    basetask_buf.push(0x00); // 数据结构方式: 自描述格式
    basetask_buf.extend_from_slice(&[0x00, 0x00, 0x01, 0x01, 0x26]); // 采样基准时间
    basetask_buf.push(0x00); // 定时采样周期单位: 分
    basetask_buf.push(0x05); // 定时采样周期
    basetask_buf.push(0x01); // 数据抽取倍率
    basetask_buf.extend_from_slice(&[0x00, 0x00]); // 执行次数: 永远执行
    basetask_buf.push(0x01); // 信息点标识组数 = 1
    basetask_buf.extend_from_slice(&[0x01, 0x01]); // 信息点标识: DA1=01H,DA2=01H -> p1
    basetask_buf.push(0x01); // 数据标识编码组数 = 1
    basetask_buf.extend_from_slice(&[0x01, 0x00, 0x01, 0x00]); // 数据标识编码: DI=00010001H(小端传输)
    show_json_case("普通任务(BASETASK)", 0xE0000301, &basetask_buf);

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

    show_json_case(
        "主站通信地址",
        0xE0000100,
        &[0x0A, 0x2F, 0x12, 0xE4, 0x23, 0x29, 0x00],
    );
    println!("=== demo 运行完成 ===");
}
