//! 简单使用示例 - 展示改进后的 API

// 使用 prelude 一次性导入所有常用 API
use spec_engine::prelude::*;

fn main() {
    println!("╔════════════════════════════════════════════════════════╗");
    println!("║            spec-engine 简单使用示例                    ║");
    println!("╚════════════════════════════════════════════════════════╝");
    println!();

    // 示例 1：简化的解析 API
    println!("【示例 1】使用简化的 parse() 函数");
    let data = vec![0x01, 0x23, 0x45, 0x67];
    
    match parse(0x00010000, "csg13", &data) {
        Ok((value, consumed)) => {
            println!("  ✓ 解析成功");
            println!("    消耗字节数: {}", consumed);
            println!("    解析结果: {:?}", value);
        }
        Err(e) => {
            println!("  ✗ 解析失败: {}", e);
        }
    }
    println!();

    // 示例 2：查找 DI 定义
    println!("【示例 2】查找 DI 定义（不解析数据）");
    if let Some(spec) = lookup_di_spec(0x00010000, "csg13", DEFAULT_REGION) {
        println!("  ✓ 找到定义");
        println!("    字段名: {}", spec.name);
        if let Some(id) = &spec.id {
            println!("    ID: {}", id);
        }
    } else {
        println!("  ✗ 未找到");
    }
    println!();

    // 示例 3：创建动态字典
    println!("【示例 3】创建动态字典");
    let catalog = create_dynamic_catalog();
    println!("  ✓ 动态字典已创建");
    println!("    嵌入字典条目数: {}", catalog.stats().embedded_entries);
    println!("    动态层数: {}", catalog.layer_count());
    println!();

    // 示例 4：访问静态字典
    println!("【示例 4】访问静态字典");
    let static_catalog = get_spec_catalog();
    println!("  ✓ 静态字典加载成功");
    println!("    总条目数: {}", static_catalog.len());
    
    // 统计协议数量
    let mut protocols = std::collections::HashSet::new();
    for (protocol, _, _, _) in static_catalog.keys() {
        protocols.insert(protocol);
    }
    println!("    协议数: {}", protocols.len());
    println!();

    println!("╔════════════════════════════════════════════════════════╗");
    println!("║                   示例完成                             ║");
    println!("╚════════════════════════════════════════════════════════╝");
    println!();
    println!("💡 注意：");
    println!("   • 使用 prelude::* 可以一次导入所有常用 API");
    println!("   • parse() 是简化版，适合大多数场景");
    println!("   • parse_di() 是完整版，支持 dir 参数");
    println!("   • create_dynamic_catalog() 自动使用嵌入字典");
}
