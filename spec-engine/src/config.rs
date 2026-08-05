//! 引擎配置模块
//!
//! 提供 `EngineConfig` 构建器和 `ConfigSource` 配置源定义。

use crate::dynamic_loader::{DiTable, Layer};
use crate::engine::Engine;
use std::path::{Path, PathBuf};
/// 配置源
///
/// 定义从何处加载 DI 定义的配置源类型。
///
/// # 变体说明
///
/// - `YamlFile`: 从单个 YAML 文件加载
/// - `YamlDir`: 从目录加载所有 YAML 文件（按文件名排序）
/// - `Binary`: 从二进制数据加载（预编译的字典）
/// - `Memory`: 从内存中的 DI 表加载（用于测试）
///
/// # 示例
///
/// ```rust,ignore
/// use spec_engine::ConfigSource;
///
/// let source1 = ConfigSource::YamlFile {
///     name: "custom".to_string(),
///     path: "/etc/app/custom.yaml".into(),
/// };
///
/// let source2 = ConfigSource::YamlDir {
///     path: "/etc/app/config".into(),
/// };
/// ```
#[derive(Debug, Clone)]
pub enum ConfigSource {
    /// YAML 文件
    YamlFile {
        /// 层名称
        name: String,
        /// 文件路径
        path: PathBuf,
    },

    /// YAML 目录（自动扫描所有 .yaml 文件）
    YamlDir {
        /// 目录路径
        path: PathBuf,
    },

    /// 二进制数据（预编译的字典）
    Binary {
        /// 层名称
        name: String,
        /// 二进制数据
        data: Vec<u8>,
    },

    /// 内存表（用于测试或动态生成）
    Memory {
        /// 层名称
        name: String,
        /// DI 定义表
        table: DiTable,
    },
}

impl ConfigSource {
    /// 加载配置源为 Layer
    fn load(&self) -> Result<Vec<Layer>, String> {
        match self {
            ConfigSource::YamlFile { name, path } => {
                let layer = Layer::from_yaml_dir(name.clone(), path)?;
                Ok(vec![layer])
            }

            ConfigSource::YamlDir { path } => {
                // 扫描目录下所有 .yaml 文件
                let mut yaml_files: Vec<PathBuf> = std::fs::read_dir(path)
                    .map_err(|e| format!("读取目录失败 {}: {}", path.display(), e))?
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.extension()
                            .and_then(|ext| ext.to_str())
                            .map(|ext| ext == "yaml" || ext == "yml")
                            .unwrap_or(false)
                    })
                    .collect();

                // 按文件名排序（支持数字前缀控制顺序）
                yaml_files.sort();

                // 加载所有文件
                let mut layers = Vec::new();
                for file_path in yaml_files {
                    let name = file_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unnamed")
                        .to_string();

                    let layer = Layer::from_yaml_dir(name, &file_path)?;
                    layers.push(layer);
                }

                Ok(layers)
            }

            ConfigSource::Binary { name, data } => {
                // 从二进制数据反序列化
                let table: DiTable = bincode::deserialize(data)
                    .map_err(|e| format!("反序列化二进制数据失败: {}", e))?;

                let layer = Layer::new(name.clone(), table);
                Ok(vec![layer])
            }

            ConfigSource::Memory { name, table } => {
                let layer = Layer::new(name.clone(), table.clone());
                Ok(vec![layer])
            }
        }
    }
}

/// 引擎配置构建器
///
/// 用于配置和构建 `Engine` 实例的构建器模式实现。
///
/// # 设计模式
///
/// 使用构建器模式链式调用添加配置源，最后调用 `build()` 创建 Engine。
///
/// # 示例
///
/// ## 基础用法
///
/// ```rust,ignore
/// use spec_engine::EngineConfig;
///
/// let engine = EngineConfig::new()
///     .yaml_file("custom", "/etc/app/custom.yaml")
///     .build()?;
/// ```
///
/// ## 加载目录
///
/// ```rust,ignore
/// let engine = EngineConfig::new()
///     .yaml_dir("/etc/app/config")
///     .build()?;
/// ```
///
/// ## 多个配置源
///
/// ```rust,ignore
/// let engine = EngineConfig::new()
///     .yaml_file("base", "/etc/app/base.yaml")
///     .yaml_file("guangdong", "/etc/app/guangdong.yaml")
///     .yaml_file("custom", "/etc/app/custom.yaml")
///     .build()?;
/// ```
///
/// ## 从环境变量
///
/// ```rust,ignore
/// let engine = EngineConfig::from_env()?;
/// ```
#[derive(Debug, Default)]
pub struct EngineConfig {
    sources: Vec<ConfigSource>,
}

impl EngineConfig {
    /// 创建新的配置构建器
    ///
    /// # 示例
    ///
    /// ```rust
    /// use spec_engine::EngineConfig;
    ///
    /// let config = EngineConfig::new();
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加 YAML 文件配置源
    ///
    /// # 参数
    ///
    /// - `name`: 层名称（用于标识）
    /// - `path`: YAML 文件路径
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// use spec_engine::EngineConfig;
    ///
    /// let config = EngineConfig::new()
    ///     .yaml_file("custom", "/etc/app/custom.yaml");
    /// ```
    pub fn yaml_file(mut self, name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        self.sources.push(ConfigSource::YamlFile {
            name: name.into(),
            path: path.into(),
        });
        self
    }

    /// 添加 YAML 目录配置源
    ///
    /// 自动扫描目录下所有 .yaml 和 .yml 文件，按文件名排序加载。
    /// 支持数字前缀控制加载顺序（如 `10_base.yaml`, `20_custom.yaml`）。
    ///
    /// # 参数
    ///
    /// - `path`: 目录路径
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// use spec_engine::EngineConfig;
    ///
    /// let config = EngineConfig::new()
    ///     .yaml_dir("/etc/app/config");
    /// ```
    pub fn yaml_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.sources.push(ConfigSource::YamlDir {
            path: path.into(),
        });
        self
    }

    /// 添加二进制数据配置源
    ///
    /// 从预编译的二进制数据加载字典。
    ///
    /// # 参数
    ///
    /// - `name`: 层名称
    /// - `data`: 二进制数据（由 `bincode` 序列化的 `DiTable`）
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// use spec_engine::EngineConfig;
    ///
    /// let binary_data = std::fs::read("compiled.bin")?;
    /// let config = EngineConfig::new()
    ///     .binary("compiled", binary_data);
    /// ```
    pub fn binary(mut self, name: impl Into<String>, data: Vec<u8>) -> Self {
        self.sources.push(ConfigSource::Binary {
            name: name.into(),
            data,
        });
        self
    }

    /// 添加内存表配置源
    ///
    /// 从内存中的 DI 表加载。适用于测试或动态生成的定义。
    ///
    /// # 参数
    ///
    /// - `name`: 层名称
    /// - `table`: DI 定义表
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// use spec_engine::{EngineConfig, DiTable};
    /// use std::collections::HashMap;
    ///
    /// let mut table = DiTable::new();
    /// // ... 填充 table
    ///
    /// let config = EngineConfig::new()
    ///     .memory("test", table);
    /// ```
    pub fn memory(mut self, name: impl Into<String>, table: DiTable) -> Self {
        self.sources.push(ConfigSource::Memory {
            name: name.into(),
            table,
        });
        self
    }

    /// 从环境变量创建配置
    ///
    /// 按以下顺序尝试读取环境变量：
    ///
    /// 1. `SPEC_ENGINE_CONFIG`
    /// 2. `SPEC_CONFIG`
    ///
    /// 如果环境变量未设置或目录不存在，返回空配置（只使用静态字典）。
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// use spec_engine::EngineConfig;
    ///
    /// // 设置环境变量
    /// std::env::set_var("SPEC_ENGINE_CONFIG", "/etc/app/config");
    ///
    /// let config = EngineConfig::from_env()?;
    /// ```
    pub fn from_env() -> Result<Self, String> {
        let path = std::env::var("SPEC_ENGINE_CONFIG")
            .or_else(|_| std::env::var("SPEC_CONFIG"))
            .ok();

        match path {
            Some(p) if Path::new(&p).exists() => Ok(Self::new().yaml_dir(p)),
            _ => Ok(Self::new()), // 环境变量未设置或路径不存在，使用空配置
        }
    }

    /// 从指定环境变量创建配置
    ///
    /// # 参数
    ///
    /// - `var_name`: 环境变量名称
    /// - `default_path`: 默认路径（环境变量未设置时使用）
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// use spec_engine::EngineConfig;
    ///
    /// let config = EngineConfig::from_env_or(
    ///     "APP_CONFIG_DIR",
    ///     "/etc/app/config"
    /// )?;
    /// ```
    pub fn from_env_or(var_name: &str, default_path: &str) -> Result<Self, String> {
        let path = std::env::var(var_name).unwrap_or_else(|_| default_path.to_string());

        if Path::new(&path).exists() {
            Ok(Self::new().yaml_dir(path))
        } else {
            Ok(Self::new())
        }
    }

    /// 构建 Engine 实例
    ///
    /// 加载所有配置源并创建 Engine。
    ///
    /// # 错误
    ///
    /// - 配置源加载失败（文件不存在、格式错误等）
    /// - YAML 编译失败
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// use spec_engine::EngineConfig;
    ///
    /// let engine = EngineConfig::new()
    ///     .yaml_dir("/etc/app/config")
    ///     .build()?;
    /// ```
    pub fn build(self) -> Result<Engine, String> {
        // 加载所有配置源
        let mut layers = Vec::new();

        for source in self.sources {
            let mut source_layers = source.load()?;
            layers.append(&mut source_layers);
        }

        // 创建 Engine
        Ok(Engine::with_layers(layers))
    }

    /// 获取配置源数量
    ///
    /// # 示例
    ///
    /// ```rust
    /// use spec_engine::EngineConfig;
    ///
    /// let config = EngineConfig::new()
    ///     .yaml_file("a", "a.yaml")
    ///     .yaml_file("b", "b.yaml");
    ///
    /// assert_eq!(config.source_count(), 2);
    /// ```
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = EngineConfig::new()
            .yaml_file("test1", "test1.yaml")
            .yaml_file("test2", "test2.yaml");

        assert_eq!(config.source_count(), 2);
    }

    #[test]
    fn test_config_from_env_fallback() {
        // 当环境变量未设置时，应该返回空配置
        std::env::remove_var("SPEC_ENGINE_CONFIG");
        std::env::remove_var("SPEC_CONFIG");

        let config = EngineConfig::from_env().unwrap();
        assert_eq!(config.source_count(), 0);
    }

    #[test]
    fn test_config_memory_source() {
        use std::collections::HashMap;

        let table = HashMap::new();
        let config = EngineConfig::new().memory("test", table);

        assert_eq!(config.source_count(), 1);
    }

    #[test]
    fn test_config_multiple_sources() {
        use std::collections::HashMap;

        let config = EngineConfig::new()
            .yaml_file("a", "a.yaml")
            .yaml_dir("dir")
            .binary("b", vec![])
            .memory("c", HashMap::new());

        assert_eq!(config.source_count(), 4);
    }
}
