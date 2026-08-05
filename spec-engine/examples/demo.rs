//! 端到端测试示例
//!
//! 演示各种 DI 解析场景，包括：
//! - 基本数据类型（BCD、Bin、ASCII）
//! - 位字段和位掩码
//! - 容器和嵌套结构
//! - 重复结构
//! - Switch 条件解析
//! - DictRef 字典引用
//! - 运行时 length_ref

use serde_json::to_string_pretty;
use spec_engine::{
    Engine, Context, Encoding, Endian, FieldLength, FieldSpec,
    BitSpec, NamedField,
};
use std::collections::HashMap;

/// 默认协议名称
const DEFAULT_PROTOCOL: &str = "csg13";
const DEFAULT_REGION: &str = "南网";
const DEFAULT_DIR: Option<&str> = None;

fn get_engine() -> Engine {
    Engine::new_default()
}

fn show_json_case(label: &str, di: u32, buf: &[u8]) {
    let engine = get_engine();
    match engine.parse_di(DEFAULT_PROTOCOL, di, DEFAULT_REGION, DEFAULT_DIR, buf) {
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

fn show_json_case_with_protocol(label: &str, protocol: &str, di: u32, region: &str, buf: &[u8]) {
    let engine = get_engine();
    match engine.parse_di(protocol, di, region, DEFAULT_DIR, buf) {
        Ok((value, consumed)) => {
            println!(
                "[OK] {label} (protocol={protocol}, DI=0x{di:08X}, consumed={consumed}/{len})",
                len = buf.len()
            );
            let json = to_string_pretty(&value).expect("序列化 JSON 失败");
            println!("{json}\n");
        }
        Err(e) => {
            println!("[ERR] {label} (protocol={protocol}, DI=0x{di:08X}, region={region}): {e}");
            panic!("解析失败: {label}: {e}");
        }
    }
}

fn show_bitmask_case(label: &str, field: &FieldSpec, buf: &[u8]) {
    let engine = get_engine();
    let mut ctx = Context::new();
    match engine.parse_field(buf, field, &mut ctx, DEFAULT_PROTOCOL, DEFAULT_REGION, DEFAULT_DIR) {
        Ok((value, consumed)) => {
            println!(
                "[OK] {label} (consumed={consumed}/{len})",
                len = buf.len()
            );
            let json = to_string_pretty(&value).expect("序列化 JSON 失败");
            println!("{json}\n");
        }
        Err(e) => {
            println!("[ERR] {label}: {e}");
            panic!("解析失败: {label}: {e}");
        }
    }
}

fn main() {
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
    show_json_case("运行状态字数据块(容器,040005FF)", 0x040005FF, &container_buf);

    // dict_ref object syntax demo: multiple data items, each with multiple points.
    // item_count: 2
    //   - first item uses DI 0x00010001 and one point
    //   - second item uses DI 0x00020001 and two points
    let dict_ref_field = FieldSpec::Container(vec![
        NamedField {
            id: None,
            ref_id: Some("item_count".to_string()),
            name: "数据项数量".to_string(),
            spec: FieldSpec::Fixed {
                encoding: Encoding::Bin {
                    endian: Endian::Little,
                    signed: false,
                },
                length: FieldLength::Fixed(1),
                unit: None,
                enum_map: None,
                format: None,
            },
            format: None,
        },
        NamedField {
            id: None,
            ref_id: None,
            name: "数据项列表".to_string(),
            spec: FieldSpec::Repeat {
                count: None,
                count_ref: Some("item_count".to_string()),
                count_expr: None,
                bits_ref: None,
                bit_direction: None,
                iterate_order: None,
                bit_specs: None,
                element: Box::new(FieldSpec::Container(vec![
                    NamedField {
                        id: None,
                        ref_id: Some("current_di".to_string()),
                        name: "数据标识".to_string(),
                        spec: FieldSpec::Fixed {
                            encoding: Encoding::Bin {
                                endian: Endian::Little,
                                signed: false,
                            },
                            length: FieldLength::Fixed(4),
                            unit: None,
                            enum_map: None,
                            format: None,
                        },
                        format: None,
                    },
                    NamedField {
                        id: None,
                        ref_id: Some("point_count".to_string()),
                        name: "采集点数量".to_string(),
                        spec: FieldSpec::Fixed {
                            encoding: Encoding::Bin {
                                endian: Endian::Little,
                                signed: false,
                            },
                            length: FieldLength::Fixed(1),
                            unit: None,
                            enum_map: None,
                            format: None,
                        },
                        format: None,
                    },
                    NamedField {
                        id: None,
                        ref_id: None,
                        name: "采集数据列表".to_string(),
                        spec: FieldSpec::Repeat {
                            count: None,
                            count_ref: Some("point_count".to_string()),
                            count_expr: None,
                            bits_ref: None,
                            bit_direction: None,
                            iterate_order: None,
                            bit_specs: None,
                            element: Box::new(FieldSpec::DictRef {
                                di_ref: "current_di".to_string(),
                            }),
                            name_template: None,
                            id_expr: None,
                        },
                        format: None,
                    },
                ])),
                name_template: None,
                id_expr: None,
            },
            format: None,
        },
    ]);

    let mut dict_ref_buf = vec![0x02];
    dict_ref_buf.extend_from_slice(&[0x01, 0x00, 0x01, 0x00]);
    dict_ref_buf.extend_from_slice(&[0x01]);
    dict_ref_buf.extend_from_slice(&[0x01, 0x00, 0x01, 0x00]);
    dict_ref_buf.extend_from_slice(&[0x01, 0x00, 0x02, 0x00]);
    dict_ref_buf.extend_from_slice(&[0x02]);
    dict_ref_buf.extend_from_slice(&[0x00, 0x05, 0x00, 0x00]);
    dict_ref_buf.extend_from_slice(&[0x23, 0x01, 0x00, 0x00]);
    
    let engine = get_engine();
    let mut ctx = Context::new();
    let (dict_ref_value, dict_ref_consumed) = engine.parse_field(
        &dict_ref_buf,
        &dict_ref_field,
        &mut ctx,
        DEFAULT_PROTOCOL,
        DEFAULT_REGION,
        DEFAULT_DIR,
    )
    .expect("dict_ref demo 解析失败");
    println!(
        "[OK] dict_ref object syntax demo (consumed={}/{})",
        dict_ref_consumed,
        dict_ref_buf.len()
    );
    println!("{}\n", to_string_pretty(&dict_ref_value).expect("序列化 JSON 失败"));

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

    let mut dlt645_0001ff00_buf = vec![0x01, 0x00, 0x00, 0x00]; // 总电能 0.01 kWh
    for _ in 0..2 {
        dlt645_0001ff00_buf.extend_from_slice(&[0x02, 0x00, 0x00, 0x00]); // 每个费率项 0.02 kWh
    }
    show_json_case_with_protocol(
        "dlt645-2007 0001FF00 (length_ref + candidate_ids)",
        "dlt645-2007",
        0x0001FF00,
        DEFAULT_REGION,
        &dlt645_0001ff00_buf,
    );

    let dlt645_05060101_buf = vec![0x68, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x68, 0x91, 0x18, 0x34, 0x34, 0x39, 0x38, 0x37, 0x36, 0x35, 0x34, 0x33, 0x33, 0x33, 0x34, 0x33, 0x33, 0x35, 0x33, 0x33, 0x36, 0x33, 0x33, 0x37, 0x33, 0x33, 0x33, 0x63, 0x16];
    show_json_case_with_protocol(
        "dlt645-2007 05060101",
        "dlt645-2007",
        0x05060101,
        DEFAULT_REGION,
        &dlt645_05060101_buf,
    );


    let dlt645_05060101_buf = vec![0x2F, 0x25, 0x07, 0x28, 0x15, 0x30, 0x01, 0xA0, 0x86, 0x01, 0x00, 0x2D, 0x00, 0x02, 0xA8, 0x61, 0x00, 0x00, 0x1E, 0x00, 0x03, 0x10, 0x27, 0x00, 0x00, 0x3C, 0x00, 0x04, 0x7C, 0x92, 0x00, 0x00, 0x19, 0x00, 0x05, 0x88, 0x13, 0x00, 0x00, 0x0F, 0x00, 0x06, 0x24, 0xF4, 0x00, 0x00, 0x32, 0x00];
    show_json_case_with_protocol(
        "dlt645-2007 05E80001",
        "dlt645-2007",
        0x05E80001,
        DEFAULT_REGION,
        &dlt645_05060101_buf,
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

    let mut e301_buf = vec![0u8; 256];
    e301_buf[0] = 0x02; // 仅第 2 个测量点需要升级
    show_json_case("E3010006 待升级电表地址列表", 0xE3010006, &e301_buf);

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
    //         altitude (4 bytes BCD, decimals=2, signed)
    let mut coord_buf = Vec::new();
    // longitude: 12.34s, 5', 30°, dir=1
    coord_buf.extend_from_slice(&[0x12, 0x34, 0x05, 0x30, 0x01]);
    // latitude: 56.78s, 6', 20°, dir=2
    coord_buf.extend_from_slice(&[0x56, 0x78, 0x06, 0x20, 0x02]);
    // altitude: 1.23 km -> bytes: 00 00 01 23 (BCD, decimals=2)
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

    // 注意：DI 0x0101FF00 和其候选费率项在字典里都使用了 runtime length_ref。
    // 这里用真实的 DI show_json_case 演示，而不是自构造 Context。 

    // 电表管理单元透传上送告警（ARD42模板）：报文内容长度由前面的"报文长度"
    // 字段驱动（length_ref），验证变长尾部能正确按引用字段的值切出来。
    let mut ard42_buf = vec![0x00u8]; // 告警状态: 恢复
    ard42_buf.extend_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06]); // 告警发生时间
    ard42_buf.extend_from_slice(&[0x03, 0x00]); // 报文长度 = 3（bin，小端）
    ard42_buf.extend_from_slice(&[0xAA, 0xBB, 0xCC]); // 报文内容，应正好被吃掉3字节
    show_json_case("电表管理单元透传上送告警(ARD42)", 0xE2000084, &ard42_buf);
    println!("=== demo 运行完成 ===");

    // bitmask demo
    let mut bitmask_buf = vec![0x00u8];
    bitmask_buf.extend_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
    bitmask_buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    show_json_case("bitmask demo", 0x00000000, &bitmask_buf);

    let bitmask_field = FieldSpec::BitMask {
        length: 1,
        bit_direction: Some("lsb".to_string()),
        iterate_order: Some("asc".to_string()),
        bit_specs: vec![
            BitSpec {
                range: (0, 0),
                name: "bit0".to_string(),
                ref_id: None,
                enum_map: None,
            },
            BitSpec {
                range: (1, 1),
                name: "bit1".to_string(),
                ref_id: None,
                enum_map: None,
            },
        ],
        element: Box::new(FieldSpec::Switch {
            on: "$bit_value".to_string(),
            cases: {
                let mut m = HashMap::new();
                m.insert("0".to_string(), Box::new(FieldSpec::Skip));
                m.insert(
                    "1".to_string(),
                    Box::new(FieldSpec::Fixed {
                        encoding: Encoding::Raw,
                        length: FieldLength::Fixed(0),
                        unit: None,
                        enum_map: None,
                        format: None,
                    }),
                );
                m
            },
            case_names: None,
            default: None,
        }),
        name_template: Some("测量点{index}".to_string()),
    };
    show_bitmask_case("bitmask 替代 bitpattern 示例", &bitmask_field, &[0x02]);

    println!("=== demo 运行完成 ===");
}
