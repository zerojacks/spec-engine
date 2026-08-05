//! # spec-compiler - DI Dictionary Compiler
//! 
//! `spec-compiler` 是一个将 YAML 字典定义编译为运行时 `FieldSpec` 结构的编译器库。
//! 它支持编译期静态编译和运行时动态加载两种模式。
//!
//! ## 核心功能
//!
//! - **YAML 解析与验证**：将声明式的 YAML 字典定义转换为强类型的运行时结构
//! - **模板展开**：支持 `template_ref` 和 `ref` 引用，实现代码复用
//! - **重复结构展开**：编译期展开 `candidate_ids`，支持静态计数和动态引用
//! - **类型检查与错误诊断**：提供详细的编译错误信息和位置提示
//! - **多层字典**：支持多个 YAML 文件组织成层级结构
//!
//! ## 基本使用
//!
//! ### 编译目录下的所有 YAML 文件
//!
//! ```rust,no_run
//! use spec_compiler::Compiler;
//!
//! let compiler = Compiler::new();
//! let table = compiler.compile_schema_dir("schema").unwrap();
//! println!("编译了 {} 个条目", table.len());
//! ```
//!
//! ### 编译单个 YAML 文件
//!
//! ```rust,ignore
//! use spec_compiler::Compiler;
//!
//! let compiler = Compiler::new();
//! // compile_yaml_file 需要指定协议名称
//! let raw_dict = compiler.compile_yaml_file("schema/csg13/csg13.yaml", "csg13").unwrap();
//! ```
//!
//! ### 自定义配置
//!
//! ```rust,no_run
//! use spec_compiler::{Compiler, CompilerConfig};
//!
//! let compiler = Compiler::with_config(CompilerConfig {
//!     verbose: true,
//!     validate: true,
//! });
//! let table = compiler.compile_schema_dir("schema").unwrap();
//! ```
//!
//! ## 输出结构
//!
//! 编译器输出的 `DiTable` 是一个 `HashMap`，键为 `(protocol, di, region, dir)`
//! 四元组，值为 `NamedField`。这个结构可以：
//!
//! - 直接用于运行时解析（通过 `spec-engine`）
//! - 序列化为二进制文件（通过 `bincode`）供嵌入式系统使用
//! - 用于生成代码或文档
//!
//! ## YAML 格式
//!
//! 详细的 YAML schema 定义请参考 `schema/di.schema.json` 和
//! `docs/DI字典YAML_Schema设计.md`。
//!
//! ### 简单示例
//!
//! ```yaml
//! data_items:
//!   - id: "00010000"
//!     name: "组合有功总电能"
//!     protocol: "csg13"
//!     region: ["南网"]
//!     length: 4
//!     type: bcd
//!     decimals: 2
//!     unit: "kWh"
//! ```
//!
//! ### 使用模板
//!
//! ```yaml
//! templates:
//!   - id: "energy_template"
//!     fields:
//!       - name: "电能值"
//!         length: 4
//!         type: bcd
//!         decimals: 2
//!         unit: "kWh"
//!
//! data_items:
//!   - id: "00010000"
//!     name: "总电能"
//!     template_ref: "energy_template"
//! ```
//!
//! ### 重复结构展开
//!
//! ```yaml
//! data_items:
//!   - id: "0000FF00"
//!     name: "电能数据块"
//!     fields:
//!       - candidate_ids:
//!           count: 4
//!           id_expr: "0x00000100 + index0*0x0100"
//!           name_template: "费率{index}电能"
//!           element:
//!             length: 4
//!             type: bcd
//!             decimals: 2
//! ```
//!
//! ## 错误处理
//!
//! 所有编译错误都通过 [`CompilerError`] 类型返回，包含详细的错误位置和提示信息：
//!
//! ```rust,ignore
//! use spec_compiler::{Compiler, CompilerError};
//!
//! let compiler = Compiler::new();
//! match compiler.compile_schema_dir("schema") {
//!     Ok(table) => println!("编译成功: {} 条目", table.len()),
//!     Err(message) => {
//!         eprintln!("编译错误: {}", message);
//!     }
//! }
//! ```

// === 公开模块 ===

/// 运行时字段类型系统定义
///
/// 包含所有运行时解析所需的类型定义，如 [`FieldSpec`]、[`Encoding`]、
/// [`FieldLength`]、[`Value`] 等核心类型。
///
/// 这些类型被设计为可序列化（通过 `serde`），既可以在编译期生成，
/// 也可以在运行时从二进制文件或 YAML 文件加载。
///
/// # 主要类型
///
/// - [`FieldSpec`](types::FieldSpec): 字段规格的核心枚举，定义了所有支持的字段类型
/// - [`NamedField`](types::NamedField): 带名称和元数据的字段
/// - [`Encoding`](types::Encoding): 数据编码方式（BCD、Binary、ASCII 等）
/// - [`FieldLength`](types::FieldLength): 字段长度定义（固定、引用、表达式）
/// - [`Value`](types::Value): 解析结果值的表示
///
/// [`FieldSpec`]: types::FieldSpec
/// [`Encoding`]: types::Encoding
/// [`FieldLength`]: types::FieldLength
/// [`Value`]: types::Value
/// [`NamedField`]: types::NamedField
pub mod types;

/// YAML AST 定义（直接映射 YAML 文件结构）
///
/// 定义了从 YAML 文件反序列化时使用的中间结构。这些类型对应
/// YAML schema 中的各个节点，包含可选字段和灵活的表示方式。
///
/// # 主要类型
///
/// - [`RawDict`](ast::RawDict): YAML 文件的根节点
/// - [`RawField`](ast::RawField): 字段定义的原始表示
/// - [`RawTemplate`](ast::RawTemplate): 模板定义
/// - [`RawCandidate`](ast::RawCandidate): 候选 DI 定义（用于编译期展开）
///
/// 这些类型由编译器内部使用，通常不需要直接操作。
///
/// [`RawDict`]: ast::RawDict
/// [`RawField`]: ast::RawField
/// [`RawTemplate`]: ast::RawTemplate
/// [`RawCandidate`]: ast::RawCandidate
pub mod ast;

/// 编译错误类型定义
///
/// 提供详细的错误类型和位置信息，帮助快速定位 YAML 配置问题。
///
/// # 主要类型
///
/// - [`CompilerError`]: 编译过程中的各类错误
/// - [`LoadError`]: 文件加载相关的错误
///
/// [`CompilerError`]: error::CompilerError
/// [`LoadError`]: error::LoadError
pub mod error;

/// 重复结构和表达式求值工具
///
/// 提供处理 `candidate_ids`、`count_expr`、`id_expr` 等重复结构的工具函数。
///
/// # 主要功能
///
/// - 计算 `count_expr` 表达式（如 `"2*ref(pn_count)"`）
/// - 展开 `id_expr` 表达式（如 `"0x00010100 + index0*0x0100"`）
/// - 生成重复字段的名称（基于 `name_template`）
///
/// # 示例
///
/// ```rust
/// use spec_compiler::repeat::eval_id_expr;
///
/// // 计算第3个元素的 DI ID
/// let di = eval_id_expr("0x00010100 + index0*0x0100", 3).unwrap();
/// assert_eq!(di, 0x00010400);
/// ```
pub mod repeat;

/// 便捷的工具函数别名
///
/// 重新导出 [`repeat`] 模块中的所有函数，提供更简短的导入路径。
///
/// # 使用方式
///
/// ```rust
/// use spec_compiler::utils::eval_id_expr;
/// ```
///
/// 等价于：
///
/// ```rust
/// use spec_compiler::repeat::eval_id_expr;
/// ```
pub mod utils {
    //! 辅助函数，用于处理 repeat 和 id 表达式
    //!
    //! 这个模块是 [`repeat`](crate::repeat) 模块的别名，提供相同的功能。
    pub use crate::repeat::*;
}

// === 内部模块（不公开）===
mod validator;
mod context;
mod generator;

// === 编译器主接口 ===
mod compiler;

/// 编译器主接口，用于将 YAML 字典编译为运行时结构
///
/// [`Compiler`] 是 spec-compiler 的核心 API，负责：
/// - 解析 YAML 文件
/// - 验证字段定义
/// - 展开模板和重复结构
/// - 生成运行时 `DiTable`
///
/// # 示例
///
/// ```rust,ignore
/// use spec_compiler::Compiler;
///
/// // 使用默认配置创建编译器
/// let compiler = Compiler::new();
///
/// // 编译整个目录
/// let table = compiler.compile_schema_dir("schema").unwrap();
/// ```
///
/// # 自定义配置
///
/// ```rust,no_run
/// use spec_compiler::{Compiler, CompilerConfig};
///
/// let compiler = Compiler::with_config(CompilerConfig {
///     verbose: true,  // 输出详细日志
///     validate: true, // 启用额外验证
/// });
/// ```
pub use compiler::{Compiler, CompilerConfig};

// === 便捷重新导出 ===

/// 常用类型，避免用户需要显式导入 `types::*`
///
/// 这些是最常用的核心类型，直接从 crate 根导出以简化使用。
///
/// # 核心字段类型
///
/// - [`FieldSpec`]: 字段规格定义的核心枚举
/// - [`NamedField`]: 带名称和元数据的字段
///
/// # 编码和长度
///
/// - [`Encoding`]: 数据编码方式（BCD、Binary、ASCII、Time 等）
/// - [`FieldLength`]: 字段长度定义（固定、引用、表达式）
/// - [`ExternalLength`]: 外部协议长度定义
///
/// # 辅助类型
///
/// - [`TimeEncoding`]: 时间字段的编码方式
/// - [`Endian`]: 字节序（大端/小端）
/// - [`FormatSpec`]: 格式化显示规格
/// - [`FormatType`]: 格式化类型（Hex/BCD/Bin）
/// - [`FormatOrder`]: 格式化字节序
/// - [`BitSpec`]: 位域规格
///
/// # 使用示例
///
/// ```rust
/// use spec_compiler::{FieldSpec, Encoding, FieldLength, Endian};
///
/// let spec = FieldSpec::Fixed {
///     encoding: Encoding::Bin {
///         endian: Endian::Little,
///         signed: false,
///         decimals: 0,
///     },
///     length: FieldLength::Fixed(4),
///     unit: Some("W".to_string()),
///     enum_map: None,
///     format: None,
/// };
/// ```
pub use types::{
    // 核心类型
    FieldSpec,
    NamedField,
    Encoding,
    FieldLength,
    
    // 辅助类型
    TimeEncoding,
    Endian,
    FormatSpec,
    FormatType,
    FormatOrder,
    BitSpec,
    ExternalLength,
};

/// 常用错误类型
///
/// - [`CompilerError`]: 编译过程中的所有错误类型
/// - [`LoadError`]: 文件加载相关的错误
///
/// # 示例
///
/// ```rust,ignore
/// use spec_compiler::{Compiler, CompilerError};
///
/// let compiler = Compiler::new();
/// match compiler.compile_schema_dir("schema") {
///     Ok(table) => println!("成功编译 {} 条目", table.len()),
///     Err(message) => eprintln!("其他错误: {}", message),
/// }
/// ```
pub use error::{CompilerError, LoadError};