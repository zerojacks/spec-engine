//! build.rs —— 使用 spec-compiler 编译 schema 目录下的 YAML 字典
//!
//! 新方案：不再在 build.rs 中包含所有编译逻辑，而是调用 spec-compiler crate
//! 提供的 Compiler API。编译结果序列化为二进制写入 OUT_DIR/di_table.bin，
//! 运行时通过 include_bytes! 嵌入并反序列化。
//!
//! 相比旧方案（2195 行 build.rs），新方案的优势：
//! - build.rs 精简到 <50 行，职责单一
//! - 编译逻辑复用：spec-compiler 可被 build.rs 和运行时动态加载共享
//! - 更好的可测试性和可维护性

use std::env;
use std::fs;
use std::path::Path;
use spec_compiler::{Compiler, CompilerConfig};

fn main() {
    // schema/ 位于 workspace root，build.rs 在 spec-engine/ 子目录
    let schema_dir = "../schema";
    println!("cargo:rerun-if-changed={}", schema_dir);

    // 创建编译器实例（启用详细日志以便调试）
    let compiler = Compiler::with_config(CompilerConfig {
        verbose: true,
        validate: true,
    });

    // 编译整个 schema 目录
    let table = compiler
        .compile_schema_dir(schema_dir)
        .unwrap_or_else(|e| panic!("编译 schema 失败: {}", e));

    // 序列化为二进制
    let bytes = bincode::serialize(&table)
        .unwrap_or_else(|e| panic!("序列化 DI 表失败: {}", e));

    // 写入 OUT_DIR/di_table.bin
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR 未设置");
    let dest = Path::new(&out_dir).join("di_table.bin");
    fs::write(&dest, &bytes)
        .unwrap_or_else(|e| panic!("写入 {} 失败: {}", dest.display(), e));

    println!(
        "cargo:warning=DI 字典构建完成：{} 条目，序列化后 {} 字节",
        table.len(),
        bytes.len()
    );
}
