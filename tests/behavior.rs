use spec_engine::{init_registries, parse_di, parse_field, Context, DictError, Encoding, FieldLength, FieldSpec, BitSpec, Value};
use std::collections::HashMap;
use std::sync::Once;

static INIT: Once = Once::new();

fn setup() {
    INIT.call_once(|| init_registries());
}

fn parse_case(di: u32, buf: &[u8]) -> (Value, usize) {
    setup();
    parse_di("csg13", di, "南网", None, buf).expect("测试用例解析失败")
}

#[test]
fn parses_fixed_bcd_values_normally() {
    let (value, consumed) = parse_case(0x00010001, &[0x01, 0x00, 0x00, 0x00]);

    assert_eq!(consumed, 4);
    match value {
        Value::Node {
            name, raw, value, ..
        } => {
            assert_eq!(name, "00010001_月冻结正向有功总电能");
            assert_eq!(raw, vec![0x01, 0x00, 0x00, 0x00]);
            match value.as_ref() {
                Value::WithUnit { value, unit } => {
                    assert_eq!(unit, "kWh");
                    assert_eq!(value.as_ref(), &Value::Float(0.01));
                }
                other => panic!("unexpected value shape: {other:?}"),
            }
        }
        other => panic!("unexpected root value: {other:?}"),
    }
}

#[test]
fn parses_bitfield_values() {
    let (value, consumed) = parse_case(0x04000501, &[0x00, 0x02]);

    assert_eq!(consumed, 2);
    match value {
        Value::Node { value, .. } => match value.as_ref() {
            Value::List(items) => {
                // ensure at least one item name contains the target substring
                assert!(items.iter().any(|item| match item {
                    Value::Node { name, .. } => name.contains("需量积算方式"),
                    _ => false,
                }));
            }
            other => panic!("expected list payload, got {other:?}"),
        },
        other => panic!("unexpected root value: {other:?}"),
    }
}

#[test]
fn parses_container_and_nested_subfield() {
    let (value, consumed) = parse_case(0x00030000, &[0x00, 0x12, 0x34, 0x56]);

    assert_eq!(consumed, 4);
    match value {
        Value::Node { name, value, .. } => {
            assert!(name.contains("组合无功1总电能"));
            match value.as_ref() {
                Value::WithUnit { value, unit } => {
                    assert_eq!(unit, "kvarh");
                    assert_eq!(value.as_ref(), &Value::Float(563412.0));
                }
                other => panic!("unexpected nested payload: {other:?}"),
            }
        }
        other => panic!("unexpected root value: {other:?}"),
    }
}

#[test]
fn parses_repeat_structures() {
    let mut buf = vec![0x01];
    buf.extend_from_slice(b"APPNAMEA");
    buf.extend_from_slice(b"V1.0.0  ");
    buf.extend_from_slice(b"AB");
    buf.extend_from_slice(b"010");
    buf.extend_from_slice(b"1");
    buf.extend_from_slice(b"0000");
    buf.extend_from_slice(b"100");
    buf.extend_from_slice(b"0");

    let (value, consumed) = parse_case(0xE1800032, &buf);

    assert_eq!(consumed, buf.len());
    match value {
        Value::Node { value, .. } => match value.as_ref() {
            Value::Map(entries) => {
                let list = entries
                    .iter()
                    .find(|(k, _)| k == "APP信息列表")
                    .expect("APP信息列表 missing");
                match &list.1 {
                    Value::Node { value, .. } => match value.as_ref() {
                        Value::List(items) => {
                            assert_eq!(items.len(), 1);
                            assert!(matches!(items[0], Value::Node { .. }));
                        }
                        other => panic!("expected repeat payload list, got {other:?}"),
                    },
                    other => panic!("expected nested APP信息列表 node, got {other:?}"),
                }
            }
            other => panic!("expected repeat payload map, got {other:?}"),
        },
        other => panic!("unexpected root value: {other:?}"),
    }
}

#[test]
fn parses_bitmask_with_skip_branch() {
    let field = FieldSpec::BitMask {
        length: 1,
        bit_direction: Some("lsb".to_string()),
        iterate_order: Some("asc".to_string()),
        bit_specs: vec![
            BitSpec {
                range: (0, 0),
                name: "low_bit".to_string(),
                ref_id: None,
                enum_map: None,
            },
            BitSpec {
                range: (1, 1),
                name: "high_bit".to_string(),
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
            case_names: Some({
                let mut names = HashMap::new();
                names.insert("0".to_string(), "无需升级".to_string());
                names.insert("1".to_string(), "需要升级".to_string());
                names
            }),
            default: None,
        }),
        name_template: Some("测量点{index}".to_string()),
    };

    let mut ctx = Context::new();
    let (value, consumed) = parse_field(&[0x02], &field, &mut ctx, "csg13", "南网", None)
        .expect("parse bitmask failed");
    assert_eq!(consumed, 1);
    match value {
        Value::List(items) => {
            assert_eq!(items.len(), 1);
            match &items[0] {
                Value::Node { name, value, .. } => {
                    assert_eq!(name, "测量点2");
                    match value.as_ref() {
                        Value::Bit { bit_start, bit_end, bit_value, bit_byte, value } => {
                            assert_eq!(*bit_start, 1);
                            assert_eq!(*bit_end, 1);
                            assert_eq!(*bit_value, 1);
                            assert_eq!(bit_byte, &vec![0x02]);
                            assert_eq!(value.as_deref(), Some(&Value::Str("需要升级".to_string())));
                        }
                        other => panic!("unexpected inner value: {other:?}"),
                    }
                }
                other => panic!("unexpected list item: {other:?}"),
            }
        }
        other => panic!("unexpected top-level bitmask value: {other:?}"),
    }
}

#[test]
fn parses_e1800023_topology_repeat_item_name_and_big_endian_hex() {
    let buf = [
        0x01, // 总记录条数
        0x01, // 本帧记录数
        0x01, // 起始记录序号
        0x12, 0x34, 0x56, 0x78, 0x90, 0x12, // 节点地址
        0x01, // 子节点数量
        0xAA, 0xBB, 0x11, 0x22, 0x33, 0x43, // 子节点信息 (hex, big endian)
    ];

    let (value, consumed) = parse_case(0xE1800023, &buf);
    assert_eq!(consumed, buf.len());

    match value {
        Value::Node { value, .. } => match value.as_ref() {
            Value::Map(entries) => {
                let node_info = entries
                    .iter()
                    .find(|(k, _)| k == "节点信息")
                    .expect("节点信息 missing");
                match &node_info.1 {
                    Value::Node { value, .. } => match value.as_ref() {
                        Value::List(items) => {
                            assert_eq!(items.len(), 1);
                            match &items[0] {
                                Value::Node { name, value, .. } => {
                                    assert_eq!(name, "第1条节点信息");
                                    match value.as_ref() {
                                        Value::Map(entries) => {
                                            let child_list = entries
                                                .iter()
                                                .find(|(k, _)| k == "子节点信息")
                                                .expect("子节点信息 missing");
                                            match &child_list.1 {
                                                Value::Node { value, .. } => match value.as_ref() {
                                                    Value::List(child_items) => {
                                                        assert_eq!(child_items.len(), 1);
                                                        match &child_items[0] {
                                                            Value::Node { name, value, .. } => {
                                                                assert_eq!(name, "第1个子节点");
                                                                assert_eq!(
                                                                    value.as_ref(),
                                                                    &Value::Str("AABB11223343".to_string())
                                                                );
                                                            }
                                                            other => panic!("expected child node, got {other:?}"),
                                                        }
                                                    }
                                                    other => panic!("expected child repeat list, got {other:?}"),
                                                },
                                                other => panic!("expected 子节点信息 node, got {other:?}"),
                                            }
                                        }
                                        other => panic!("expected node payload map, got {other:?}"),
                                    }
                                }
                                other => panic!("expected repeat item node, got {other:?}"),
                            }
                        }
                        other => panic!("expected repeat payload list, got {other:?}"),
                    },
                    other => panic!("expected 节点信息 node, got {other:?}"),
                }
            }
            other => panic!("expected root payload map, got {other:?}"),
        },
        other => panic!("unexpected root value: {other:?}"),
    }
}

#[test]
fn child_field_node_name_uses_id_name_when_id_present_but_ref_id_is_hidden() {
    let mut buf = vec![0x01];
    buf.extend_from_slice(b"APPNAMEA");
    buf.extend_from_slice(b"V1.0.0  ");
    buf.extend_from_slice(b"AB");
    buf.extend_from_slice(b"010");
    buf.extend_from_slice(b"1");
    buf.extend_from_slice(b"0000");
    buf.extend_from_slice(b"100");
    buf.extend_from_slice(b"100");
    buf.extend_from_slice(b"0");

    let (value, _) = parse_case(0xE1800032, &buf);

    match value {
        Value::Node { value, .. } => match value.as_ref() {
            Value::Map(entries) => {
                let app_count_node = entries
                    .iter()
                    .find(|(k, _)| k == "APP数量")
                    .expect("APP数量 missing");
                match &app_count_node.1 {
                    Value::Node { name, .. } => assert_eq!(name, "APP数量"),
                    other => panic!("expected node for APP数量, got {other:?}"),
                }
            }
            other => panic!("expected map payload, got {other:?}"),
        },
        other => panic!("unexpected root value: {other:?}"),
    }
}

#[test]
fn parses_candidate_ids_generated_dis() {
    let (value, consumed) = parse_case(0x00000100, &[0x23, 0x01, 0x00, 0x00]);
    assert_eq!(consumed, 4);

    match value {
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
}

#[test]
fn parses_template_ip_with_port() {
    let (value, consumed) = parse_case(
        0xE0000100,
        &[0x0A, 0x2F, 0x12, 0xE4, 0x23, 0x29, 0x00],
    );

    assert_eq!(consumed, 7);
    match value {
        Value::Node { value, .. } => match value.as_ref() {
            Value::Map(entries) => {
                let address = entries
                    .iter()
                    .find(|(k, _)| k == "主通信地址")
                    .expect("address field missing");
                match &address.1 {
                    Value::Node { name, raw, value } => {
                        assert_eq!(name, "主通信地址");
                        assert_eq!(raw, &vec![0x0A, 0x2F, 0x12, 0xE4, 0x23, 0x29]);
                        match value.as_ref() {
                            Value::Map(subentries) => {
                                let ip = subentries
                                    .iter()
                                    .find(|(k, _)| k == "IP地址")
                                    .expect("IP地址 missing");
                                assert_eq!(ip.1, Value::Node {
                                    name: "IP地址".to_string(),
                                    raw: vec![0x0A, 0x2F, 0x12, 0xE4],
                                    value: Box::new(Value::Str("10.47.18.228".to_string())),
                                });
                                let port = subentries
                                    .iter()
                                    .find(|(k, _)| k == "端口号")
                                    .expect("端口号 missing");
                                assert_eq!(port.1, Value::Node {
                                    name: "端口号".to_string(),
                                    raw: vec![0x23, 0x29],
                                    value: Box::new(Value::Int(9001)),
                                });
                            }
                            other => panic!("expected nested map payload, got {other:?}"),
                        }
                    }
                    other => panic!("expected 主通信地址 node, got {other:?}"),
                }
            }
            other => panic!("expected map payload, got {other:?}"),
        },
        other => panic!("unexpected root value: {other:?}"),
    }
}

#[test]
fn parses_switch_cases() {
    let (value, consumed) = parse_case(0xE0001001, &[]);

    assert_eq!(consumed, 0);
    match value {
        Value::Node { value, .. } => match value.as_ref() {
            Value::Map(entries) => assert!(entries.is_empty()),
            other => panic!("expected empty map for switch default case, got {other:?}"),
        },
        other => panic!("unexpected root value: {other:?}"),
    }
}

#[test]
fn returns_error_for_unknown_di() {
    let err = parse_di("csg13", 0xFFFF0000, "南网", None, &[]).unwrap_err();
    assert!(matches!(err, DictError::UnknownDi { .. }));
}

#[test]
fn returns_error_when_buffer_is_too_short() {
    let err = parse_di("csg13", 0x00010001, "南网", None, &[0x01]).unwrap_err();
    assert!(matches!(err, DictError::UnexpectedEof { .. }));
}

#[test]
fn parses_0001ff00_minimal_exists() {
    // minimal check: crate knows about DI 0x0001FF00 and returns UnexpectedEof on too-short buffer
    let err = parse_di("csg13", 0x0001FF00, "南网", None, &[0x00]).unwrap_err();
    assert!(matches!(err, DictError::UnexpectedEof { .. }));
}

#[test]
fn parses_0001ff00_repeat_item_names_include_id() {
    fn collect_leaf_names<'a>(value: &'a Value, names: &mut Vec<String>) {
        match value {
            Value::Node { name, value, .. } => {
                names.push(name.clone());
                collect_leaf_names(value.as_ref(), names);
            }
            Value::WithUnit { value, .. } => collect_leaf_names(value.as_ref(), names),
            Value::Map(entries) => {
                for (_, v) in entries {
                    collect_leaf_names(v, names);
                }
            }
            Value::List(items) => {
                for item in items {
                    collect_leaf_names(item, names);
                }
            }
            _ => {}
        }
    }

    // 2 rates, plus the 4-byte total energy field
    let mut buf = vec![0x02u8];
    buf.extend_from_slice(&[0x00, 0x12, 0x34, 0x56]);
    buf.extend_from_slice(&[0x23, 0x01, 0x00, 0x00]);
    buf.extend_from_slice(&[0x23, 0x01, 0x00, 0x00]);

    let (value, consumed) = parse_case(0x0001FF00, &buf);
    assert_eq!(consumed, buf.len());

    let mut names = Vec::new();
    collect_leaf_names(&value, &mut names);
    assert!(names.iter().any(|n| n.starts_with("00010100_")));
    assert!(names.iter().any(|n| n.starts_with("00010200_")));
}

#[test]
fn parses_0101ff00_repeat_inner_names_are_expanded() {
    let mut buf = vec![0x03u8];
    buf.extend_from_slice(&[0x12, 0x00, 0x00]);
    buf.extend_from_slice(&[0x01, 0x01, 0x01, 0x01, 0x20]);
    buf.extend_from_slice(&[0x11, 0x00, 0x00, 0x02, 0x01, 0x01, 0x01, 0x20]);
    buf.extend_from_slice(&[0x22, 0x00, 0x00, 0x03, 0x02, 0x02, 0x02, 0x20]);
    buf.extend_from_slice(&[0x33, 0x00, 0x00, 0x04, 0x03, 0x03, 0x03, 0x20]);

    let (value, consumed) = parse_case(0x0101FF00, &buf);
    assert_eq!(consumed, buf.len());

    fn collect_leaf_names<'a>(value: &'a Value, names: &mut Vec<String>) {
        match value {
            Value::Node { name, value, .. } => {
                names.push(name.clone());
                collect_leaf_names(value.as_ref(), names);
            }
            Value::WithUnit { value, .. } => collect_leaf_names(value.as_ref(), names),
            Value::Map(entries) => {
                for (_, v) in entries {
                    collect_leaf_names(v, names);
                }
            }
            Value::List(items) => {
                for item in items {
                    collect_leaf_names(item, names);
                }
            }
            _ => {}
        }
    }

    let mut names = Vec::new();
    collect_leaf_names(&value, &mut names);
    assert!(names.iter().any(|n| n == "(当前)正向有功费率1最大需量"));
    assert!(names.iter().any(|n| n == "(当前)正向有功费率2最大需量"));
    assert!(names.iter().any(|n| n == "(当前)正向有功费率3最大需量"));
    assert!(!names.iter().any(|n| n.contains("{index}")));
}

#[test]
fn parses_additional_candidate_id() {
    // reuse same payload shape as other candidate_id tests to ensure generated ids resolve
    let (value, consumed) = parse_case(0x00000200, &[0x23, 0x01, 0x00, 0x00]);
    assert_eq!(consumed, 4);
    match value {
        Value::Node { name, value, .. } => {
            assert!(name.contains("费率1电能") || name.contains("费率"));
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

#[test]
fn parses_0001ff00_with_6_rates_container() {
    // rate_count = 6, then a 4-byte 总电能 field, then 6 * 4 bytes BCD values
    let mut buf = vec![0x06u8];
    // (当前)正向有功总电能 (4 bytes)
    buf.extend_from_slice(&[0x00, 0x12, 0x34, 0x56]);
    for _ in 0..6 {
        buf.extend_from_slice(&[0x23, 0x01, 0x00, 0x00]);
    }

    let (value, consumed) = parse_case(0x0001FF00, &buf);
    assert_eq!(consumed, buf.len());
    match value {
        Value::Node { value, .. } => match value.as_ref() {
            Value::Map(entries) => {
                // basic sanity: container should include the fee count field
                assert!(entries.iter().any(|(k, _)| k == "费率数"));
            }
            _ => panic!("expected container Map for 0001FF00"),
        },
        _ => panic!("expected Node root for 0001FF00"),
    }
}

#[test]
fn parses_0001ff00_generated_6_rates() {
    // Verify generated candidate IDs 0x00010100..0x00010600 parse correctly
    let payload = [0x23, 0x01, 0x00, 0x00];
    let base: u32 = 0x00010100;
    for i in 0..6 {
        let di = base + (i * 0x00000100);
        let (value, consumed) = parse_case(di, &payload);
        assert_eq!(consumed, 4, "DI 0x{:08X} consumed bytes mismatch", di);
        match value {
            Value::Node { name, value, .. } => {
                assert!(name.contains("费率") || name.contains("费率1"));
                match value.as_ref() {
                    Value::WithUnit { value, unit } => {
                        assert_eq!(unit, "kWh");
                        assert_eq!(value.as_ref(), &Value::Float(1.23));
                    }
                    other => panic!("unexpected candidate_id value: {other:?}"),
                }
            }
            other => panic!("unexpected root value for DI 0x{:08X}: {other:?}", di),
        }
    }
}

#[test]
fn parses_04001501_single_bit_set() {
    // first bit set -> one element follows (1 byte BCD)
    let mut buf = vec![0x01u8];
    buf.extend_from_slice(&[0u8; 11]); // total 12 bytes for the bitfield
    buf.push(0x03); // element value for the set bit

    let (value, consumed) = parse_case(0x04001501, &buf);
    assert_eq!(consumed, 13);

    match value {
        Value::Node { value, .. } => match value.as_ref() {
            Value::Map(entries) => {
                let mut hits = Vec::new();
                for (k, v) in entries.iter() {
                    if k.contains("新增次数") {
                        if let Value::Node { value, .. } = v {
                            if let Value::WithUnit { value: inner, unit } = value.as_ref() {
                                if unit == "次" {
                                    if let Value::Int(i) = inner.as_ref() {
                                        hits.push(*i);
                                    }
                                }
                            }
                        }
                    }
                }
                assert_eq!(hits.len(), 1, "expected exactly one 新增次数 entry");
                assert_eq!(hits[0], 3);
            }
            other => panic!("expected map payload, got {other:?}"),
        },
        other => panic!("unexpected root value: {other:?}"),
    }
}

#[test]
fn parses_04001501_multiple_bits_set() {
    // first two bits set -> two elements follow
    let mut buf = vec![0x03u8];
    buf.extend_from_slice(&[0u8; 11]);
    buf.push(0x05);
    buf.push(0x07);

    let (value, consumed) = parse_case(0x04001501, &buf);
    assert_eq!(consumed, 14);

    match value {
        Value::Node { value, .. } => match value.as_ref() {
            Value::Map(entries) => {
                let mut hits = Vec::new();
                for (k, v) in entries.iter() {
                    if k.contains("新增次数") {
                        if let Value::Node { value, .. } = v {
                            if let Value::WithUnit { value: inner, unit } = value.as_ref() {
                                if unit == "次" {
                                    if let Value::Int(i) = inner.as_ref() {
                                        hits.push(*i);
                                    }
                                }
                            }
                        }
                    }
                }
                assert_eq!(hits.len(), 2, "expected exactly two 新增次数 entries");
                assert_eq!(hits[0], 5);
                assert_eq!(hits[1], 7);
            }
            other => panic!("expected map payload, got {other:?}"),
        },
        other => panic!("unexpected root value: {other:?}"),
    }
}

#[test]
fn parses_04001501_no_bits_set() {
    // all zeros -> no repeat elements
    let buf = vec![0u8; 12];
    let (value, consumed) = parse_case(0x04001501, &buf);
    assert_eq!(consumed, 12);

    match value {
        Value::Node { value, .. } => match value.as_ref() {
            Value::Map(entries) => {
                // ensure no '新增次数' keys present
                assert!(entries.iter().all(|(k, _)| !k.contains("新增次数")));
            }
            other => panic!("expected map payload, got {other:?}"),
        },
        other => panic!("unexpected root value: {other:?}"),
    }
}

#[test]
fn parses_basetask_template_with_info_point_and_di_code() {
    // E0000301 "普通任务" -> template_ref: BASETASK，端到端验证
    // 信息点标识(info_point)和数据标识编码(di_code)在真实报文树里联动工作
    let mut buf = vec![0x01]; // 有效性标志: 有效
    buf.extend_from_slice(&[0x00, 0x00, 0x01, 0x01, 0x26]); // 上报基准时间 mmhhDDMMYY
    buf.push(0x00); // 定时上报周期单位: 分
    buf.push(0x05); // 定时上报周期
    buf.push(0x00); // 数据结构方式: 自描述格式
    buf.extend_from_slice(&[0x00, 0x00, 0x01, 0x01, 0x26]); // 采样基准时间
    buf.push(0x00); // 定时采样周期单位: 分
    buf.push(0x05); // 定时采样周期
    buf.push(0x01); // 数据抽取倍率
    buf.extend_from_slice(&[0x00, 0x00]); // 执行次数: 永远执行
    buf.push(0x01); // 信息点标识组数 = 1
    buf.extend_from_slice(&[0x01, 0x01]); // 信息点标识: DA1=01H,DA2=01H -> p1
    buf.push(0x01); // 数据标识编码组数 = 1
    buf.extend_from_slice(&[0x01, 0x00, 0x01, 0x00]); // 数据标识编码: DI=00010001H(小端传输)

    let (value, consumed) = parse_case(0xE0000301, &buf);
    assert_eq!(consumed, buf.len());

    match value {
        Value::Node { value, .. } => match value.as_ref() {
            Value::Map(entries) => {
                let info_point_entry = entries
                    .iter()
                    .find(|(k, _)| k == "信息点标识")
                    .expect("缺少信息点标识条目");
                match &info_point_entry.1 {
                    Value::Node { value, .. } => match value.as_ref() {
                        Value::List(items) => {
                            assert_eq!(items.len(), 1);
                            match &items[0] {
                                Value::Node { name, value, .. } => {
                                    assert_eq!(name, "第1组信息点");
                                    let expected_inner = Value::List(vec![Value::Pn(1)]);
                                    assert_eq!(value.as_ref(), &expected_inner);
                                }
                                other => panic!("unexpected info_point item: {other:?}"),
                            }
                        }
                        other => panic!("unexpected info_point value: {other:?}"),
                    },
                    other => panic!("unexpected info_point shape: {other:?}"),
                }

                let di_entry = entries
                    .iter()
                    .find(|(k, _)| k == "数据标识编码")
                    .expect("缺少数据标识编码条目");
                match &di_entry.1 {
                    Value::Node { value, .. } => match value.as_ref() {
                        Value::List(items) => {
                            assert_eq!(items.len(), 1);
                            match &items[0] {
                                Value::Node { name, value, .. } => {
                                    assert_eq!(name, "第1组数据标识编码");
                                    assert_eq!(value.as_ref(), &Value::Str(
                                        "00010001_月冻结正向有功总电能".to_string()
                                    ));
                                }
                                other => panic!("unexpected di_code item: {other:?}"),
                            }
                        }
                        other => panic!("unexpected di_code value: {other:?}"),
                    },
                    other => panic!("unexpected di_code node: {other:?}"),
                }
            }
            other => panic!("expected map payload, got {other:?}"),
        },
        other => panic!("unexpected root value: {other:?}"),
    }
}

#[test]
fn bitmask_can_replace_bitpattern_style_named_bit_list() {
    let field = FieldSpec::BitMask {
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

    let mut ctx = Context::new();
    let (value, consumed) = parse_field(&[0x02], &field, &mut ctx, "csg13", "南网", None)
        .expect("bitmask bitpattern replacement failed");

    assert_eq!(consumed, 1);
    match value {
        Value::List(items) => {
            assert_eq!(items.len(), 1);
            match &items[0] {
                Value::Node { name, value, .. } => {
                    assert_eq!(name, "测量点2");
                    match value.as_ref() {
                        Value::Bit { bit_start, bit_end, bit_value, bit_byte, value } => {
                            assert_eq!(*bit_start, 1);
                            assert_eq!(*bit_end, 1);
                            assert_eq!(*bit_value, 1);
                            assert_eq!(bit_byte, &vec![0x02]);
                            assert!(matches!(value.as_deref(), Some(Value::Bytes(bytes)) if bytes.is_empty()));
                        }
                        other => panic!("unexpected inner value: {other:?}"),
                    }
                }
                other => panic!("unexpected list item: {other:?}"),
            }
        }
        other => panic!("unexpected top-level bitmask value: {other:?}"),
    }
}
