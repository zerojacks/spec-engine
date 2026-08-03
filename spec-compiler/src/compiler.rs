//! 编译器主模块
//!
//! 提供统一的编译接口，将 YAML 字典文件编译为运行时的 DI 表。
//! 
//! 主要功能：
//! - 从目录读取并合并多个 YAML 文件
//! - 语义校验
//! - 生成 FieldSpec 树
//! - 序列化为二进制格式

use crate::ast::{RawDict, DEFAULT_REGION};
use crate::context::{BuildCtx, BuildScope};
use crate::validator::validate_semantics;
use crate::generator::{collect_raw_ids, gen_named_field};
use crate::types::NamedField;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;

/// 编译器配置
#[derive(Debug, Clone)]
pub struct CompilerConfig {
    /// 是否启用详细日志
    pub verbose: bool,
    /// 是否在编译前验证语义
    pub validate: bool,
}

impl Default for CompilerConfig {
    fn default() -> Self {
        Self {
            verbose: false,
            validate: true,
        }
    }
}

/// DI 字典编译器
pub struct Compiler {
    config: CompilerConfig,
}

impl Compiler {
    /// 创建新的编译器实例
    pub fn new() -> Self {
        Self {
            config: CompilerConfig::default(),
        }
    }

    /// 使用指定配置创建编译器
    pub fn with_config(config: CompilerConfig) -> Self {
        Self { config }
    }

    /// 从 YAML 字符串编译单个字典
    pub fn compile_yaml_str(&self, yaml_content: &str, protocol: &str) -> Result<RawDict, String> {
        let mut dict: RawDict = serde_yaml::from_str(yaml_content)
            .map_err(|e| format!("YAML 解析失败: {}", e))?;

        // 为所有顶层条目设置协议
        for rf in &mut dict.data_items {
            if rf.protocol.is_none() {
                rf.protocol = Some(protocol.to_string());
            }
        }
        for template in &mut dict.templates {
            if template.protocol.is_none() {
                template.protocol = Some(protocol.to_string());
            }
        }

        Ok(dict)
    }

    /// 从 YAML 文件编译字典
    pub fn compile_yaml_file<P: AsRef<Path>>(&self, path: P, protocol: &str) -> Result<RawDict, String> {
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("读取文件 {} 失败: {}", path.as_ref().display(), e))?;
        
        self.compile_yaml_str(&content, protocol)
    }

    /// 从目录编译整个 schema（包含多个协议子目录）
    pub fn compile_schema_dir<P: AsRef<Path>>(&self, schema_dir: P) -> Result<HashMap<(String, u32, String, Option<String>), NamedField>, String> {
        let schema_path = schema_dir.as_ref();
        
        if self.config.verbose {
            println!("正在编译 schema 目录: {}", schema_path.display());
        }

        // 读取目录
        let mut entries: Vec<PathBuf> = fs::read_dir(schema_path)
            .map_err(|e| format!("无法读取 {} 目录: {}", schema_path.display(), e))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        entries.sort();

        // 检查根目录是否有未归类的 YAML 文件
        let stray: Vec<_> = entries
            .iter()
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .map(|e| e == "yaml" || e == "yml")
                        .unwrap_or(false)
            })
            .collect();
        if !stray.is_empty() {
            return Err(format!(
                "{} 目录下发现未归类的 .yaml 文件: {:?}\n\
                 字典文件必须放在协议子目录下，例如 schema/csg1209022/xxx.yaml，\n\
                 子目录名就是该文件里未显式指定 protocol 的顶层条目的默认协议。",
                schema_path.display(), stray
            ));
        }

        // 获取所有协议子目录
        let protocol_dirs: Vec<_> = entries.iter().filter(|p| p.is_dir()).collect();
        if protocol_dirs.is_empty() {
            return Err(format!(
                "{} 目录下没有找到任何协议子目录（例如 schema/csg1209022/）",
                schema_path.display()
            ));
        }

        // 合并所有字典
        let mut combined = RawDict::default();

        for proto_dir in &protocol_dirs {
            let protocol_name = proto_dir
                .file_name()
                .ok_or_else(|| format!("协议目录 {} 没有合法目录名", proto_dir.display()))?
                .to_string_lossy()
                .to_string();

            if self.config.verbose {
                println!("  处理协议: {}", protocol_name);
            }

            // 读取该协议下的所有 YAML 文件
            let mut paths: Vec<_> = fs::read_dir(proto_dir)
                .map_err(|e| format!("无法读取 {} 目录: {}", proto_dir.display(), e))?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .map(|ext| ext == "yaml" || ext == "yml")
                        .unwrap_or(false)
                })
                .collect();
            paths.sort();

            for path in &paths {
                if self.config.verbose {
                    println!("    读取文件: {}", path.display());
                }

                let content = fs::read_to_string(path)
                    .map_err(|e| format!("读取 {} 失败: {}", path.display(), e))?;
                let mut dict: RawDict = serde_yaml::from_str(&content)
                    .map_err(|e| format!("解析 {} 失败: {}", path.display(), e))?;

                // 为顶层条目设置协议
                for rf in &mut dict.data_items {
                    if rf.protocol.is_none() {
                        rf.protocol = Some(protocol_name.clone());
                    }
                }
                for template in &mut dict.templates {
                    if template.protocol.is_none() {
                        template.protocol = Some(protocol_name.clone());
                    }
                }

                combined.templates.extend(dict.templates);
                combined.data_items.extend(dict.data_items);
            }
        }

        // 语义校验
        if self.config.validate {
            if self.config.verbose {
                println!("正在进行语义校验...");
            }
            validate_semantics(&combined);
        }

        // 编译字典
        self.compile_dict(&combined)
    }

    /// 编译合并后的字典为 DI 表
    pub fn compile_dict(&self, combined: &RawDict) -> Result<HashMap<(String, u32, String, Option<String>), NamedField>, String> {
        if self.config.verbose {
            println!("正在编译字典...");
            println!("  模板数量: {}", combined.templates.len());
            println!("  数据项数量: {}", combined.data_items.len());
        }

        // 第一遍：建立 (id, protocol, region, dir) -> 原始定义 映射
        let mut di_raw_map = HashMap::new();
        for rf in &combined.data_items {
            let protocol = rf
                .protocol
                .clone()
                .ok_or_else(|| format!("顶层条目 {:?} 缺少 protocol", rf.id))?;
            let regions = rf
                .region
                .clone()
                .unwrap_or_else(|| vec![DEFAULT_REGION.to_string()]);
            let dir = rf.dir.clone();
            for region in regions {
                collect_raw_ids(rf, &protocol, &region, &dir, &mut di_raw_map);
            }
        }

        // 建立模板映射
        let mut templates = HashMap::new();
        for template in &combined.templates {
            let protocol = template
                .protocol
                .clone()
                .ok_or_else(|| format!("模板 {} 缺少 protocol", template.id))?;
            let regions = template
                .region
                .clone()
                .unwrap_or_else(|| vec![DEFAULT_REGION.to_string()]);
            let dir = template.dir.clone();
            if template.id.is_empty() {
                return Err("templates 中的模板条目缺少 id".to_string());
            }
            for region in regions {
                let key = (template.id.clone(), protocol.clone(), region.clone(), dir.clone());
                if templates.contains_key(&key) {
                    return Err(format!("模板重复定义: {:?}", key));
                }
                let mut template_for_region = template.clone();
                template_for_region.region = Some(vec![region.clone()]);
                template_for_region.protocol = Some(protocol.clone());
                template_for_region.dir = dir.clone();
                templates.insert(key, template_for_region);
            }
        }

        let mut ctx = BuildCtx::with_data(templates, di_raw_map);

        // 第二遍：展开每个顶层 data_item，构造 FieldSpec 值
        for rf in &combined.data_items {
            let top_protocol = rf
                .protocol
                .clone()
                .ok_or_else(|| format!("顶层条目 {:?} 缺少 protocol", rf.id))?;
            let top_regions = rf
                .region
                .clone()
                .unwrap_or_else(|| vec![DEFAULT_REGION.to_string()]);
            let top_dir = rf.dir.clone();
            for top_region in top_regions {
                let mut scope = BuildScope::new();
                // 收窄 region 为当前循环的单一值
                let mut rf_for_region = rf.clone();
                rf_for_region.region = Some(vec![top_region.clone()]);
                let _ = gen_named_field(
                    &rf_for_region,
                    &top_protocol,
                    &top_region,
                    top_dir.as_deref(),
                    &mut ctx,
                    &mut scope,
                );
            }
        }

        // 收拢为最终的 DI 表
        let mut table: HashMap<(String, u32, String, Option<String>), NamedField> = HashMap::new();
        for (id, protocol, region, dir, named_field) in ctx.registrations {
            table.insert((protocol, id, region, dir), named_field);
        }

        if self.config.verbose {
            println!("编译完成，共 {} 个条目", table.len());
        }

        Ok(table)
    }

    /// 编译并序列化为二进制格式
    pub fn compile_to_bin<P: AsRef<Path>>(&self, schema_dir: P) -> Result<Vec<u8>, String> {
        let table = self.compile_schema_dir(schema_dir)?;
        
        bincode::serialize(&table)
            .map_err(|e| format!("序列化 DI 表失败: {}", e))
    }

    /// 从二进制文件加载 DI 表
    pub fn load_from_bin<P: AsRef<Path>>(path: P) -> Result<HashMap<(String, u32, String, Option<String>), NamedField>, String> {
        let bytes = fs::read(&path)
            .map_err(|e| format!("读取文件 {} 失败: {}", path.as_ref().display(), e))?;
        
        bincode::deserialize(&bytes)
            .map_err(|e| format!("反序列化 DI 表失败: {}", e))
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compiler_creation() {
        let compiler = Compiler::new();
        assert!(!compiler.config.verbose);
        assert!(compiler.config.validate);
    }

    #[test]
    fn test_compiler_with_config() {
        let config = CompilerConfig {
            verbose: true,
            validate: false,
        };
        let compiler = Compiler::with_config(config);
        assert!(compiler.config.verbose);
        assert!(!compiler.config.validate);
    }
}
