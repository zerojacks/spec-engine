# spec-engine —— 基于 YAML Schema 的规范解析与表生成库

按照《DI字典YAML Schema设计》文档实现：YAML 描述数据字典 → `build.rs` 编译期展开成
Rust 源码 → 运行时只有一个统一的 `parse_field` 递归函数处理全部节点类型。
## 项目结构

```
spec-engine/
├── build.rs              # YAML → Rust 代码生成（编译期展开 template/di_sequence）
├── schema/
│   └── test_di.yaml      # DI 字典源文件，覆盖全部15种类型组合
├── src/
│   ├── lib.rs             # 对外入口：parse_di / get_di_table
│   ├── types.rs           # FieldSpec / Encoding / Value / BitSpec / NamedField
│   ├── context.rs         # 解析作用域（原始字节 + 已解码值 双绑定）
│   ├── decode.rs          # bcd/bin/ascii/hex/time 底层解码 + 符号位处理
│   ├── parser.rs          # parse_field 统一递归入口
│   ├── error.rs           # DictError
│   └── registry.rs        # external/custom 处理器注册表
└── examples/
    └── demo.rs            # 15组端到端测试，覆盖字典里每一种类型组合
```

## 运行验证

```bash
cargo build              # 编译（build.rs 从 schema/*.yaml 生成 DI 表）
cargo run --example demo # 直接输出 JSON 解析结果，按真实字节验证每个类型组合
```

`cargo run --example demo` 的输出现在会对每个类型组合打印解析结果的 JSON，
无需内部断言即可观察实际解析结构。全部成功时会打印 `=== demo 运行完成 ===`。

## 解析输出统一命名

为了让解析结果更一致，`parse_container()` 的 `Map` key 现在和 `Value::Node.name` 一致：

- 对于带 `id` 的字段，Map key 会使用完整节点名 `ID_名称`，
- 而不是仅使用裸字段名。

这样像 `0101FF00` 这种带子字段 `01010000` 的容器，
在 JSON 输出里会出现类似：

- Map key `01010000_(当前)正向有功总最大需量及发生时间`
- 对应 `Node.name` 也会是 `01010000_(当前)正向有功总最大需量及发生时间`

这项统一改动让节点寻址和输出结构更清晰，也便于后续通过 ID+名称直接定位字段。 

## 覆盖的类型组合（对应 schema/test_di.yaml 里的编号）

| # | DI | 类型 | 验证点 |
|---|-----|------|--------|
| 1 | 00010001 | bcd + enum | enum命中/未命中两种路径 |
| 4 | 00010004 | time | 格式化时间戳 |
| 6 | 00020002 | bin, little, signed | 符号位位置由 endian 推导 |
| 7 | 00020003 | bin, signed（单字节） | endian 不影响的边界情况 |
| 9 | E0000100 | switch | 条件分支命中 case 与落到 default |
| 10 | 00030000 | repeat + template | 计数重复 + 模板复用 |
| 11 | 040005FF / 04000501 | 容器 + 双重身份 | 子字段可单独寻址 |
| 13 | 00050000 | di_sequence | 跨DI拼接 |
| 15 | 00070000 | custom | 逃生舱处理器 |
| 16 | 00080000 | region | 省份专有定义命中、省份专有枚举值、未覆盖省份回退通用定义、显式default |


同一个 DI 码在不同省份/局方可能有不同定义，多数 DI 是通用的，只有少数需要
按省覆盖——不希望每个省份都把全部 DI 抄一遍。设计跟 `switch` 的
"cases + default"是同一个思路：

  （`"default"`）这个通用桶里。
  也可以显式写 `region:` 覆盖（比如一个通用容器里只有某一个子字段需要
  省份专有定义）。
  再回退查 `(di, DEFAULT_REGION)`，两者都查不到才是真的未知 DI。
  `di_sequence`/`dict_ref` 递归引用其他 DI 时，走的是同一个 region 上下文
  （一份报文来自哪个省份，报文里所有 DI 引用都该在那个省份下解释）。
  优先取"当前 region 下那个 DI 该有的样子"，查不到才内联通用定义。
  panic（数据冲突，必须显式解决）；同一个 `id` 在不同 `region` 下出现
parse_di(0x00080000, spec_engine::DEFAULT_REGION, &[0x01])?; // -> "尖峰谷分时电价"

```yaml
  name: 电价类型（通用定义）
  length: 1
  type: bcd
  enum:
    "01": 尖峰谷分时电价
    "02": 阶梯电价

  region: "GD"                # 广东专有定义，覆盖通用定义
  name: 电价类型（广东定义）
  length: 1
  type: bcd
  enum:
    "01": 目录电价
    "02": 市场化电价
    "03": 阶梯电价
```

```rust,ignore
parse_di(0x00080000, "GD", &[0x02])?;              // -> "市场化电价"（命中GD专有定义）
parse_di(0x00080000, "SC", &[0x02])?;               // -> "阶梯电价"（SC无专有定义，回退通用）
parse_di(0x00080000, spec_engine::DEFAULT_REGION, &[0x01])?; // -> "尖峰谷分时电价"
```

## 在其他项目中使用 / 运行时加载

默认情况下，`spec-engine` 在构建时把 `schema/*.yaml` 展开并把生成的 `di_table.bin`
通过 `include_bytes!(concat!(env!("OUT_DIR"), "/di_table.bin"))` 打包进二进制。
这对大多数用法最简单且效率最高。但如果你希望在运行时从外部文件加载字典
（例如把新的 `di_table.bin` 部署到服务器并在下次启动时使用），可以在程序
启动阶段调用：

```rust
use spec_engine::init_di_table_from_file;

fn main() -> Result<(), Box<dyn std::error::Error>> {
  // 在首次调用 parse_di 之前加载外部 di_table.bin（可选）
  init_di_table_from_file("/path/to/di_table.bin")?;
  // 然后正常使用 parse_di / init_registries 等
  Ok(())
}
```

注意：目前该函数只会在全局表尚未初始化时生效；若要在进程运行中替换已加载表，
请在发布版中使用运行时热重载（`Arc<RwLock<...>>`）变体或通过重启进程来切换。


## 验证过程中发现并修复的问题

这份代码不是凭空写的，是拿到手之后实际装进容器、`cargo build` 编译、
再用真实字节跑一遍每种类型组合验证出来的。过程中发现了几个只靠读代码
不容易看出来、必须跑起来才会暴露的问题：

1. **`decode.rs` 里 `decode_ascii`/`decode_hex`/`decode_time` 各自被定义了两次**
   （大概率是拼接时的复制粘贴问题），直接导致编译失败（E0428），已去重。

2. **`lib.rs` 的 `pub use decode::{...}` 少导出了 `decode_ascii`/`decode_time`**，
   而 `parser.rs` 用 `use super::{decode_ascii, decode_time, ...}` 引用它们，
   导致 `unresolved imports`，已修正导出列表。

3. **`parser.rs` 里五处 `xxx.clone()` 传给期望 `String` 的 `DictError` 变体，
   但 `xxx` 类型是 `&str`**（`on`/`count_ref`/`protocol`/`handler`/`di_ref` 都是
   `String`），导致 `E0308` 类型不匹配，已全部改成 `.to_string()`。

   `count_ref`）、`parse_external`（读 `length: Ref(...)`）、`parse_dict_ref`
   （读 `ref` 指向的 DI 码）这三处，都硬编码了 `decode_bin_u64(&raw, Endian::Big)`
   是 `endian: little` 还是 `endian: big`。跑 `dict_ref` 那组测试时（`current_di`
   字段声明的是默认小端），如果按这三处原来的写法解析，会把小端字节错误地
   当大端数字读，日志 DI 会算错、或者因为长度算错导致 `UnexpectedEof`。
   **修复方式**：`Context` 现在对每个绑定的字段同时保存原始字节（给 `switch`
   按十六进制 key 匹配用）和已经用该字段自己声明的编码方式解出来的 `Value`
   （给 `repeat`/`external`/`dict_ref` 按数值引用用），三处硬编码的
   `Endian::Big` 全部改成读 `ctx.get_value(...).as_usize()/.as_u32()`，
   不再自己瞎猜字节序。

5. **【实测发现的测试数据冲突】`schema/test_di.yaml` 里独立的位域测试
   （原 `04000501`）和"双重身份"测试（`040005FF` 容器内同样 id 为
   `04000501` 的子字段）用了同一个 DI 码但给出了两份不同的字段定义**——
   `build.rs` 原来的逻辑是後注册的直接覆盖前面的，不会报错，相当于第一份
   定义被静默丢弃。已经把独立测试项改名为 `04000509` 避免冲突，同时给
   `build.rs` 加了一个检测：**同一个 DI 码如果被注册了两次且生成的代码不
   一样就直接 `panic`**，把这类字典本身的定义冲突从"运行时用错哪份定义都
   不知道"提前到"编译期直接报错说清楚是哪个 DI 冲突"。完全相同的重复注册
   （比如同一个 DI 被 `di_sequence` 引用了好几次）仍然被允许，不受影响。

6. **【实测发现的真实bug】`src/parser.rs` 的 `lookup_di` 返回值引用了入参
   `table`，但函数签名没有标注生命周期**（`table`/`region` 两个引用参数,
   返回值究竟借用自哪一个，编译器无法推断），触发 `E0106`。这处不是这次
   加 region 改出来的，是更早一版加 region 支持时遗留的，一直没有跑
   `cargo build` 验证过，这次接着实现 `build.rs` 的 region 贯穿时顺带
   跑起来才暴露。已加 `<'a>` 生命周期标注修复。

7. **`build.rs` 加 region 支持后重新跑了一遍冲突检测的负面测试**：临时插入
   一条跟已有 `(id="00080000", region="GD")` 冲突的定义，确认 `cargo build`
   会在编译期直接 panic 并准确报出是哪个 DI、哪个 region 冲突，而不是
   静默用后者覆盖前者——冲突检测的 key 从纯 `id` 换成 `(id, region)` 之后
   这个保护机制还是有效的，验证完已从字典里移除这条临时冲突项。

## 已知的简化 / 后续可以做的事

  因为"bit0是哪个物理位"本来就需要按真实报文核实，这里选大端只是一个
  需要在实际对接协议时验证的默认假设，可按需要调整）。
  （`registry.rs`），真正对接协议时把 `parse_dlt645_demo` 换成真正的
   DL/T 645 解析实现即可，不需要改 `spec-engine` 库本身的其它部分。
  （比如具体是字典里哪个字段解析失败），如果要在生产环境定位问题，
  建议给 `DictError` 加一个字段路径（如 `"040005FF.密钥状态"`）方便定位。
