# DynamicCatalog 使用示例

## 概述

本目录包含 `DynamicCatalog` 的完整使用示例，演示如何在运行时动态加载和管理 YAML 字典。

## 示例程序

### dynamic_catalog_demo.rs

完整演示 DynamicCatalog 的所有核心功能：

1. **使用编译时嵌入的基础字典**
2. **运行时从 YAML 文件加载动态层**
3. **测试层的优先级和覆盖规则**
4. **测试 Region 特定的定义**
5. **层管理操作（加载、卸载、重载）**

### 运行示例

```bash
# 在 spec-engine 目录下运行
cd spec-engine
cargo run --example dynamic_catalog_demo
```

## 测试 YAML 文件

### schema/csg13/dynamic.yaml

用于测试动态加载的 YAML 文件，包含以下测试场景：

1. **覆盖测试**
   ```yaml
   - id: "00010000"
     name: "(当前)正向有功总电能【动态加载版本】"
   ```
   覆盖嵌入字典中已存在的 DI 定义

2. **新增复杂字段**
   ```yaml
   - id: "0000FF00"
     name: "(当前)组合有功电能数据块"
     fields: [...]
   ```
   添加嵌入字典中不存在的新 DI，包含嵌套字段和 candidate_ids

3. **新增简单字段**
   ```yaml
   - id: "0000FF01"
     name: "动态加载测试字段1"
     type: bin
   ```
   
4. **Region 特定定义**
   ```yaml
   - id: "0000FF02"
     region: ["云南"]
   
   - id: "0000FF03"
     region: ["深圳"]
   ```

## 如何添加更多测试

### 1. 编辑 dynamic.yaml

在 `examples/schema/csg13/dynamic.yaml` 中添加新的 DI 定义：

```yaml
data_items:
  # 添加你的测试 DI
  - id: "0000FF04"  # 使用 0x0000FFxx 范围避免冲突
    name: "你的测试字段"
    length: 4
    type: bcd
    unit: "kW"
    decimals: 2
```

### 2. 修改示例代码

在 `dynamic_catalog_demo.rs` 中添加查找代码：

```rust
// 测试新增的 DI
if let Some(field) = catalog.lookup("csg13", 0x0000FF04, "南网", None) {
    println!("  DI 0x0000FF04: {}", field.name);
}
```

### 3. 重新运行

```bash
cargo run --example dynamic_catalog_demo
```

## 支持的字段类型

可以在 dynamic.yaml 中使用所有标准字段类型：

- **基本类型**: `bcd`, `bin`, `ascii`, `hex`
- **时间类型**: `time`
- **容器类型**: `container`, `template`
- **控制结构**: `switch`, `repeat`, `bitfield`
- **特殊类型**: `di_sequence`, `dict_ref`, `external`

## Region 支持

支持的 region 值：

- `南网` (默认)
- `广东`
- `广西`
- `云南`
- `贵州`
- `海南`
- `深圳`

示例：
```yaml
- id: "0000FF05"
  name: "广东特定字段"
  region: ["广东"]
  length: 2
  type: bin
```

## 实际运行结果

```
【步骤 4】测试动态覆盖
  DI 0x00010000 (csg13, 南网):
    名称: (当前)正向有功总电能【动态加载版本】
    来源: ✓ 动态层（覆盖成功）

【步骤 5】测试动态新增的 DI
  DI 0x0000FF00 (csg13, 南网):
    名称: (当前)组合有功电能数据块
    来源: ✓ 动态层（新增）

【步骤 6】测试 Region 特定定义
  DI 0x0000FF02 (csg13, 云南):
    名称: 云南省特定电压
    来源: ✓ 动态层（云南特定）
```

## 常见问题

### Q: 运行示例时提示找不到 YAML 文件？

**A:** 确保在 `spec-engine` 目录下运行：
```bash
cd spec-engine
cargo run --example dynamic_catalog_demo
```

### Q: 如何确认我的 YAML 语法正确？

**A:** 使用 spec-tools 验证：
```bash
cargo run -p spec-tools -- validate --input examples/schema
```

### Q: 如何查看编译后的字典内容？

**A:** 使用 spec-tools 查询：
```bash
# 先编译
cargo run -p spec-tools -- compile --input examples/schema --output /tmp/test.bin

# 查询特定 DI
cargo run -p spec-tools -- query --dict /tmp/test.bin --di 0000FF00 --protocol csg13

# 查看统计信息
cargo run -p spec-tools -- stats --input /tmp/test.bin --by-protocol
```

## 相关文档

- [DynamicCatalog API 文档](../src/dynamic_loader.rs)
- [YAML 字典规范](../../schema/di.schema.json)
- [spec-tools 使用指南](../../spec-tools/README.md)
