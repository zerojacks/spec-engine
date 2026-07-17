use spec_engine::get_di_table;

fn main() {
    let table = get_di_table();
    let wanted: Vec<u32> = vec![
        0x00010001, 0x00010002, 0x00010003, 0x00010004, 0x00020001, 0x00020002, 0x00020003,
        0x00030000, 0xE0000100, 0x00040000, 0x00050000, 0x00060000, 0x00070000, 0x00080000,
    ];
    let mut entries: Vec<_> = table
        .keys()
        .filter(|(protocol, di, _, _)| protocol == "csg13" && wanted.contains(di))
        .collect();
    entries.sort();
    for (protocol, di, region, dir) in entries.iter() {
        println!("{:?} 0x{:08X} {:?} {:?}", protocol, di, region, dir);
    }
}
