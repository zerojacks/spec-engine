use spec_engine::{get_di_table, init_registries};

fn main() {
    init_registries();
    let table = get_di_table();
    println!("DI table entries: {}", table.len());
    for ((protocol, di, region, dir), named_field) in table.iter() {
        println!(
            "key=(protocol={}, di=0x{:08X}, region={}, dir={:?})",
            protocol, di, region, dir
        );
        println!("  NamedField = {:?}", named_field);
    }
}
