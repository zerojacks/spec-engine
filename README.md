# spec-engine — 电力通信协议解析引擎

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

`spec-engine` 是一个强大的电力通信协议解析引擎，专为处理中国电力行业的各种通信协议（如 DL/T645、Q/CSG1209022 等）而设计。它支持编译期静态字典和运行时动态加载两种模式。

## 核心特性

### 📦 编译期静态字典
- 从 YAML 文件编译生成嵌入式二进制字典
- 零运行时开销的字典查找
- 支持 35,000+ DI 条目的快速访问

### 🔥 运行时动态加载
- 支持动态加载 YAML 字典层
- 层级优先级：后加载的层覆盖先加载的层
- 自动回退到基础字典

### 🎯 多编码支持
- **BCD**: Binary-Coded Decimal，电力行业常用编码
- **Binary**: 二进制整数（支持大小端、有符号/无符号）
- **ASCII**: ASCII 字符串
- **Hex**: 十六进制字符串
- **Time**: 时间格式（多种编码方式）
- **Raw**: 原始字节

### ⚡ 高级解析特性
- **位域解析**: 将字节拆分为位字段
- **条件分支**: 根据前面字段的值选择不同的解析规则
- **重复结构**: 支持计数重复和位图重复
- **动态长度**: 字段长度可引用前面字段的值
- **外部协议**: 支持嵌套解析其他协议的报文
- **区域覆盖**: 按省份/局方自定义 DI 定义，自动回退到通用定义

## 项目结构

```
spec-engine/
├── spec-compiler/           # 字典编译器
│   ├── src/
│   │   ├── lib.rs          # 编译器主接口
│   │   ├── types.rs        # 类型系统（FieldSpec、Encoding、Value 等）
│   │   ├── ast.rs          # YAML AST 定义
│   │   ├── compiler.rs     # 编译逻辑
│   │   ├── validator.rs    # 验证器
│   │   ├── generator.rs    # 代码生成
│   │   └── repeat.rs       # 重复结构处理
│
├── spec-engine/             # 运行时解析引擎
│   ├── build.rs            # 编译期字典构建
│   ├── src/
│   │   ├── lib.rs          # 对外入口（parse_di、create_dynamic_catalog 等）
│   │   ├── parser.rs       # 递归解析引擎
│   │   ├── decode.rs       # 低级解码（BCD/BIN/ASCII/HEX/TIME）
│   │   ├── context.rs      # 解析上下文
│   │   ├── error.rs        # 错误类型
│   │   ├── registry.rs     # 外部协议和自定义处理器注册
│   │   ├── dynamic_loader.rs  # 动态字典加载器
│   │   └── prelude.rs      # 常用 API 导出
│   └── examples/
│       ├── simple_usage.rs           # 基础解析示例
│       ├── dynamic_catalog_demo.rs   # 动态字典加载示例
│       ├── dump_di_table.rs          # 字典导出工具
│       └── list_keys.rs              # 列出所有 DI 条目
│
├── spec-tools/              # 命令行工具
│   └── src/
│       └── main.rs         # CLI 命令（compile、query、stats、validate）
│
└── schema/                  # DI 字典源文件（YAML）
    ├── csg13/              # 南方电网 Q/CSG1209022-2019（CSG-1.3）
    ├── csg16/              # 南方电网 Q/CSG1209022-2019（CSG-1.6）
    ├── dlt645-2007/        # 国标 DL/T 645-2007
    └── di.schema.json      # YAML Schema 定义
```

## 快速开始

### 安装

在 `Cargo.toml` 中添加：

```toml
[dependencies]
spec-engine = "0.2"
```

### 基础解析

```rust
use spec_engine::{parse_di, DEFAULT_REGION};

// 报文数据（BCD 编码的电能值）
let data = vec![0x34, 0x12, 0x00, 0x00];  // 表示 1234.00 kWh

// 按 DI 码解析
let (value, consumed) = parse_di(
    "csg13",           // 协议名称
    0x00010000,        // DI 标识
    DEFAULT_REGION,    // 区域（"南网"）
    None,              // 方向（None 表示通用）
    &data,             // 报文数据
)?;

println!("解析结果: {:?}", value);
println!("消耗字节: {}", consumed);
```

### 简化 API

```rust
use spec_engine::parse;

let (value, consumed) = parse(0x00010000, "csg13", &data)?;
```

### 动态字典加载

```rust
use spec_engine::create_dynamic_catalog;

// 创建动态字典管理器
let mut catalog = create_dynamic_catalog();

// 加载自定义层
catalog.load_yaml_dir("custom".into(), "config/custom")?;

// 查找（优先从 custom 层查找）
if let Some(field) = catalog.lookup("csg13", 0x00010000, "南网", None) {
    println!("字段名: {}", field.name);
}

// 层管理
println!("已加载的层: {:?}", catalog.list_layers());
catalog.stats().print();
```

## 命令行工具

### 安装

```bash
cargo install --path spec-tools
```

### 编译字典

```bash
# 编译 YAML 为二进制文件
spec-tools compile --input schema --output di_table.bin

# 查看统计信息
spec-tools stats --dict di_table.bin

# 查询单个 DI
spec-tools query --dict di_table.bin --di 00010000 --protocol csg13

# 验证 YAML 格式
spec-tools validate --input schema
```

## YAML 字典格式

### 简单示例

```yaml
data_items:
  - id: "00010000"
    name: "组合有功总电能"
    protocol: "csg13"
    region: ["南网"]
    length: 4
    type: bcd
    decimals: 2
    unit: "kWh"
```

### 使用模板

```yaml
templates:
  - id: "energy_template"
    fields:
      - name: "电能值"
        length: 4
        type: bcd
        decimals: 2
        unit: "kWh"

data_items:
  - id: "00010000"
    name: "总电能"
    template_ref: "energy_template"
```

### 重复结构展开

```yaml
data_items:
  - id: "0000FF00"
    name: "电能数据块"
    fields:
      - candidate_ids:
          count: 63
          id_expr: "0x00010100 + index0*0x0100"
          name_template: "费率{index}电能"
          element:
            length: 4
            type: bcd
            decimals: 2
            unit: "kWh"
```

详细的 YAML Schema 定义请参考 [`schema/di.schema.json`](schema/di.schema.json) 和 [`docs/DI字典YAML_Schema设计.md`](docs/DI字典YAML_Schema设计.md)。

## 区域覆盖（Region Override）

多省份/多局方数据字典可以共存，支持按区域覆盖定义：

```yaml
# 通用定义
- id: "00080000"
  name: "电价类型"
  region: ["南网"]
  length: 1
  type: bcd
  enum:
    "01": "尖峰谷分时电价"
    "02": "阶梯电价"

# 广东省定义（覆盖）
- id: "00080000"
  name: "电价类型"
  region: ["广东"]
  length: 1
  type: bcd
  enum:
    "01": "目录电价"
    "02": "市场化电价"
    "03": "阶梯电价"
```

查找时自动回退：

```rust
// 查找广东定义
parse_di("csg13", 0x00080000, "广东", None, &[0x02])?; // -> "市场化电价"

// 查找四川定义（回退到南网通用定义）
parse_di("csg13", 0x00080000, "四川", None, &[0x02])?; // -> "阶梯电价"
```

## 运行示例

```bash
# 构建项目
cargo build --workspace

# 运行基础示例
cargo run --example simple_usage

# 运行动态字典示例
cargo run --example dynamic_catalog_demo

# 导出字典
cargo run --example dump_di_table

# 列出所有 DI
cargo run --example list_keys
```

## 架构设计

### 编译流程

```
YAML 文件
    ↓
spec-compiler 解析
    ↓
AST (RawDict, RawField)
    ↓
验证与展开（模板、重复结构）
    ↓
FieldSpec 树
    ↓
bincode 序列化
    ↓
di_table.bin (嵌入到可执行文件)
```

### 运行流程

```
用户调用 parse_di(protocol, di, region, dir, data)
    ↓
查找字典 (protocol, di, region, dir)
    ↓
获取 NamedField 和 FieldSpec
    ↓
递归遍历 FieldSpec 树
    ↓
调用 decode 函数解析字节
    ↓
生成 Value 树
```

### 动态字典架构

```
DynamicCatalog
  ├─ embedded: 编译时嵌入的静态字典（优先级最低）
  └─ layers: 运行时加载的动态层（按加载顺序）
      ├─ Layer 1: 基础字典
      ├─ Layer 2: 区域扩展
      └─ Layer 3: 客户定制（优先级最高）
```

查找时从最新层开始往回查，每层内部支持 region 回退。

## 性能特性

- **字典查找**: O(1) 时间复杂度（基于 HashMap）
- **零拷贝解析**: 直接引用输入缓冲区
- **最小内存分配**: 重用上下文对象
- **编译期优化**: 大部分结构在编译期展开

基准测试：
- 静态字典加载：首次约 10ms（35,000 条目）
- 单次解析：0.5-2μs（取决于字段复杂度）
- 动态层加载：YAML 约 1-2s，二进制约 50-100ms

## 文档

完整的 API 文档可通过以下命令生成：

```bash
cargo doc --workspace --no-deps --open
```

或访问在线文档：
- [spec-compiler API 文档](https://docs.rs/spec-compiler)
- [spec-engine API 文档](https://docs.rs/spec-engine)

## 测试

```bash
# 运行所有测试
cargo test --workspace

# 运行特定测试
cargo test --package spec-engine --test behavior

# 查看测试覆盖率（需要安装 tarpaulin）
cargo tarpaulin --workspace --out Html
```

## 贡献

欢迎贡献！请：

1. Fork 本项目
2. 创建特性分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'Add amazing feature'`)
4. 推送到分支 (`git push origin feature/amazing-feature`)
5. 打开 Pull Request

## 许可证

本项目采用 MIT 许可证。详见 [LICENSE](LICENSE) 文件。

## 致谢

本项目为电力通信协议解析提供了统一的解决方案，感谢所有贡献者和用户的支持！

---

**相关项目**:
- [spec-compiler](spec-compiler/): 字典编译器
- [spec-tools](spec-tools/): 命令行工具
