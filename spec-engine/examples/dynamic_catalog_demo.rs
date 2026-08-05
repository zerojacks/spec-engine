//! Engine 动态加载示例
//!
//! 演示如何：
//! 1. 使用编译时嵌入的基础字典
//! 2. 运行时从 YAML 文件加载额外的层
//! 3. 层的优先级和覆盖规则
//! 4. 使用 EngineConfig 构建带动态层的 Engine
//! 5. Engine 的热更新（创建新实例）

use spec_engine::{Engine, EngineConfig, Layer, DiTable};

fn main() {
    println!("╔════════════════════════════════════════════════════════╗");
    println!("║         Engine 动态加载功能演示                        ║");
    println!("╚════════════════════════════════════════════════════════╝");
    println!();

    // 1. 创建只使用静态字典的 Engine
    println!("【步骤 1】创建基础 Engine（只使用静态字典）");
    let static_engine = Engine::new_default();
    println!("  动态层数：{}", static_engine.layer_count());
    println!();

    // 2. 查看静态字典中原有的 DI
    println!("【步骤 2】查看静态字典中的原始定义");
    
    println!("  ◆ DI 0x00010000 (csg13, 南网):");
    if let Some(field) = static_engine.lookup("csg13", 0x00010000, "南网", None) {
        println!("    名称: {}", field.name);
        println!("    来源: 静态字典");
    } else {
        println!("    未找到");
    }
    println!();

    // 3. 使用 EngineConfig 构建带动态层的 Engine
    println!("【步骤 3】使用 EngineConfig 加载动态层");
    let yaml_path = "examples/schema/csg13";
    
    let dynamic_engine = match EngineConfig::new()
        .yaml_dir(yaml_path)
        .build()
    {
        Ok(engine) => {
            println!("  ✓ 成功加载动态层");
            println!("  当前层数: {}", engine.layer_count());
            println!("  层列表: {:?}", engine.layer_names());
            engine
        }
        Err(e) => {
            eprintln!("  ✗ 加载失败: {}", e);
            eprintln!("  提示: 请确保运行目录在 spec-engine 根目录");
            return;
        }
    };
    println!();

    // 4. 对比静态和动态 Engine
    println!("【步骤 4】对比静态 Engine 和动态 Engine");
    
    println!("  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  ◆ DI 0x00010000 对比");
    println!("  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    // 静态 Engine
    if let Some(field) = static_engine.lookup("csg13", 0x00010000, "南网", None) {
        println!("  静态 Engine:");
        println!("    名称: {}", field.name);
    }
    
    // 动态 Engine
    if let Some(field) = dynamic_engine.lookup("csg13", 0x00010000, "南网", None) {
        println!("  动态 Engine:");
        println!("    名称: {}", field.name);
        if field.name.contains("动态") {
            println!("    状态: ✓ 已被动态层覆盖");
        }
    }
    println!();

    // 5. 测试解析（使用动态 Engine）
    println!("【步骤 5】测试解析（使用动态 Engine）");
    let data = vec![0x01, 0x23, 0x45, 0x67];
    
    match dynamic_engine.parse("csg13", 0x00010000, "南网", None, &data) {
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

    // 6. 热更新演示：创建新的 Engine 实例
    println!("【步骤 6】热更新演示（创建新 Engine 实例）");
    
    // 模拟热更新：加载不同的配置
    let _updated_engine = match EngineConfig::new()
        .yaml_dir(yaml_path)
        .build()
    {
        Ok(engine) => {
            println!("  ✓ 创建新 Engine 成功");
            println!("  旧 Engine 仍然可用（不影响现有解析）");
            println!("  新 Engine 层数: {}", engine.layer_count());
            engine
        }
        Err(e) => {
            println!("  ✗ 创建失败: {}", e);
            return;
        }
    };
    
    // 验证旧 Engine 仍然可用
    println!("  验证旧 Engine 仍然可用:");
    if dynamic_engine.lookup("csg13", 0x00010000, "南网", None).is_some() {
        println!("    ✓ 旧 Engine 正常工作");
    }
    
    println!();

    // 7. 多层配置演示
    println!("【步骤 7】多层配置演示");
    
    // 创建多层的 Engine  
    let layer1 = Layer::new("base".to_string(), DiTable::new());
    let layer2 = Layer::new("custom".to_string(), DiTable::new());
    
    let multi_layer_engine = Engine::with_layers(
        vec![layer1, layer2],
    );
    
    println!("  ✓ 创建多层 Engine");
    println!("  层列表: {:?}", multi_layer_engine.layer_names());
    println!("  优先级: {} > {} > 静态字典", 
        multi_layer_engine.layer_names().get(1).unwrap_or(&"".to_string()),
        multi_layer_engine.layer_names().get(0).unwrap_or(&"".to_string())
    );
    println!();

    // 8. Engine Clone 演示
    println!("【步骤 8】Engine Clone 演示（轻量复制）");
    let cloned_engine = dynamic_engine.clone();
    println!("  ✓ Engine 克隆成功");
    println!("  原 Engine 层数: {}", dynamic_engine.layer_count());
    println!("  克隆 Engine 层数: {}", cloned_engine.layer_count());
    println!("  说明: Clone 只是增加 Arc 引用计数，非常轻量");
    println!();

    println!("╔════════════════════════════════════════════════════════╗");
    println!("║                   演示完成                             ║");
    println!("╚════════════════════════════════════════════════════════╝");
    println!();
    println!("💡 新架构特点：");
    println!("   • Engine 是不可变的，配置在创建时确定");
    println!("   • 热更新通过创建新 Engine 实例实现");
    println!("   • 旧 Engine 实例仍然有效，可以平滑切换");
    println!("   • Engine 是轻量 Clone 的（Arc-based）");
    println!("   • 使用 EngineConfig 构建器创建复杂配置");
    println!();
    println!("💡 提示：");
    println!("   可以修改 examples/schema/csg13/dynamic.yaml 添加更多测试 DI");
    println!("   然后重新运行此示例查看效果");
}
