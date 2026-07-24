# spec-engine —— 基于 YAML Schema 的规范解析与表生成库

`spec-engine` 以 YAML 描述的 DI 字典为核心，编译期将字典展开成 Rust 代码，运行时只保留一个统一的递归解析引擎。

## 核心设计

- `schema/*.yaml` 定义 DI 字典、字段类型、长度、编码、单位、枚举、region 覆盖等。
- `build.rs` 在编译期读取 YAML，展开 `di_sequence`、模板、region 覆盖等结构，并生成最终的 DI 表。
- 运行时通过 `parse_di` / `parse_field` 递归解析报文，生成结构化 `Value` 树。
- `Value` 结构支持：整型、浮点、字符串、字节、列表、Map、带单位值、解析节点、位域、跳过、无效值等。
- 对于非法 BCD / 解析失败情况，当前设计不会误判为 `0`，而是生成 `Value::Invalid { reason }`，保留字段失败信息。

## 项目结构

```
spec-engine/
├── build.rs              # YAML → Rust 代码生成与 DI 表构建
├── schema/               # 主 DI 字典源文件
│   └── test_di.yaml      # 测试用 DI 字典，覆盖各种类型组合
├── src/
│   ├── lib.rs            # 对外入口：parse_di / parse_field / init_registries 等
│   ├── types.rs          # FieldSpec / Encoding / Value / BitSpec / FormatSpec
│   ├── context.rs        # 解析上下文：原始字节 + 已解码值 双向绑定
│   ├── decode.rs         # 低级解码：BCD/BIN/ASCII/HEX/TIME + 符号位处理
│   ├── parser.rs         # 统一递归解析函数及辅助逻辑
│   ├── error.rs          # DictError 定义
│   └── registry.rs       # external/custom 处理器注册和查找
└── examples/
    └── demo.rs          # 端到端演示程序，按真实字节验证解析结果
```

## 主要特性

- 支持多种编码：`Bin`、`Bcd`、`Ascii`、`Hex`、`Time`、`Raw`
- 支持带单位值：`WithUnit { value, unit }`
- 支持字段命名节点：`Node { name, raw, value }`
- 支持 `List` / `Map` / `Bit` / `Skip`
- 支持 `Value::Invalid { reason }`，用于表达解析失败而不是硬编码为默认值
- 支持 `switch` / `repeat` / `template` / `di_sequence` / `dict_ref`
- 支持 region 覆盖：优先命中当前 region 定义，回退到 `DEFAULT_REGION`
- 运行时输出一致：容器 `Map` key 与 `Node.name` 保持统一，便于 ID+名称定位

## 解析值设计

`Value` 的当前设计包括：

- `Int(i64)`
- `Float(f64)`
- `Str(String)`
- `Bytes(Vec<u8>)`
- `List(Vec<Value>)`
- `Map(Vec<(String, Value)>)`
- `WithUnit { value: Box<Value>, unit: String }`
- `Node { name: String, raw: Vec<u8>, value: Box<Value> }`
- `Bit { ... }`
- `Skip`
- `Invalid { reason: String }`
- `Pn(i64)`

其中 `Invalid` 用于表达字段解析失败，如非法 BCD 码、超出范围的时间编码等。当前实现不会把非法 BCD 直接当成 `0`，而是保留失败原因，让上层消费时能区分“真实 0”与“无效值”。

## region 与 DI 覆盖模型

- 多省份/多局方数据字典可以共存。
- 普通定义写在默认 region 下，特殊 region 通过 `region: "GD"` 之类覆盖。
- 查找顺序：当前 region -> `DEFAULT_REGION`。
- 如果两者都不存在，才会报 `UnknownDi`。
- `di_sequence` / `dict_ref` 的解析仍在当前 region 上下文中执行，保证同一报文内所有 DI 解释一致。

示例：

```yaml
name: 电价类型（通用定义）
length: 1
type: bcd
enum:
  "01": 尖峰谷分时电价
  "02": 阶梯电价

region: "GD"
name: 电价类型（广东定义）
length: 1
type: bcd
enum:
  "01": 目录电价
  "02": 市场化电价
  "03": 阶梯电价
```

```rust
parse_di("csg13", 0x00080000, "GD", None, &[0x02])?; // -> 市场化电价
parse_di("csg13", 0x00080000, "SC", None, &[0x02])?; // -> 阶梯电价
```

## 运行验证

```bash
cargo build
cargo run --example demo
```

`cargo run --example demo` 会对每个测试数据组输出实际解析结果的 JSON，便于观察结构和字段值。当前版本在 demo 运行结束时会打印 `=== demo 运行完成 ===`。

## 典型问题与修复方向

该项目已经实测过编译和真实字节解析，多次发现并修复了以下类问题：

- 编译期生成的 `build.rs` 易产生重复定义或冲突，已在构建逻辑中增加 DI 冲突检测。
- 解析逻辑必须同时保留字段原始字节与解码结果，才能支持 `switch` 按 raw key 匹配和 `dict_ref`/`repeat` 按数值引用。
- 非法 BCD 不能再被误判为 `0`，当前设计改为 `Value::Invalid`。
- `Map` key 与 `Node.name` 的一致性提升了输出可读性和字段定位能力。

## 运行时加载与扩展

默认行为是将 `schema/*.yaml` 在编译期展开为 `di_table.bin` 并内嵌到二进制中。
如果需要运行时加载外部字典，可以使用：

```rust
use spec_engine::init_di_table_from_file;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_di_table_from_file("/path/to/di_table.bin")?;
    Ok(())
}
```

> 注意：该加载接口只在全局 DI 表尚未初始化时生效。要动态替换已加载表，建议在应用层引入 `Arc<RwLock<...>>` 或通过重启进程实现热切换。
