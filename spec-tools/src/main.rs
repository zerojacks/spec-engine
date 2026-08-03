//! spec-tools - CLI tools for DI dictionary management
//!
//! 提供以下子命令：
//! - compile: 编译 YAML 字典为二进制格式
//! - query: 查询 DI 定义
//! - stats: 显示字典统计信息
//! - validate: 验证 YAML 字典语法和语义

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use spec_compiler::{Compiler, CompilerConfig};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "spec")]
#[command(about = "DI 字典管理工具", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 编译 YAML 字典为二进制格式
    Compile {
        /// YAML 字典目录（包含协议子目录）
        #[arg(short, long)]
        input: PathBuf,

        /// 输出二进制文件路径
        #[arg(short, long)]
        output: PathBuf,

        /// 启用详细输出
        #[arg(short, long)]
        verbose: bool,

        /// 跳过语义校验
        #[arg(long)]
        no_validate: bool,
    },

    /// 查询 DI 定义
    Query {
        /// 二进制字典文件路径
        #[arg(short = 'D', long)]
        dict: PathBuf,

        /// DI 码（十六进制，如 00010000）
        #[arg(short = 'i', long)]
        di: String,

        /// 协议名称
        #[arg(short, long)]
        protocol: String,

        /// 区域名称（默认：南网）
        #[arg(short, long, default_value = "南网")]
        region: String,

        /// 方向（0/1，可选）
        #[arg(short = 'd', long)]
        dir: Option<String>,

        /// 输出格式：text 或 json
        #[arg(short = 'f', long, default_value = "text")]
        format: String,
    },

    /// 显示字典统计信息
    Stats {
        /// 二进制字典文件路径或 YAML 目录
        #[arg(short, long)]
        input: PathBuf,

        /// 按协议分组统计
        #[arg(long)]
        by_protocol: bool,

        /// 按区域分组统计
        #[arg(long)]
        by_region: bool,
    },

    /// 验证 YAML 字典语法和语义
    Validate {
        /// YAML 字典目录
        #[arg(short, long)]
        input: PathBuf,

        /// 启用详细输出
        #[arg(short, long)]
        verbose: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Compile {
            input,
            output,
            verbose,
            no_validate,
        } => cmd_compile(input, output, verbose, !no_validate),

        Commands::Query {
            dict,
            di,
            protocol,
            region,
            dir,
            format,
        } => cmd_query(dict, di, protocol, region, dir, format),

        Commands::Stats {
            input,
            by_protocol,
            by_region,
        } => cmd_stats(input, by_protocol, by_region),

        Commands::Validate { input, verbose } => cmd_validate(input, verbose),
    }
}

fn cmd_compile(input: PathBuf, output: PathBuf, verbose: bool, validate: bool) -> Result<()> {
    println!("编译 YAML 字典...");
    println!("  输入目录: {}", input.display());
    println!("  输出文件: {}", output.display());

    let compiler = Compiler::with_config(CompilerConfig { verbose, validate });

    let table = compiler
        .compile_schema_dir(&input)
        .map_err(|e| anyhow::anyhow!("编译失败: {}", e))?;

    let bytes = bincode::serialize(&table).context("序列化失败")?;

    std::fs::write(&output, &bytes)
        .with_context(|| format!("写入文件 {} 失败", output.display()))?;

    println!();
    println!("✓ 编译完成");
    println!("  条目数: {}", table.len());
    println!("  文件大小: {} 字节 ({:.2} MB)", bytes.len(), bytes.len() as f64 / 1024.0 / 1024.0);

    Ok(())
}

fn cmd_query(
    dict: PathBuf,
    di: String,
    protocol: String,
    region: String,
    dir: Option<String>,
    format: String,
) -> Result<()> {
    // 解析 DI 码
    let di_num = u32::from_str_radix(&di, 16)
        .with_context(|| format!("无效的 DI 码: {}", di))?;

    // 加载字典
    let table = Compiler::load_from_bin(&dict)
        .map_err(|e| anyhow::anyhow!("加载字典文件失败: {}", e))?;

    // 查找 DI（带 region 回退）
    let key = (protocol.clone(), di_num, region.clone(), dir.clone());
    
    let field = table.get(&key)
        .or_else(|| {
            // 回退：忽略 dir
            if dir.is_some() {
                table.get(&(protocol.clone(), di_num, region.clone(), None))
            } else {
                None
            }
        })
        .or_else(|| {
            // 回退：默认 region + dir
            if region != "南网" {
                table.get(&(protocol.clone(), di_num, "南网".to_string(), dir.clone()))
                    .or_else(|| table.get(&(protocol.clone(), di_num, "南网".to_string(), None)))
            } else {
                None
            }
        })
        .with_context(|| {
            format!(
                "未找到 DI: 0x{:08X} (protocol={}, region={}, dir={:?})",
                di_num, protocol, region, dir
            )
        })?;

    // 输出结果
    match format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(field)
                .context("JSON 序列化失败")?;
            println!("{}", json);
        }
        "text" | _ => {
            println!("╔════════════════════════════════════════════════════════╗");
            println!("║                  DI 定义查询结果                       ║");
            println!("╚════════════════════════════════════════════════════════╝");
            println!();
            println!("DI 码: 0x{:08X}", di_num);
            println!("协议: {}", protocol);
            println!("区域: {}", region);
            if let Some(d) = dir {
                println!("方向: {}", d);
            }
            println!();
            if let Some(id) = &field.id {
                println!("ID: {}", id);
            }
            if let Some(ref_id) = &field.ref_id {
                println!("Ref ID: {}", ref_id);
            }
            println!("名称: {}", field.name);
            println!();
            println!("字段定义: {:#?}", field.spec);
        }
    }

    Ok(())
}

fn cmd_stats(input: PathBuf, by_protocol: bool, by_region: bool) -> Result<()> {
    let table = if input.is_dir() {
        // 从 YAML 目录编译
        println!("从 YAML 目录编译字典...");
        let compiler = Compiler::new();
        compiler.compile_schema_dir(&input)
            .map_err(|e| anyhow::anyhow!("编译失败: {}", e))?
    } else {
        // 从二进制文件加载
        Compiler::load_from_bin(&input)
            .map_err(|e| anyhow::anyhow!("加载文件失败: {}", e))?
    };

    println!("╔════════════════════════════════════════════════════════╗");
    println!("║                  字典统计信息                          ║");
    println!("╚════════════════════════════════════════════════════════╝");
    println!();
    println!("总条目数: {}", table.len());
    println!();

    if by_protocol {
        let mut protocols = std::collections::HashMap::new();
        for (protocol, _, _, _) in table.keys() {
            *protocols.entry(protocol.clone()).or_insert(0) += 1;
        }

        println!("按协议分组:");
        let mut sorted: Vec<_> = protocols.iter().collect();
        sorted.sort_by_key(|(name, _)| *name);
        for (protocol, count) in sorted {
            println!("  {}: {} 条目", protocol, count);
        }
        println!();
    }

    if by_region {
        let mut regions = std::collections::HashMap::new();
        for (_, _, region, _) in table.keys() {
            *regions.entry(region.clone()).or_insert(0) += 1;
        }

        println!("按区域分组:");
        let mut sorted: Vec<_> = regions.iter().collect();
        sorted.sort_by_key(|(name, _)| *name);
        for (region, count) in sorted {
            println!("  {}: {} 条目", region, count);
        }
        println!();
    }

    if !by_protocol && !by_region {
        // 默认显示协议和区域数量
        let mut protocols = std::collections::HashSet::new();
        let mut regions = std::collections::HashSet::new();
        for (protocol, _, region, _) in table.keys() {
            protocols.insert(protocol.clone());
            regions.insert(region.clone());
        }

        println!("协议数量: {}", protocols.len());
        println!("区域数量: {}", regions.len());
        println!();
        println!("提示: 使用 --by-protocol 或 --by-region 查看详细分组统计");
    }

    Ok(())
}

fn cmd_validate(input: PathBuf, verbose: bool) -> Result<()> {
    println!("验证 YAML 字典...");
    println!("  输入目录: {}", input.display());
    println!();

    let compiler = Compiler::with_config(CompilerConfig {
        verbose,
        validate: true,
    });

    match compiler.compile_schema_dir(&input) {
        Ok(table) => {
            println!("✓ 验证通过");
            println!("  条目数: {}", table.len());
            Ok(())
        }
        Err(e) => {
            eprintln!("✗ 验证失败:");
            eprintln!("  {}", e);
            std::process::exit(1);
        }
    }
}
