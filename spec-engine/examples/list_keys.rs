//! 列出特定 DI 键的示例

use spec_engine::Engine;

fn main() {
    let engine = Engine::new_default();
    
    let wanted: Vec<u32> = vec![
        0x00010001, 0x00010002, 0x00010003, 0x00010004, 0x00020001, 0x00020002, 0x00020003,
        0x00020004, 0x00030001, 0x00030002, 0x00030003, 0x00030004,
    ];

    println!("=== 查找指定的 DI 键 ===\n");
    
    for di in wanted {
        match engine.lookup("csg13", di, "南网", None) {
            Some(field) => {
                println!("DI {:08X}: {}", di, field.name);
            }
            None => {
                println!("DI {:08X}: (未找到)", di);
            }
        }
    }
}
