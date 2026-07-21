# 计量自动化终端上行通信规约 —— DI 字典 YAML Schema 设计

用于描述数据标识(DI)定义的字典源文件格式，覆盖字段的编码方式、条件依赖、重复结构、位域、模板复用、外部协议嵌套等全部已知模式。目标：**新增 DI 只编辑数据，不改 Rust 代码**；由 `build.rs` 在编译期把 YAML 展开、生成静态查找表，运行时只面对一棵已展开好的字段树。

---

## 顶层结构

```yaml
templates:      # 可复用的字段组模板，供 template 类型字段引用
  <template_id>: ...

data_items:      # 真正的 DI 字典条目
  - id: <DI码>
    ...
```

---

## 字段类型一览表

| 类型 | 用途 | 对应原文场景 |
| --- | --- | --- |
| `fixed`（默认） | 定长、无分支的普通字段 | 绝大多数字段（BCD/BIN/ASCII/时间戳） |
| `bitfield` | 按 bit 切分的状态字 | 附录C.5 运行状态字 |
| `bitmask` | 按位掩码驱动的重复结构：依次检查每一位是否置位，再按位决定是否解析后续内容 | 失败表/告警项目位图 |
| `switch` | 类型/含义依赖同容器内另一字段的值 | E0000100 通信地址（依赖通道类型） |
| `repeat` | 前导计数字段决定后面重复次数 | 费率数据块、APP列表 |
| `template` | 引用可复用模板 | APP信息（30字节模板） |
| `external` | 内容是另一套协议的完整报文，需调用外部解析器 | 中继转发内嵌 DL/T 645 报文等 |
| `di_sequence` | 按顺序拼接其他已定义 DI 的字段规则（引用目标编译期已知） | E000010F"以上数据项集合"、ARD2"发生时数据" |
| `dict_ref` | 用某字段的**运行时值**去全局DI字典查格式（引用目标编译期未知） | 主动上报报文中"数据标识+对应采集值"列表 |
| `custom` | 无法数据化的少数字段，挂函数名逃生舱 | 语义无法从表格推断的极少数字段 |
| `skip` | 直接跳过若干字节/不生成节点，用于保留位、空分支或无意义字段 | 填充字节、空分支 |
| `info_point` | 信息点标识DA：固定2字节，测量点组号+位掩码，运行时动态算出命中的测量点号 | 6.1.3 信息点标识DA |
| `di_code` | 数据标识编码DI：固定4字节，小端读出后查同协议DI字典拿名称，不解析数据内容 | 6.1.4 数据标识编码DI |

`fixed` 内部按编码方式还细分：`bcd` / `bin` / `ascii` / `time` / `hex`（1.0节，原样十六进制字符串）；`bcd`/`bin` 还可附加 `endian`（1.1节，字节序）与 `signed`（1.2节，原码符号位，位置自动推导）

---

## 1. `fixed` —— 定长字段

```yaml
- name: 心跳周期
  length: 1
  type: bcd          # bcd | bin | ascii | hex
  unit: 分            # 可选
  decimal: 0          # 可选，BCD小数位数，如 NNNNNN.NN → decimal: 2
  enum:               # 可选，键值枚举
    "00": TCP
    "01": UDP

# 时间字段（BCD或BIN编码 + time 指定格式）
- name: 上报基准时间
  length: 5
  type: bcd
  time: "mmhhDDMMYY"   # 指定BCD编码的时间格式

- name: 上报时间
  length: 6
  type: bin           # 传输BIN编码的时间
  endian: little
  time: "ssmmhhDDMMYY"
```

- `time` 配合 `type: bcd` 或 `type: bin`，用于指定时间的格式（如 `ssmmhhDDMMYY`）。这样时间字段既需要控制编码方式（BCD vs BIN），也需要控制格式。
- `enum` 的 key 统一按字符串写十六进制（`"00"`），值统一是解析后的原始字节数值对应的字符串

### `format` 对象 —— 展示层格式化配置

- 字段的 `type` 定义了解析语义；`format` 定义了报文解析后的展示/输出格式。
- `format` 是一个子对象，常见字段包括：
  - `type`：输出格式类型，可选 `hex`/`bcd`/`bin`。
  - `group_bytes`：每组字节数。
  - `separator`：组间分隔符。
  - `pad`：是否填充宽度。
  - `endian`：组内字节序，仅在 `group_bytes > 1` 时有效。
  - `order`：组序，`normal`/`reverse`，用于控制整个组序列的正序或逆序输出。

例如：

```yaml
- name: IP地址
  length: 4
  type: bin
  format:
    order: normal
    endian: little
    group_bytes: 1
    separator: "."
    pad: false
```

这段配置表示：按 BIN 解析这 4 字节，展示时把每个字节按小端组内字节序处理（单字节时无影响），再按正常组序输出成 `10.47.18.228`。

如果要把整个组序列反转，可以写：

```yaml
- name: 逆序字节列表
  length: 4
  type: bin
  format:
    order: reverse
    group_bytes: 1
    separator: "."
```

这会把字节顺序整体反转后输出，比如 `0A 2F 12 E4` → `228.18.47.10` （按每字节反序展示）。

### 1.0 `hex` —— 原样十六进制字符串（不做数值/字符转换）

```yaml
- name: 密码等级标识
  length: 2
  type: hex           # 原始字节直接拼成十六进制字符串，如 FF 01 → "FF01"
  enum:
    "FFFF": 未设置
    "0001": 一级密码
```

- **与 `bin` 的区别**：`bin` 的语义是"这几个字节代表一个数值"，因此需要 `endian`；`hex` 的语义是"这几个字节本身就是一个标识符/代码"，不涉及数值大小比较，**不需要 `endian`**——字节按原始传输顺序逐个转成两位大写十六进制字符直接拼接，没有"倒序读"的问题
- **与 `ascii` 的区别**：`ascii` 把字节当 ASCII 字符解码成可读文本（`41 42` → `"AB"`）；`hex` 把字节当十六进制数字符号本身输出（`41 42` → `"4142"`）——两者都输出字符串，但语义不同，不要混淆
- 适用场景：字段本质是代码/标志位组合而非可比较大小的数值（如 `FFFF` 代表"未设置"这种哨兵值）、MAC地址/序列号/密钥指纹这类只关心匹配不关心大小的标识符，以及原文档没写清楚编码方式、只给了十六进制示例的字段——用 `hex` 比强行猜 `bin`/`bcd` 更安全

### 1.1 字节序（`endian`）——仅对 `type: bin` 且 `length > 1` 有意义

**协议本身字节序不统一，必须逐字段标注，不能假设全局默认：**

- **帧/链路层字段是小端**（低字节在前）：原文明确"链路层传输顺序为低位在前，高位在后"，长度域 L（图5-2）也是先低字节后高字节。这类字段（报文长度、SEQ序号等）用 `endian: little`
- **应用层部分字段是大端**（网络字节序）：比如 `0A 2F 12 E4 23 29` 表示 IP `10.47.18.228` 端口 `9001`——`23 29` 按大端读 `0x2329=9001` 才对得上，说明端口号这类字段是 `endian: big`

```yaml
- name: 报文长度
  length: 2
  type: bin
  endian: little        # 帧级字段，遵循链路层约定

- name: 端口号
  length: 2
  type: bin
  endian: big            # 网络字节序（IP+端口示例已验证）
```

**约定：**
- `bcd`/`ascii` 本身是十进制/字符序列，不存在字节序问题，不需要 `endian`
- 当时间字段配合 `time` 时，`endian` 仅对 `type: bin` 有效
- `format.order` 用于控制展示时整体组序正序/逆序，而 `endian` 仍然只影响组内字节顺序
- 不写 `endian` 时默认 `little`（跟随链路层约定），但**大端字段必须显式写 `endian: big`，不能省略**——省略等同于"没验证过"
- 复核字典时建议专门拉一份"所有 `bin` 且 `length>1`"的清单，逐条对照原文有无十六进制示例验证字节序；没有示例的先标 `endian: unknown` + `confidence: low`，走人工复核

### 1.2 符号位（`signed`）—— 原码，不是补码；位置由 `endian`/固定约定推导，不需手填

某些 `bcd`/`bin` 字段用最高位单独表示正负（如线损率、有功功率方向、校时误差、浮动系数等），去掉符号位后剩下的位是**数值的绝对值**（原码/sign-magnitude），**不是**补码——不能直接强转成 Rust 有符号整数，否则数值完全不对（比如 `0x8005` 在补码下是 `-32763`，但在这种约定下应解读成"符号位=1，数值=5，结果-5"）。原文档里 BIN 字段也明确用过这个约定（如"校时误差TTTT，BIN编码，最高位代表符号位"、"浮动系数最高位表示符号位，其余7位BIN编码"），不是只有 BCD 才有。

**符号位的物理位置不需要单独手填，而是由已有配置推导：**
- **`bcd` 字段**：原文格式串里 `S` 固定写在最前面（如 `0SNN.NN`、`SNNNNNNN.N`），BCD 按十进制数位从左到右编码不受字节序影响，符号固定在**首字节**
- **`bin` 字段**：符号位是"数值的最高有效位"，这个位落在传输顺序里的哪个字节，完全由已经配置好的 `endian` 决定——小端时数值最高有效字节在传输顺序的最后一个字节，符号位就在那里；大端时在第一个传输字节。**不需要额外的 `sign_position` 配置项**，避免和 `endian` 打架出现矛盾配置

```yaml
- name: 本日线损率
  length: 3
  type: bcd
  decimal: 2
  signed: true          # 对应原文"0SNN.NN"，S固定在首字节高位，无需额外配置

- name: 校时误差TTTT
  length: 2
  type: bin
  endian: little         # 决定了数值最高有效字节在传输顺序里的位置
  signed: true           # 符号位就在那个最高有效字节的最高bit

- name: 功率定值浮动系数
  length: 1
  type: bin
  signed: true           # 单字节，endian不影响，符号位即该字节最高位
```

- 解析规则：定位到符号所在字节后取出最高位判断正负，**清掉这一位**后剩下的位按正常的 `bcd`/`bin` 规则解出绝对值，最后按符号取负——复用已有解码逻辑，不是另起一套

---

## 2. `bitfield` —— 位域分段

```yaml
- name: 运行状态字1
  length: 2
  type: bitfield
  bits:
    - range: [0, 0]         # [start, end]，单bit时相等
      name: 保留
    - range: [1, 1]
      name: 需量积算方式
      enum: { "1": 区间, "0": 滑差 }
    - range: [1, 2]         # 跨多位，如"供电方式"占2bit
      name: 供电方式
      enum: { "00": 主电源, "01": 辅助电源, "10": 电池供电 }
    - range: [6, 15]
      name: 保留
```

**注意事项：**
- bit 编号约定（bit0 是 LSB 还是 MSB）需对照真实报文核实一次，写进 schema 顶层注释固定死，不能靠猜
- `enum` 的 key 是该位段截出来的**数值**，不是原始字节

## 2.5 `bitmask` —— 按位掩码驱动的可变解析

```yaml
- name: 失败表
  length: 256
  type: bitmask
  bit_order: msb
  name_template: "失败表{index}"
  element:
    type: switch
    on: $bit_value
    cases:
      "0": { length: 0, type: skip }
      "1": { length: 0, type: bin, name: "失败表{index}" }
```

- `bitmask` 适用于“先给出一个位图/掩码字节序列，再根据每一位是否置位决定后续是否还有跟随内容”的场景。
- 解析时会按 `length` 指定的总字节数，把每一位依次遍历；对每个 bit，运行时会把当前 bit 的值绑定为 `$bit_value`，然后用 `element` 里的 `switch`/`repeat`/`fixed` 等子结构继续解析。
- 这类字段常用于“告警项目位图”“失败表”“状态掩码”这类协议结构：位为 0 时跳过，不占用字节；位为 1 时继续读后续字段。
- `bit_order` 控制从 LSB 还是 MSB 开始遍历，默认通常按 `asc`/`lsb` 语义即可；若协议文档明确是从高位到低位，写 `bit_order: msb`。

## 2.6 `skip` —— 直接跳过、不生成节点

```yaml
- name: 预留字节
  length: 1
  type: skip

- name: 按位掩码空分支
  type: switch
  on: $bit_value
  cases:
    "0": { length: 0, type: skip }
    "1": { length: 1, type: bcd }
```

- `type: skip` 的作用是“消耗指定长度的字节，但不生成任何解析节点”，常用于协议里的保留字节、填充字节、无意义字段或某些位为 0 时的空分支。
- 若 `length` 省略，默认按 0 处理；在 `switch`/`bitmask` 的分支里，`skip` 常与 `length: 0` 配合使用，表示“当前位不带任何后续数据”。

---

## 3. `switch` —— 条件类型分支

```yaml
- name: 通信地址
  length: 8
  type: switch
  on: 通信通道类型        # 引用同容器内已解析的字段名
  cases:
    "02": ip_with_port    # GPRS/CDMA
    "04": ip_with_port    # Ethernet
  default: phone_bcd
```

- `on` 引用的字段必须先于本字段被解析（即在 YAML 里排在前面）。
- 如果被引用字段有 `id`，`switch.on` 应优先使用该 `id`，因为 `id` 在语义上更稳定、避免同名字段歧义；如果没有 `id`，则使用字段 `name`。
- 引用其他字段时不要使用 `$` 前缀，除非你用的是 `switch.on` 的特殊长度匹配值：`$remaining`、`$len`、`$length`。
- 当 `on` 值为 `$remaining` / `$len` / `$length` 时，`switch` 会以当前字段剩余字节长度为条件分支，适用于可变长度尾部解析。
- `cases` 的值可以是编码类型名（可以是内置类型，也可以通过 `type: template` + `template_ref` 指向另一个模板），也可以直接写成一个内联字段定义。

```yaml
- name: 终端心跳
  protocol: csg13
  region: ["南网"]
  type: switch
  on: "$remaining"
  cases:
    "0":
      type: container
      fields: []
    "5":
      type: bcd
      length: 5
      time: "mmhhDDMMYY"
    "6":
      type: bcd
      length: 6
      time: "ssmmhhDDMMYY"
  default:
    type: custom
    handler: "parse_unknown_heartbeat"
```

---

## 4. `repeat` —— 计数重复

```yaml
- name: APP数量
  id: app_count          # 命名绑定，供后面 count_ref 引用
  length: 1
  type: bin

- name: APP信息
  type: repeat
  count_ref: app_count    # 引用前面字段的解析结果作为重复次数
  element:
    type: template
    template_ref: APP
```

- 总长度 = count × 单个元素长度，**不需要额外写 `30 × APP数量` 这类表达式**，长度由 `repeat` 语义自动推导
- `element` 既可以是 `type: template` 引用，也可以直接内联一个 `fixed` 定义
- `repeat` 仅用于运行时计数的重复结构，必须通过 `count_ref` 引用前面的计数字段。引用时不要使用 `$` 前缀，直接使用字段 `id` 或 `name`。若被引用字段有 `id`，`count_ref` 应直接使用该 `id`。
- 如果重复元素的数量和 DI 号规律在编译期已知，可在字段列表中使用结构化的 `candidate_ids` 条目，
  由 `build.rs` 生成对应的候选 DI 号并注册到字典。
  `candidate_ids` 必须作为父字段的 `fields` 子项出现，且只能使用结构化字段语义：
  包含 `count`、可选 `count_ref`、`id_expr`、可选 `name_template` 和 `element`。
  例如：

```yaml
- id: "0001FF00"
  name: "(当前)正向有功电能数据块"
  protocol: "csg13"
  region: ["南网"]
  fields:
    - name: "费率数"
      id: rate_count
      length: 1
      type: bcd
    - id: "00010000"
      name: "(当前)正向有功总电能"
      length: 4
      type: bcd
      unit: "kWh"
      decimal: 2
    - repeat:
        count_ref: rate_count
        name_template: "(当前)正向有功费率{index}电能"
        element:
          length: 4
          type: bcd
          unit: "kWh"
          decimal: 2
    - candidate_ids:
        count: 63
        count_ref: rate_count
        id_expr: "0x00010100 + index0*0x0100"
        name_template: "(当前)正向有功费率{index}电能"
        element:
          length: 4
          type: bcd
          unit: "kWh"
          decimal: 2
```

- 这种写法用于在编译期注册一组候选 DI 号，同时运行时仍按 `repeat` + `count_ref` 解析实际数量。
---

## 5. `template` —— 可复用模板

```yaml
templates:
  APP:
    length: 30
    fields:
      - name: APP名称
        length: 8
        type: ascii
      - name: APP版本号
        length: 8
        type: ascii
      - name: APP厂家代码
        length: 2
        type: ascii
      - name: APP CPU占用率
        length: 3
        decimal: 2
        type: bcd
        unit: "%"
      - name: APP CPU运行状态
        length: 1
        type: bcd
        enum: { "01": 运行, "02": 停止, "03": 异常 }
      - name: APP虚拟内存大小
        length: 4
        type: bcd
        unit: kb
      - name: APP内存占用率
        length: 3
        type: bcd
        unit: "%"
      - name: APP复位次数
        length: 1
        type: bcd
        unit: 次
```

- 模板在 `build.rs` 阶段**编译期展开内联**，运行时不知道"模板"这个概念，等价于手写了一遍字段树，性能与普通 `fixed` 序列无差异
- 复用方式：`{ type: template, template_ref: APP }`

---

## 6. `external` —— 外部协议报文

```yaml
- name: 内嵌报文
  type: external
  protocol: dlt645-2007     # 只声明"归哪个协议管"
  length: remaining          # 或具体长度 / 长度引用
```

- Rust 侧维护协议解析器注册表，按 `protocol` 名分发：
  ```rust
  type ExternalParser = fn(&[u8]) -> Result<Value>;
  static EXTERNAL_REGISTRY: Lazy<HashMap<&str, ExternalParser>> = ...;
  ```
- 复用单位是"整个协议"，不是单个字段，与 `custom` 的区别在此

---

## 7. `di_sequence` —— 按DI列表顺序拼接

```yaml
templates:
  ARD2:
    fields:
      - name: 告警状态
        length: 1
        type: bcd
        enum: { "00": 恢复, "01": 发生 }
      - name: 告警发生时间
        length: 6
        type: bcd
        time: ssmmhhDDMMYY
      - name: 发生时数据
        type: di_sequence
        items: [00010000, 00020000, 00030000, 00040000,
                0201FF00, 0202FF00, 0203FF00, 0204FF00, 0206FF00]
```

- `items` 里的每个 DI 必须已在字典别处定义
- `build.rs` 需分两遍扫描：第一遍建好"DI→字段树"完整映射表，第二遍再展开 `di_sequence`（不能假设 YAML 文件里定义顺序天然满足依赖）
- 展开后等价于一个 `Container`，运行时无需感知"跨DI引用"这个概念

---

## 7.5. `dict_ref` —— 运行时按值查全局字典（区别于 di_sequence）

**与 `di_sequence` 的本质区别**：`di_sequence` 引用的是一组**编译期就已知**的固定 DI 列表，可以在 `build.rs` 阶段直接展开内联；`dict_ref` 引用的是一个**只有解析真实报文时才知道值**的字段（比如报文里携带的"数据标识"字节本身），必须在**运行时**才能查到对应格式，不能提前展开。

典型场景——主动上报报文中"数据标识+对应采集值"反复出现：

```
| 数据标识1 | BIN | 4    |                    |
| 数据1-1   | BIN | 变长  | 数据标识1的第1个采集数据 |
| ......   | ... | ...  | ......              |
| 数据1-n   | BIN | 变长  | 数据标识1的第n个采集数据 |
```

```yaml
- name: 采集点数量n
  id: point_count
  length: 1
  type: bin

- name: 数据项数量m
  id: item_count
  length: 1
  type: bin

- name: 数据项列表
  type: repeat
  count_ref: item_count
  element:
    type: container
    fields:
      - name: 数据标识
        id: current_di        # 绑定给内层 dict_ref 引用
        length: 4
        type: bin
      - name: 采集数据列表
        type: repeat
        count_ref: point_count
        element:
          type: dict_ref
          ref: current_di      # 用 current_di 的运行时值去全局DI表查格式
```

**注意事项：**
- `dict_ref` 引用目标的字节长度可能因DI不同而不同（比如电能量4字节BCD、开关量1字节），内层 `repeat` **不能按"count × 定长"预先推导总长度**，必须逐次调用 `dict_ref` 解析、按实际吃掉的字节数累加偏移——`parse_field` 骨架本来就是按实际解析长度累加offset，天然支持，不需要额外改动，只是这里不能做定长优化假设
- **未知DI必须优雅降级**：报文里出现的DI如果字典没收录（协议升版、设备先升级），`DI_TABLE.get()` 失败要返回 `Err`，不能 panic，且应支持"跳过这一条继续解析后续内容"而不是让整帧报文全部失败
- 如果查到的DI本身又是 `repeat`/`switch` 等复合结构，无需特殊处理——`dict_ref` 只是递归调用同一个 `parse_field`，复用已有逻辑

---

## 7.6 bits_ref —— 位驱动的 repeat 与运行时变量

为了解析像 `04001501` 这种“状态字 + 针对已置位的位继续携带各自采集数据”的模式，新增一套 data-driven 写法，避免为每种设备写 custom handler：

- 在 `bitfield` 中可为字段指定 `ref_id`，用于被其它字段引用：

```yaml
- name: 主动上报状态字
  length: 12
  type: bitfield
  ref_id: report_status
  bits:
    - range: [0,0]
      name: 负荷开关误动或拒动
      enum: { "1": 已发生, "0": 未发生 }
    - range: [1,1]
      name: ESAM错误
    # ...
```

- `repeat` 支持通过 `bits_ref` 指向某个先前解析过的 bitfield 的 `ref_id`，按每个 bit 的定义顺序/指定顺序驱动多次解析：

```yaml
- type: repeat
  bits_ref: report_status      # 对应上面 ref_id
  bit_order: asc               # 可选，'asc' 或 'desc'（默认 asc）
  name_template: "{bit_name}新增次数"
  element:
    type: switch
    on: $bit_value             # 运行时合成变量
    cases:
      "0": { length: 0, type: fixed }
      "1": { length: 1, type: bcd, unit: 次 }
```

解释与语义：

- `bits_ref`：运行时取名为 `report_status` 的 bitfield 的原始 bytes，并按该 bitfield 的 `bits` 描述逐位（或位段）迭代。此字段在 `build.rs` 阶段需验证 `bits_ref` 指向已存在的 `ref_id`，并把被引用的 `BitSpec` 列表嵌入到 repeat 的 `bit_specs` 字段，以便运行时直接使用（无需再次查 schema）。

- `bit_order`：控制按 `bits` 定义的升序（`asc`）或降序（`desc`）迭代，默认为 `asc`。

- `name_template`：和其它 repeat 一致，支持占位符 `{index}`、`{index0}`、`{id}`，并新增 `{bit_name}` 与 `{bit_ref}`（如果 bit 定义带有 `ref_id`）。例如 `"{bit_name}新增次数"` 会在每个已置位的 bit 上生成相应名称。

- 运行时绑定的合成变量（可被 `switch.on` 或其它模板引用）：
  - `bit_value`：该 bit/位段的数值（整数）；在 `switch.on` 中可以写成 `$bit_value` 来匹配 case。运行时也把其原始 bytes 绑定为 raw，可通过 `ctx.get_raw` 访问。
  - `bit_index`：位段的起始 bit 索引（整数）。
  - `bit_name`：该 bit 在 schema 中的 `name`（字符串）。
  - `bit_ref`：若该 bit 声明了 `ref_id`，该字段的字符串值；可用于进一步的 `dict_ref` 或模板替换。

- `$` 前缀语义扩展：
  - 在 `switch.on` 中，写 `$var_name` 表示使用由运行时合成（或前面字段绑定）的变量 `var_name` 的已解码值（非原始 bytes）；这使得 `switch` 能根据每次迭代的 `bit_value` 分支解析行为（常用于 0/1 位的开/关或可选跟随数据）。
  - 原有的 `$remaining` / `$len` / `$length` 保持不变，用于基于剩余字节长度的分支。

- 跳过无输出的分支（重要）：“如果某个 case 的解析长度为 0（例如上例中 `"0": { length: 0, type: fixed }`），运行时解析器不会为该次迭代产生节点或占位值”。实现细节：解析器在每次位驱动的迭代中会：
  1. `push` 新的解析作用域并绑定 `bit_*` 变量；
  2. 调用 `parse_field` 解析 `element`，得到 `(v, consumed)`；
  3. 若 `consumed == 0`，则 `pop` 作用域并继续下一位（不会把 `v` 封装为 `Node` 并加入结果列表）；
  4. 否则按常规把解析值封装成 `Node` 并加入 repeat 结果列表，偏移量累加 `consumed`。

  这种策略保证在位为 0 时既不消耗字节也不会在最终 JSON/树形输出中留下空节点。

- `bit_ref` 的高级用法：当某个 bit 的 `BitSpec` 指定了 `ref_id`，可在对应的 repeat 元素内使用 `dict_ref` 指向该 `bit_ref`，实现“位被置位时，按该位关联的另一个 DI 的格式继续解析”的模式；`build.rs` 应保证这些 `ref_id` 在编译期已注册为可被引用的格式（或在运行时提供降级处理）。

示例（汇总）

```yaml
- id: "04001501"
  name: 主动上报状态字
  protocol: csg13
  region: ["南网"]
  fields:
    - name: 主动上报状态字
      length: 12
      type: bitfield
      ref_id: report_status
      bits: ...
    - type: repeat
      bits_ref: report_status
      bit_order: asc
      name_template: "{bit_name}新增次数"
      element:
        type: switch
        on: $bit_value
        cases:
          "0": { length: 0, type: fixed }
          "1": { length: 1, type: bcd, unit: 次 }
```

注意：`build.rs` 在处理 `repeat.bits_ref` 时需要把被引用的 bit 描述一并解析并序列化进运行时表（`bit_specs`），否则运行时无法得知迭代顺序与 bit 的 `name`/`range` 信息。

---

## 8. 容器内子字段可选"双重身份"（既是子字段，也是独立可寻址DI）

```yaml
- id: 040005FF
  name: 运行状态字数据块
  length: 14
  fields:
    - id: 04000501          # 带id：同时注册进全局DI表，可单独被寻址读取
      name: 运行状态字1
      length: 2
      type: bitfield
      bits: [...]
    - name: 密钥状态          # 不带id：纯容器内部字段，不可单独寻址
      length: 4
      type: fixed
      encoding: raw
```

- `build.rs` 遍历字段树时，凡带 `id` 的节点额外注册进顶层 `DI_TABLE`
- 寻址（从这个字节开始按这棵子树解析）与"作为父容器一部分被解析"是两件独立的事，互不冲突

---

## 9. `custom` —— 逃生舱（少数无法数据化的字段）

```yaml
- name: 某复杂字段
  type: custom
  handler: parse_xxx_field    # 对应 Rust 里手写的解析函数名
```

- 仅用于：字段名无法从表格推断、回指消解无法可靠自动化、真正的复合业务逻辑
- 使用频率应控制在个位数百分比（参考此前统计：A.2 家族 12 个字段仅 2-3 个不规则）

---

## 10. `info_point` —— 信息点标识 DA（6.1.3，测量点选择位图）

```yaml
- name: 信息点标识组数
  length: 1
  type: bin
  ref_id: pn_count
- name: 信息点标识
  type: repeat
  count_ref: pn_count
  element:
    name: "第{index}组测量点"
    type: info_point       # 不需要任何额外配置，固定读2字节 DA1+DA2
```

- 语义（原文 6.1.3）：DA2(第2字节)是测量点组号(1~254)，DA1(第1字节)是该组内8个测量点的位掩码，
  D0..D7 依次对应 p((DA2-1)\*8+1) .. p((DA2-1)\*8+8)；两个哨兵值：DA1=DA2=00H → 终端测量点p0，
  DA1=DA2=FFH → 除终端测量点外的所有测量点
- 同一个 `info_point` 字段可能同时命中多个测量点（如 DA2=01H,DA1=03H → p1、p2），因此解析结果
  始终是 `Value::List`（哨兵值是 `Value::Str`），**不要**假设一个 `info_point` 只对应一个测量点号
- 之所以是内置类型而非走 `custom` 逃生舱：这是协议里明确定义、可复用的通用结构（不止 BASETASK
  用到），值得跟 `bitfield`/`repeat` 一样做成数据驱动、不用为每处引用都手写一个 handler

---

## 11. `di_code` —— 数据标识编码 DI（6.1.4，只标识不解析内容）

```yaml
- name: 数据标识编码组数
  length: 1
  type: bin
  ref_id: di_count
- name: 数据标识编码
  type: repeat
  count_ref: di_count
  element:
    name: "第{index}个数据标识"
    type: di_code       # 不需要任何额外配置，固定读4字节，按小端解出DI码后查字典
```

- 语义（原文 6.1.4）：DI 由 DI3/DI2/DI1/DI0 四个字节构成；报文里的传输顺序是 DI0,DI1,DI2,DI3
  （小端，跟链路层字段的字节序约定一致），按小端读出的32位数值就是字典里 `id:` 对应的 DI 码
- 解析结果是 `Value::Str`，格式固定为 `"{DI码8位大写十六进制}_{字典里的name}"`（跟 `parse_di`
  顶层结果的命名规则完全一致），查不到时不中断整体解析，退化为 `"{DI码}_未知数据标识"`——报文里
  出现字典还没收录的 DI 在实际场景中是正常情况，不应该让一个陌生 DI 拖垮整条任务定义的解析
- **只解析标识本身，不递归解析该 DI 的数据内容**：原文区分了两种帧——"只有数据标识无数据标识
  内容"（如 BASETASK 里枚举任务包含哪些DI，`di_code` 用在这里）和"数据标识+紧跟其数据标识内容"
  （如主动上报报文，每个 DA/DI 后面紧跟对应的 DATA）。后一种场景每个 DA/DI 在报文里是独立展开的
  一条记录，不是"标识+可复用结构"的关系，应该用 `dict_ref`（按运行时值查字典**并解析目标DI的
  完整字段结构**）或者显式拼 DA+DI+data 三个字段表达，不要把 `di_code` 套在这种场景上

---

## 对应的 Rust 端 FieldSpec（运行时，10种节点；`repeat`/`template`/`di_sequence` 在 build.rs 阶段已内联展开，`dict_ref` 因引用目标编译期未知，必须保留为运行时节点）

```rust
enum Endian { Little, Big }

enum Encoding {
    Bin  { endian: Endian, signed: bool },        // 符号位位置由 endian 推导，不需单独字段
    Bcd  { digits: u8, decimals: u8, signed: bool }, // 符号位固定首字节
    Ascii,
    Hex,              // 原样十六进制字符串，不涉及数值/字节序
    Time { format: &'static str },
    Raw,
}

enum FieldSpec {
    Fixed { encoding: Encoding, length: usize },
    BitField { length: usize, bits: Vec<BitSpec> },
    Switch { on: String, cases: HashMap<u8, Box<FieldSpec>>, default: Box<FieldSpec> },
    Repeat { count_ref: String, element: Box<FieldSpec> }, // element已展开好的树
    External { protocol: &'static str },
    Container(Vec<NamedField>),   // di_sequence 展开后落到这里
    Custom(&'static str),         // 函数名索引
    DictRef { di_ref: String },   // 运行时用 di_ref 对应字段的值查全局 DI_TABLE
    InfoPoint,                    // 信息点标识DA，固定2字节，无配置项
    DiCode,                       // 数据标识编码DI，固定4字节，无配置项
}

struct NamedField {
    id: Option<u32>,     // Some时注册进全局 DI_TABLE，可单独寻址
    name: &'static str,
    spec: FieldSpec,
}

struct BitSpec {
    range: (u8, u8),
    name: &'static str,
    enum_map: Option<HashMap<u8, &'static str>>,
}

// bin 类型按 endian 解码；实际项目建议用 byteorder crate 的
// read_u16::<LittleEndian>() / BigEndian 代替手写实现
fn decode_bin(raw: &[u8], endian: &Endian) -> u64 {
    match endian {
        Endian::Little => raw.iter().rev().fold(0u64, |acc, &b| (acc << 8) | b as u64),
        Endian::Big    => raw.iter().fold(0u64, |acc, &b| (acc << 8) | b as u64),
    }
}

// hex：原样转十六进制字符串，无数值/字节序概念
fn decode_hex(raw: &[u8]) -> String {
    raw.iter().map(|b| format!("{:02X}", b)).collect()
}

// signed：原码（sign-magnitude），不是补码。符号位位置从 endian（bin）或固定首字节（bcd）推导，
// 清掉符号位后复用已有的 bcd/bin 解码，最后按符号位取负 —— 不新增独立解析路径，只是包一层。
fn decode_signed_bin(raw: &[u8], endian: &Endian, magnitude: impl Fn(&[u8]) -> f64) -> f64 {
    let sign_byte_idx = match endian {
        Endian::Big => 0,                  // 大端：第一个传输字节是最高有效字节
        Endian::Little => raw.len() - 1,   // 小端：最后一个传输字节是最高有效字节
    };
    let is_negative = (raw[sign_byte_idx] & 0x80) != 0;
    let mut cleared = raw.to_vec();
    cleared[sign_byte_idx] &= 0x7F;
    let value = magnitude(&cleared);
    if is_negative { -value } else { value }
}

fn decode_signed_bcd(raw: &[u8], magnitude: impl Fn(&[u8]) -> f64) -> f64 {
    let is_negative = (raw[0] & 0x80) != 0;   // BCD符号固定在首字节
    let mut cleared = raw.to_vec();
    cleared[0] &= 0x7F;
    let value = magnitude(&cleared);
    if is_negative { -value } else { value }
}

// DictRef 分支只是复用同一个 parse_field 递归调用一次，不引入新解析逻辑：
fn parse_field(buf: &[u8], spec: &FieldSpec, ctx: &mut Context) -> Result<(Value, usize)> {
    match spec {
        // ...其余分支...
        FieldSpec::DictRef { di_ref } => {
            let di = ctx.values.get(di_ref).and_then(|v| v.as_u32())
                .ok_or(Error::MissingRef(di_ref.clone()))?;
            let target_spec = DI_TABLE.get(&di).ok_or(Error::UnknownDi(di))?;
            parse_field(buf, target_spec, ctx)
        }
    }
}
```

---

## 维护流程速览

1. **新增常规 DI**（定长、无分支）→ 编辑 `data_items` 加一条，跑 `build.rs`，不碰 Rust
2. **新增有条件/重复/位域的 DI** → 用对应原语（`switch`/`repeat`/`bitfield`）描述，仍是纯数据编辑
3. **新增可复用结构（如"传感器列表"）** → 加一个 `templates` 条目 + 一处 `type: template` 字段引用
4. **接入另一套协议的报文** → 加一个 `external` 声明 + 在 Rust 注册表里注册一次解析函数
5. **报文里携带"数据标识+对应值"这种运行时才能确定格式的字段** → 用 `dict_ref` 引用已有的全局DI字典，不需要新写解析逻辑，且天然支持字典以后扩充新DI
6. **新增/复核多字节 `bin` 字段** → 必须逐条核对原文有无十六进制示例验证字节序，没把握先标 `endian: unknown` + `confidence: low`
7. **字段有方向/正负含义（功率方向、线损率、校时误差等）** → 核实是否用最高位表示符号（原码而非补码），标注 `signed: true`；位置由 `endian`（bin）或固定首字节（bcd）自动推导，不需要额外手填位置
8. **字段的位/字节含义要靠"运行时才知道的另一部分数据"动态计算，但结构本身是协议里明确定义、会被多处复用的通用规则**（如 `info_point` 信息点标识DA：位的含义要等读到组号字节才能算，不是编译期能展开的静态位域；`di_code` 数据标识编码：字节需要按约定顺序重新排列才能得到可查字典的码）→ 不要因为"通用机制表达不了"就直接摊手扔进 `custom`。这种情况应该做成跟 `bitfield`/`repeat` 同级的**内置类型**：`types.rs` 加一个无字段的 `FieldSpec` 单元变体、`build.rs` 加一行类型分发、`parser.rs` 写解析函数——**同时**在两处 `match FieldSpec` 补上新分支（`parse_field` 分发 + repeat 展开时的克隆函数 `instantiate_field_spec`，编译器的 exhaustive match 检查会在漏改时直接报错，不会静默漏掉）。真正判断"值不值得做成内置类型"的标准是复用次数：只在协议里出现一次、语义还无法从表格推断的字段留给 `custom` 逃生舱；协议明确定义、以后还会被别的DI/模板引用的结构（DA、DI 都符合）就该数据驱动
9. **真正遇到第13种结构模式**（现有13类：`fixed`及其`bcd`/`bin`/`ascii`/`hex`/`time`子形态算一类、`bitfield`、`bitmask`、`switch`、`repeat`、`template`、`external`、`di_sequence`、`dict_ref`、`custom`、`skip`、`info_point`、`di_code`，都无法表达）→ 才需要扩展 schema 本身，这种情况应该很少发生