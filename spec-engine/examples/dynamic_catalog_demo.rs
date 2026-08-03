//! DynamicCatalog 使用示例
//!
//! 演示如何：
//! 1. 使用编译时嵌入的基础字典
//! 2. 运行时从 YAML 文件加载额外的层
//! 3. 层的优先级和覆盖规则
//! 4. 层管理（加载、卸载、重载）
//! 5. Region 特定的定义

use spec_engine::dynamic_loader::DynamicCatalog;
use spec_engine::get_spec_catalog;

fn main() {
    println!("╔════════════════════════════════════════════════════════╗");
    println!("║         DynamicCatalog 功能演示                        ║");
    println!("╚════════════════════════════════════════════════════════╝");
    println!();

    // 1. 创建 DynamicCatalog，使用编译时嵌入的字典作为基础层
    println!("【步骤 1】创建 DynamicCatalog（基于嵌入字典）");
    let embedded = get_spec_catalog().clone();
    println!("  嵌入字典条目数：{}", embedded.len());
    
    let mut catalog = DynamicCatalog::new(embedded);
    println!("  初始动态层数：{}", catalog.layer_count());
    println!();

    // 2. 查看嵌入字典中原有的 DI
    println!("【步骤 2】查看嵌入字典中的原始定义");
    
    println!("  ◆ DI 0x00010000 (csg13, 南网):");
    if let Some(field) = catalog.lookup("csg13", 0x00010000, "南网", None) {
        println!("    名称: {}", field.name);
        println!("    定义: {:?}", field.spec);
        println!("    来源: 嵌入字典");
    } else {
        println!("    未找到");
    }
    
    println!();
    println!("  ◆ DI 0x00020000 (csg13, 南网):");
    if let Some(field) = catalog.lookup("csg13", 0x00020000, "南网", None) {
        println!("    名称: {}", field.name);
        println!("    定义: {:?}", field.spec);
        println!("    来源: 嵌入字典");
    } else {
        println!("    未找到");
    }
    println!();

    // 3. 从 YAML 文件加载动态层
    println!("【步骤 3】从 YAML 文件加载动态层");
    let yaml_path = "examples/schema";
    
    match catalog.load_yaml_dir("dynamic_layer".to_string(), yaml_path) {
        Ok(()) => {
            println!("  ✓ 成功从 {} 加载动态层", yaml_path);
            println!("  当前层数: {}", catalog.layer_count());
        }
        Err(e) => {
            eprintln!("  ✗ 加载失败: {}", e);
            eprintln!("  提示: 请确保运行目录在 spec-engine 根目录");
            return;
        }
    }
    println!();

    // 4. 测试覆盖：查看被动态层覆盖后的 DI
    println!("【步骤 4】测试动态覆盖（对比前后变化）");
    
    println!("  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  ◆ DI 0x00010000 - 类型和精度改变");
    println!("  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    if let Some(field) = catalog.lookup("csg13", 0x00010000, "南网", None) {
        println!("  名称: {}", field.name);
        println!("  定义: {:?}", field.spec);
        if field.name.contains("动态覆盖") {
            println!("  状态: ✓ 已被动态层覆盖");
            println!("  变化: 4字节/2位小数 → 8字节/4位小数");
        } else {
            println!("  状态: ✗ 未被覆盖（使用嵌入字典）");
        }
    }
    
    println!();
    println!("  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  ◆ DI 0x00020000 - 简单字段变为容器结构");
    println!("  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    if let Some(field) = catalog.lookup("csg13", 0x00020000, "南网", None) {
        println!("  名称: {}", field.name);
        match &field.spec {
            spec_engine::FieldSpec::Fixed { .. } => {
                println!("  类型: Fixed (简单字段)");
                println!("  状态: ✗ 未被覆盖（使用嵌入字典）");
            }
            spec_engine::FieldSpec::Container(fields) => {
                println!("  类型: Container (容器结构)");
                println!("  子字段数: {}", fields.len());
                println!("  状态: ✓ 已被动态层覆盖");
                println!("  变化: 简单 BCD 字段 → 包含状态/值/时间戳的复杂结构");
                for (i, f) in fields.iter().enumerate() {
                    println!("    [{} {}]", i + 1, f.name);
                }
            }
            _ => {
                println!("  类型: {:?}", field.spec);
            }
        }
    }
    println!();

    // 5. 测试新增的 DI（嵌入字典中不存在）
    println!("【步骤 5】测试动态新增的 DI");
    
    // 测试复杂的容器字段
    if let Some(field) = catalog.lookup("csg13", 0x0000FF00, "南网", None) {
        println!("  DI 0x0000FF00 (csg13, 南网):");
        println!("    名称: {}", field.name);
        println!("    来源: ✓ 动态层（新增）");
    } else {
        println!("  DI 0x0000FF00: ✗ 未找到");
    }
    
    // 测试简单字段
    if let Some(field) = catalog.lookup("csg13", 0x0000FF01, "南网", None) {
        println!("  DI 0x0000FF01 (csg13, 南网):");
        println!("    名称: {}", field.name);
        println!("    来源: ✓ 动态层（新增）");
    } else {
        println!("  DI 0x0000FF01: ✗ 未找到");
    }
    println!();

    // 6. 测试 Region 特定的定义
    println!("【步骤 6】测试 Region 特定定义");
    
    // 云南特定
    if let Some(field) = catalog.lookup("csg13", 0x0000FF02, "云南", None) {
        println!("  DI 0x0000FF02 (csg13, 云南):");
        println!("    名称: {}", field.name);
        println!("    来源: ✓ 动态层（云南特定）");
    } else {
        println!("  DI 0x0000FF02 (云南): ✗ 未找到");
    }
    
    // 深圳特定
    if let Some(field) = catalog.lookup("csg13", 0x0000FF03, "深圳", None) {
        println!("  DI 0x0000FF03 (csg13, 深圳):");
        println!("    名称: {}", field.name);
        println!("    来源: ✓ 动态层（深圳特定）");
    } else {
        println!("  DI 0x0000FF03 (深圳): ✗ 未找到");
    }
    
    // 测试 Region 回退：在南网查找深圳特定的 DI（应该找不到）
    if let Some(field) = catalog.lookup("csg13", 0x0000FF03, "南网", None) {
        println!("  DI 0x0000FF03 (csg13, 南网):");
        println!("    名称: {}", field.name);
        println!("    来源: 回退到了深圳定义（不应该发生）");
    } else {
        println!("  DI 0x0000FF03 (csg13, 南网): ✓ 未找到（正确，因为这是深圳特定）");
    }
    println!();

    // 7. 层管理：重新加载
    println!("【步骤 7】测试层重新加载");
    match catalog.reload_yaml_dir("dynamic_layer", yaml_path) {
        Ok(()) => {
            println!("  ✓ 成功重新加载 dynamic_layer");
        }
        Err(e) => {
            println!("  ✗ 重新加载失败: {}", e);
        }
    }
    println!();

    // 8. 层管理：卸载
    println!("【步骤 8】测试层卸载（验证恢复原始定义）");
    println!("  卸载前层列表: {:?}", catalog.list_layers());
    
    match catalog.unload_layer("dynamic_layer") {
        Ok(removed) => {
            println!("  ✓ 已卸载层: {}", removed.name);
            println!("  卸载后层列表: {:?}", catalog.list_layers());
            
            println!();
            println!("  验证卸载后是否恢复原始定义：");
            
            // 验证 DI 0x00010000
            if let Some(field) = catalog.lookup("csg13", 0x00010000, "南网", None) {
                println!("    DI 0x00010000:");
                println!("      名称: {}", field.name);
                if field.name.contains("动态覆盖") {
                    println!("      状态: ✗ 仍然是动态层定义（卸载失败）");
                } else {
                    println!("      状态: ✓ 恢复为嵌入字典定义");
                    println!("      验证: 名称不再包含\"动态覆盖\"");
                }
            }
            
            // 验证 DI 0x00020000
            if let Some(field) = catalog.lookup("csg13", 0x00020000, "南网", None) {
                println!("    DI 0x00020000:");
                match &field.spec {
                    spec_engine::FieldSpec::Fixed { .. } => {
                        println!("      状态: ✓ 恢复为简单字段");
                    }
                    spec_engine::FieldSpec::Container(_) => {
                        println!("      状态: ✗ 仍然是容器结构（卸载失败）");
                    }
                    _ => {}
                }
            }
        }
        Err(e) => {
            println!("  ✗ 卸载失败: {}", e);
        }
    }
    println!();

    // 9. 统计信息
    println!("【步骤 9】完整统计信息");
    catalog.stats().print();

    println!();
    println!("╔════════════════════════════════════════════════════════╗");
    println!("║                   演示完成                             ║");
    println!("╚════════════════════════════════════════════════════════╝");
    println!();
    println!("💡 提示：");
    println!("   可以修改 examples/schema/csg13/dynamic.yaml 添加更多测试 DI");
    println!("   然后重新运行此示例查看效果");
}
