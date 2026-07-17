//! 端到端测试：对字典里覆盖到的每一种组合类型构造真实字节，跑一遍 parse_di，
//! 并用 assert 核对结果，确保不只是"编译通过"而是"解析结果正确"。

use spec_engine::{init_registries, parse_di, Value};

/// 默认协议名称
const DEFAULT_PROTOCOL: &str = "csg13";
const DEFAULT_REGION: &str = "南网";
const DEFAULT_DIR: Option<&str> = None;

fn show_case(label: &str, di: u32, buf: &[u8]) -> Value {
    match parse_di(DEFAULT_PROTOCOL, di, DEFAULT_REGION, DEFAULT_DIR, buf) {
        Ok((value, consumed)) => {
            println!(
                "[OK] {label} (DI=0x{di:08X}, consumed={consumed}/{len})",
                len = buf.len()
            );
            println!("{}", value.format_tree());
            value
        }
        Err(e) => {
            println!("[ERR] {label} (DI=0x{di:08X}, region={DEFAULT_REGION}): {e}");
            panic!("解析失败: {label}: {e}");
        }
    }
}

fn assert_basic_bcd_examples() {
    let v = show_case(
        "月冻结正向有功总电能(bcd,南网)",
        0x00010001,
        &[0x01, 0x00, 0x00, 0x00],
    );
    assert_eq!(
        v,
        Value::Node {
            name: "00010001_月冻结正向有功总电能".to_string(),
            raw: vec![0x01, 0x00, 0x00, 0x00],
            value: Box::new(Value::WithUnit {
                value: Box::new(Value::Float(0.01)),
                unit: "kWh".to_string(),
            }),
        }
    );

    let v = show_case(
        "月冻结正向有功总电能(bcd,南网,另一值)",
        0x00010001,
        &[0x23, 0x01, 0x00, 0x00],
    );
    assert_eq!(
        v,
        Value::Node {
            name: "00010001_月冻结正向有功总电能".to_string(),
            raw: vec![0x23, 0x01, 0x00, 0x00],
            value: Box::new(Value::WithUnit {
                value: Box::new(Value::Float(1.23)),
                unit: "kWh".to_string(),
            }),
        }
    );

    let v = show_case(
        "月冻结反向有功总电能(bcd,南网)",
        0x00020001,
        &[0x00, 0x05, 0x00, 0x00],
    );
    assert_eq!(
        v,
        Value::Node {
            name: "00020001_月冻结反向有功总电能".to_string(),
            raw: vec![0x00, 0x05, 0x00, 0x00],
            value: Box::new(Value::WithUnit {
                value: Box::new(Value::Float(5.0)),
                unit: "kWh".to_string(),
            }),
        }
    );
}

fn assert_bitfield_case() {
    let v = show_case("运行状态字1(bitfield,南网)", 0x04000501, &[0x00, 0x02]);
    match &v {
        Value::Node { value, .. } => match value.as_ref() {
                Value::Map(entries) => {
                assert!(entries.iter().any(|(k, _)| k.contains("需量积算方式")));
            }
            _ => panic!("期望内部 Map"),
        },
        _ => panic!("期望 Node"),
    }
}

fn assert_container_case() {
    let mut buf = vec![0x00u8, 0x02];
    buf.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    buf.extend_from_slice(&[0; 8]);
    buf.extend_from_slice(&[0; 4]);
    let _ = show_case("运行状态字数据块(容器,040005FF)", 0x040005FF, &buf);
}

fn assert_subfield_case() {
    let _ = show_case(
        "组合无功1总电能(子字段,南网)",
        0x00030000,
        &[0x00, 0x12, 0x34, 0x56],
    );
    let _ = show_case(
        "组合无功2总电能(子字段,南网)",
        0x00040000,
        &[0x00, 0x12, 0x34, 0x56],
    );
}

fn assert_candidate_ids_cases() {
    let v = show_case(
        "00000100_(当前)组合有功费率1电能",
        0x00000100,
        &[0x23, 0x01, 0x00, 0x00],
    );
    match &v {
        Value::Node { name, value, .. } => {
            assert!(name.contains("费率1电能"));
            match value.as_ref() {
                Value::WithUnit { value, unit } => {
                    assert_eq!(unit, "kWh");
                    assert_eq!(value.as_ref(), &Value::Float(1.23));
                }
                other => panic!("unexpected candidate_id value: {other:?}"),
            }
        }
        other => panic!("unexpected root value: {other:?}"),
    }

    let v = show_case(
        "00030400_(当前)组合有功费率4电能",
        0x00030400,
        &[0x23, 0x01, 0x00, 0x00],
    );
    match &v {
        Value::Node { name, value, .. } => {
            assert!(name.contains("费率4电能"));
            match value.as_ref() {
                Value::WithUnit { value, unit } => {
                    assert_eq!(unit, "kWh");
                    assert_eq!(value.as_ref(), &Value::Float(1.23));
                }
                other => panic!("unexpected candidate_id value: {other:?}"),
            }
        }
        other => panic!("unexpected root value: {other:?}"),
    }
}

fn assert_0001ff00_case() {
    // 构造更严格的示例：rate_count = 6，4 字节总电能，然后 6 个不相同的 4 字节 BCD
    let mut buf = vec![0x06u8];
    // 总电能（4 字节）：任意值
    buf.extend_from_slice(&[0x00, 0x12, 0x34, 0x56]);

    // 6 个不同的 BCD 编码值：1.23, 2.34, 3.45, 4.56, 5.67, 6.78
    let rates: [[u8; 4]; 6] = [
        [0x23, 0x01, 0x00, 0x00],
        [0x34, 0x02, 0x00, 0x00],
        [0x45, 0x03, 0x00, 0x00],
        [0x56, 0x04, 0x00, 0x00],
        [0x67, 0x05, 0x00, 0x00],
        [0x78, 0x06, 0x00, 0x00],
    ];
    for r in &rates {
        buf.extend_from_slice(r);
    }

    let v = show_case("(当前)正向有功电能数据块(0001FF00) 6 费率", 0x0001FF00, &buf);

    // 递归查找首个 Value::List（代表重复字段的解析结果）并断言内容
    fn find_first_list(v: &Value) -> Option<&Vec<Value>> {
        match v {
            Value::List(items) => Some(items),
            Value::Map(entries) => {
                for (_, val) in entries {
                    if let Some(l) = find_first_list(val) {
                        return Some(l);
                    }
                }
                None
            }
            Value::Node { value, .. } => find_first_list(value.as_ref()),
            Value::WithUnit { value, .. } => find_first_list(value.as_ref()),
            _ => None,
        }
    }

    if let Some(list) = find_first_list(&v) {
        assert_eq!(list.len(), 6, "应解析出 6 个费率条目");
        // 检查前几个元素的数值
        for (i, item) in list.iter().enumerate() {
            // item 期望是 Value::Node 包含 WithUnit -> Float
            match item {
                Value::Node { value, .. } => match value.as_ref() {
                    Value::WithUnit { value, unit } => {
                        assert_eq!(unit, "kWh");
                        let expected = match i {
                            0 => 1.23,
                            1 => 2.34,
                            2 => 3.45,
                            3 => 4.56,
                            4 => 5.67,
                            5 => 6.78,
                            _ => unreachable!(),
                        };
                        assert_eq!(value.as_ref(), &Value::Float(expected));
                    }
                    other => panic!("unexpected item payload: {other:?}"),
                },
                other => panic!("unexpected list item shape: {other:?}"),
            }
        }
    } else {
        panic!("未在解析结果中找到重复字段的列表值");
    }
}

fn assert_repeat_case() {
    let v = show_case(
        "终端APP列表信息(repeat)",
        0xE1800032,
        &[
            0x01, b'A', b'P', b'P', b'N', b'A', b'M', b'E', b'A', b'V', b'1', b'.', b'0', b'.',
            b'0', b' ', b' ', b'A', b'B', b'0', b'1', b'0', b'1', b'0', b'0', b'0', b'0', b'1',
            b'0', b'0', b'1', b'0', b'0', b'0',
        ],
    );
    match &v {
        Value::Node { value, .. } => match value.as_ref() {
            Value::Map(entries) => {
                let list = entries.iter().find(|(k, _)| k == "APP信息列表").unwrap();
                match &list.1 {
                    Value::Node { value, .. } => match value.as_ref() {
                        Value::List(items) => assert_eq!(items.len(), 1),
                        _ => panic!("期望重复字段解析为 List"),
                    },
                    _ => panic!("期望重复字段包装为 Node"),
                }
            }
            _ => panic!("期望重复字段解析为 Map"),
        },
        _ => panic!("期望 Node"),
    }
}

fn assert_switch_and_custom_cases() {
    let v = show_case("终端登录消息(switch)", 0xE0001001, &[]);
    match &v {
        Value::Node { value, .. } => match value.as_ref() {
            Value::Map(entries) => assert!(entries.is_empty()),
            _ => panic!("期望空 Map"),
        },
        _ => panic!("期望 Node"),
    }

    let v = show_case(
        "主站通信地址(E0000100)",
        0xE0000100,
        &[0x0A, 0x2F, 0x12, 0xE4, 0x23, 0x29, 0x00, 0x00, 0x02],
    );
    match &v {
        Value::Node {
            name, raw, value, ..
        } => {
            assert_eq!(name, "E0000100_主站通信地址");
            assert_eq!(
                raw,
                &vec![0x0A, 0x2F, 0x12, 0xE4, 0x23, 0x29, 0x00, 0x00, 0x02]
            );
            match value.as_ref() {
                Value::Map(entries) => {
                    let address = entries.iter().find(|(k, _)| k == "主通信地址").unwrap();
                    assert_eq!(
                        address.1,
                        Value::Node {
                            name: "主通信地址".to_string(),
                            raw: vec![0x0A, 0x2F, 0x12, 0xE4, 0x23, 0x29, 0x00, 0x00],
                            value: Box::new(Value::Str("10.47.18.228:9001".to_string())),
                        }
                    );
                }
                _ => panic!("期望内部 Map"),
            }
        }
        _ => panic!("期望 Node"),
    }
}

fn main() {
    init_registries();
    println!("=== DI 字典解析 —— 全类型组合端到端验证 ===\n");

    assert_basic_bcd_examples();
    assert_bitfield_case();
    assert_container_case();
    assert_subfield_case();
    assert_candidate_ids_cases();
    assert_0001ff00_case();
    assert_repeat_case();
    assert_switch_and_custom_cases();

    println!("\n=== demo 运行通过 ===");
}
