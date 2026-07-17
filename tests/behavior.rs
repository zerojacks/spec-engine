use spec_engine::{init_registries, parse_di, DictError, Value};
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
            Value::Map(entries) => {
                assert!(entries.iter().any(|(k, _)| k.contains("需量积算方式")));
            }
            other => panic!("expected map payload, got {other:?}"),
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
                            assert!(matches!(items[0], Value::Map(_)));
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
fn parses_custom_handlers() {
    let (value, consumed) = parse_case(
        0xE0000100,
        &[0x0A, 0x2F, 0x12, 0xE4, 0x23, 0x29, 0x00, 0x00, 0x02],
    );

    assert_eq!(consumed, 9);
    match value {
        Value::Node { value, .. } => match value.as_ref() {
            Value::Map(entries) => {
                let address = entries
                    .iter()
                    .find(|(k, _)| k == "主通信地址")
                    .expect("address field missing");
                assert_eq!(
                    address.1,
                    Value::Node {
                        name: "主通信地址".to_string(),
                        raw: vec![0x0A, 0x2F, 0x12, 0xE4, 0x23, 0x29, 0x00, 0x00],
                        value: Box::new(Value::Str("10.47.18.228:9001".to_string())),
                    }
                );
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
