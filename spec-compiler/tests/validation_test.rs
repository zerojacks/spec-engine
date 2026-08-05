//! 测试 YAML schema 验证功能

use spec_compiler::ast::RawDict;

#[test]
#[should_panic(expected = "unknown field `decimal`")]
fn test_unknown_field_decimal() {
    // 这个测试期望在反序列化时因为未知字段而 panic
    // 正确的字段名是 decimals（复数），不是 decimal（单数）
    let yaml = r#"
data_items:
  - id: "TEST0001"
    name: "测试BCD字段"
    protocol: "test"
    type: bcd
    length: 4
    decimal: 2
    unit: "kWh"
"#;
    
    let _dict: RawDict = serde_yaml::from_str(yaml).unwrap();
}

#[test]
#[should_panic(expected = "unknown field `count_reference`")]
fn test_unknown_field_typo() {
    // 测试拼写错误：count_reference 应该是 count_ref
    let yaml = r#"
data_items:
  - id: "TEST0002"
    name: "测试 Repeat 字段"
    protocol: "test"
    type: repeat
    count_reference: test_count
    element:
      type: fixed
      length: 1
"#;
    
    let _dict: RawDict = serde_yaml::from_str(yaml).unwrap();
}

#[test]
fn test_valid_schema() {
    // 测试正确的 schema 可以成功解析
    let yaml = r#"
data_items:
  - id: "TEST0003"
    name: "正确的BCD字段"
    protocol: "test"
    type: bcd
    length: 4
    decimals: 2
    unit: "kWh"
"#;
    
    let dict: RawDict = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(dict.data_items.len(), 1);
}
