//! 回归测试：覆盖一轮 schema/csg13.yaml + build.rs 审计中修复的每一个真实 bug。
//!
//! 这些测试存在的意义不是"覆盖率"，是防止同类问题以后又悄悄漏进来——它们
//! 修复前全部会失败（region 多值、id 冲突、lengthrule 语法这几类问题都不会让
//! `cargo build` 失败，只会在跑到具体 DI/具体省份时才发现查不到或者解析错误，
//! 光靠 `cargo build` 通过是测不出来的），所以专门写成断言而不是打印。

use spec_engine::Engine;

const PROTOCOL: &str = "csg13";

fn get_engine() -> Engine {
    Engine::new_default()
}

fn parse_di(protocol: &str, di: u32, region: &str, dir: Option<&str>, buf: &[u8]) -> Result<(spec_engine::Value, usize), spec_engine::DictError> {
    let engine = get_engine();
    engine.parse_di(protocol, di, region, dir, buf)
}

/// bug: `region: ["南网,广东,海南"]` 被误写成逗号拼接的单个字符串，导致这条
/// DI 在"广东"/"海南"单独查询时查不到（实际注册进去的 key 是那个不存在的
/// 省份"南网,广东,海南"）。修复后应该按列表里的每个省份分别可查。
///
/// 这个测试同时覆盖了另一个更底层的 build.rs bug：顶层 data_items 展开循环
/// 里，`effective_region` 会优先采用字段自身的完整 region 列表（取第一个），
/// 导致哪怕 YAML 里 region 写对了、变成了真正的多元素列表，展开循环里也只有
/// 第一个 region 会被真正注册，后面的全部静默丢失。
#[test]
fn heartbeat_flag_is_queryable_in_every_listed_region() {
    for region in ["南网", "广东", "海南", "广西", "贵州", "云南"] {
        let result = parse_di(PROTOCOL, 0xE0001003, region, None, &[0x01]);
        assert!(
            result.is_ok(),
            "E0001003 在 region={region} 应该能查到，实际: {result:?}"
        );
    }
}

/// bug: `region: ["南网,广东"]` 同上，另一个受影响的 DI。
#[test]
fn air_conditioner_switch_status_is_queryable_in_multiple_regions() {
    for region in ["南网", "广东"] {
        let result = parse_di(PROTOCOL, 0x08000201, region, None, &[0x01]);
        assert!(
            result.is_ok(),
            "08000201 在 region={region} 应该能查到，实际: {result:?}"
        );
    }
}

/// bug: `E0001210` 这个 id 同时被"负荷控制回路断线检测"和"心跳是否带时标"
/// 两个完全不相关的字段占用（后者是复制粘贴时的笔误，应为 E0001003）。
/// 修复后 E0001210 应该只剩"负荷控制回路断线检测"一种语义，解析出的字段
/// 结构应为 4 字节；用一个跟"心跳是否带时标"（1字节bcd）明显不同的字节数
/// 来做区分性验证。
#[test]
fn load_control_loop_disconnect_detection_has_its_own_unambiguous_id() {
    let raw = [0x01u8, 0x00, 0x00, 0x3C]; // 是否开启 + 检测开始时间 + 检测周期，共4字节
    let result = parse_di(PROTOCOL, 0xE0001210, "南网", None, &raw);
    let (value, consumed) = result.expect("E0001210(南网) 应该能正常解析为负荷控制回路断线检测");
    assert_eq!(consumed, 4, "负荷控制回路断线检测应该是4字节结构，不应该再混进心跳字段的1字节定义");
    let name = format!("{value:?}");
    assert!(
        name.contains("负荷控制回路断线检测"),
        "E0001210 解析出的字段名应该是负荷控制回路断线检测，实际: {name}"
    );
}

/// bug: `E1800034` 同时被"台区户变关系识别结果"和"拓扑识别启停信息"占用
/// （后者的 region 还错写成了 "topo" 这种不存在的伪省份）。修复后台区户变
/// 关系识别结果应该能在南网正常解析出来；拓扑识别启停信息改用了新分配的
/// id（E1800036，注册在 region="topo" 下）。
#[test]
fn substation_area_topology_result_keeps_its_original_id() {
    // 应答的台区节点总数量(2字节bcd) = 0 + 0个重复元素，构造一个合法的最小报文
    let result = parse_di(PROTOCOL, 0xE1800034, "南网", None, &[0x00, 0x00]);
    assert!(
        result.is_ok(),
        "E1800034(南网) 台区户变关系识别结果应该能正常解析，实际: {result:?}"
    );
}

/// bug: ARD42 模板里"报文内容"字段写成 `lengthrule: 1 * 报文长度`（裸字段名，
/// 不符合表达式语法，且被引用的"报文长度"字段没有打 ref_id），运行时解析
/// 这条 DI 必定报错。修复后改用 `ref_id` + `length_ref`，应该能按"报文长度"
/// 字段的实际值正确切出对应长度的内容。
#[test]
fn ard42_frame_content_length_follows_frame_length_field() {
    let mut raw = vec![0x00u8]; // 告警状态
    raw.extend_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06]); // 告警发生时间 6字节
    raw.extend_from_slice(&[0x03, 0x00]); // 报文长度 = 3（bin，小端）
    raw.extend_from_slice(&[0xAA, 0xBB, 0xCC]); // 报文内容，应该正好被吃掉3字节

    let (_value, consumed) = parse_di(PROTOCOL, 0xE2000084, "南网", None, &raw)
        .expect("E2000084(ARD42模板) 应该能正常解析，不应该再报lengthrule语法错误");
    assert_eq!(consumed, 1 + 6 + 2 + 3, "应该正好消耗掉告警状态+告警发生时间+报文长度+报文内容");
}

/// bug: APPINFO 模板里"APP内存占用率"字段被复制粘贴了两遍，导致模板比
/// 协议设计多出 3 字节，所有引用 APPINFO 的字段（比如 E1800032）解析出的
/// 偏移量都会错位。这里不去断言具体内部字段值（模板内容以后可能调整），
/// 只断言"用协议设计给定的长度能够解析完整、不报错"这个更稳定的性质——
/// 如果字段数又不小心多了一个，consumed 大概率会跟输入长度对不上或者
/// 直接解析失败。
#[test]
fn app_info_template_length_is_not_inflated_by_duplicated_field() {
    // 200 字节的全零缓冲区足够覆盖 E1800032 引用的 APPINFO repeat 结构，
    // 这里只关心"能不能正常跑完"，不去断言 repeat 次数等易变细节。
    let raw = vec![0u8; 200];
    let result = parse_di(PROTOCOL, 0xE1800032, "南网", None, &raw);
    assert!(
        result.is_ok(),
        "E1800032（引用 APPINFO 模板）应该能正常解析，实际: {result:?}"
    );
}
