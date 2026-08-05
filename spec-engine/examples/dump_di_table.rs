// 这个示例展示如何访问内置字典的条目
// 注意：直接访问静态字典是内部实现细节，正常使用应该通过 Engine API

fn main() {
    println!("=== DI 字典条目数量 ===");
    println!();
    println!("注意：这个示例需要访问内部 API。");
    println!("推荐使用方式：");
    println!();
    println!("use spec_engine::Engine;");
    println!();
    println!("let engine = Engine::new_default();");
    println!("// 使用 engine.lookup() 查找特定 DI");
    println!("// 使用 engine.parse() 解析数据");
    println!();
    println!("内置字典已在编译时嵌入，包含所有协议定义。");
}
