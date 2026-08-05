//! 简单使用示例 - 展示新的 Engine API

use spec_engine::{Engine, EngineConfig, DEFAULT_REGION};

fn main() {
    println!("╔════════════════════════════════════════════════════════╗");
    println!("║            spec-engine 简单使用示例                    ║");
    println!("╚════════════════════════════════════════════════════════╝");
    println!();

    // 示例 1：创建 Engine（只使用静态字典）
    println!("【示例 1】创建 Engine（只使用静态字典）");
    let engine = Engine::new_default();
    println!("  ✓ Engine 创建成功");
    println!("    动态层数: {}", engine.layer_count());
    println!();

    // 示例 2：使用 Engine 解析数据
    println!("【示例 2】使用 Engine 解析数据");
    let data = vec![0x01, 0x23, 0x45, 0x67];
    
    match engine.parse("csg13", 0x00010000, DEFAULT_REGION, None, &data) {
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

    // 示例 3：查找 DI 定义（不解析数据）
    println!("【示例 3】查找 DI 定义（不解析数据）");
    if let Some(field) = engine.lookup("csg13", 0x00010000, DEFAULT_REGION, None) {
        println!("  ✓ 找到定义");
        println!("    字段名: {}", field.name);
        if let Some(id) = &field.id {
            println!("    ID: {}", id);
        }
    } else {
        println!("  ✗ 未找到");
    }
    println!();

    // 示例 4：使用完整版 parse（指定 region 和 dir）
    println!("【示例 4】使用完整版 parse（指定 region 和 dir）");
    match engine.parse("csg13", 0x00010000, "广东", None, &data) {
        Ok((_value, consumed)) => {
            println!("  ✓ 解析成功（使用广东定义或回退到南网）");
            println!("    消耗字节数: {}", consumed);
        }
        Err(e) => {
            println!("  ✗ 解析失败: {}", e);
        }
    }
    println!();

    // 示例 5：使用 EngineConfig 构建器
    println!("【示例 5】使用 EngineConfig 构建器");
    let config = EngineConfig::new();
    // 注意：这里演示 API，不加载实际的动态层
    println!("  ✓ EngineConfig 创建成功");
    println!("    配置源数: {}", config.source_count());
    println!();

    // 示例 6：Engine 是轻量 Clone 的
    println!("【示例 6】Engine 是轻量 Clone 的（Arc-based）");
    let engine_clone = engine.clone();
    println!("  ✓ Engine 克隆成功（只是 Arc 引用计数+1）");
    println!("    原 Engine 层数: {}", engine.layer_count());
    println!("    克隆 Engine 层数: {}", engine_clone.layer_count());
    println!();

    println!("╔════════════════════════════════════════════════════════╗");
    println!("║                   示例完成                             ║");
    println!("╚════════════════════════════════════════════════════════╝");
    println!();
    println!("💡 新架构特点：");
    println!("   • Engine 是不可变的，配置快照在创建时确定");
    println!("   • Engine 是线程安全的，可以在多线程间共享");
    println!("   • Engine.clone() 是轻量的（Arc-based）");
    println!("   • 使用 EngineConfig 构建器创建带动态层的 Engine");
    println!("   • parse() 支持指定 protocol、DI、region 和 dir");
}
