use spec_engine::{Engine, Context, FieldSpec, FieldLength, Encoding, Endian, NamedField, Value};

fn get_engine() -> Engine {
    Engine::new_default()
}

#[test]
fn length_ref_remaining_runtime_demo() {
    // 构造一个容器：header(2 bytes) + tail(remaining)
    let fields = vec![
        NamedField {
            id: None,
            ref_id: None,
            name: "header".to_string(),
            spec: FieldSpec::Fixed {
                encoding: Encoding::Bin {
                    endian: Endian::Little,
                    signed: false,
                },
                length: FieldLength::Fixed(2),
                unit: None,
                enum_map: None,
                format: None,
            },
            format: None,
        },
        NamedField {
            id: None,
            ref_id: None,
            name: "tail".to_string(),
            spec: FieldSpec::Fixed {
                encoding: Encoding::Raw,
                length: FieldLength::Ref("$remaining".to_string()),
                unit: None,
                enum_map: None,
                format: None,
            },
            format: None,
        },
    ];

    let spec = FieldSpec::Container(fields);

    // buffer: 2 bytes header + 3 bytes tail
    let buf: Vec<u8> = vec![0x11, 0x22, 0xAA, 0xBB, 0xCC];

    // 预先把 $remaining 绑定到上下文（运行时通常由外层容器/调用处绑定）
    let mut ctx = Context::new();
    // bind raw empty, value = Int(remaining_bytes)
    ctx.bind("$remaining", vec![], Value::Int((buf.len() - 2) as i64));

    let engine = get_engine();
    let (value, consumed) = engine.parse_field(&buf, &spec, &mut ctx, "dlt645-2007", "南网", None)
        .expect("parse failed");

    assert_eq!(consumed, buf.len());

    // 简单断言解析结果包含 header 和 tail，并且 tail 长度为 3
    if let Value::Map(entries) = value {
        let mut found_header = false;
        let mut found_tail = false;
        for (k, v) in entries {
            if k.contains("header") {
                found_header = true;
                // header 应解析为整数 0x2211（小端）或类似值，根据 Bin 解码
            }
            if k.contains("tail") {
                found_tail = true;
                // tail raw bytes 长度应为 3
                if let Value::Node { raw, .. } = v {
                    assert_eq!(raw.len(), 3);
                } else {
                    panic!("tail not a node");
                }
            }
        }
        assert!(found_header && found_tail, "missing header/tail entries");
    } else {
        panic!("container did not yield a Map");
    }
}

#[test]
fn repeat_count_expr_runtime_demo() {
    let fields = vec![
        NamedField {
            id: None,
            ref_id: None,
            name: "header".to_string(),
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
            ref_id: None,
            name: "values".to_string(),
            spec: FieldSpec::Repeat {
                count: None,
                count_ref: None,
                count_expr: Some("$remaining / 4".to_string()),
                bits_ref: None,
                bit_direction: None,
                iterate_order: None,
                bit_specs: None,
                element: Box::new(FieldSpec::Fixed {
                    encoding: Encoding::Bin {
                        endian: Endian::Little,
                        signed: false,
                    },
                    length: FieldLength::Fixed(4),
                    unit: None,
                    enum_map: None,
                    format: None,
                }),
                name_template: Some("item{index}".to_string()),
                id_expr: None,
            },
            format: None,
        },
    ];

    let spec = FieldSpec::Container(fields);
    let buf: Vec<u8> = vec![0x01, 0x02, 0x03, 0x04, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];
    let mut ctx = Context::new();
    
    let engine = get_engine();
    let (value, consumed) = engine.parse_field(&buf, &spec, &mut ctx, "dlt645-2007", "南网", None)
        .expect("parse failed");

    assert_eq!(consumed, buf.len());
    if let Value::Map(entries) = value {
        assert!(entries.iter().any(|(k, _)| k.contains("header")));
        let values_node = entries
            .iter()
            .find(|(k, _)| k.contains("values"))
            .expect("missing values node");
        if let Value::Node { value, .. } = &values_node.1 {
            if let Value::List(items) = value.as_ref() {
                assert_eq!(items.len(), 2);
            } else {
                panic!("values node did not contain a List");
            }
        } else {
            panic!("values entry is not a Node");
        }
    } else {
        panic!("container did not yield a Map");
    }
}
