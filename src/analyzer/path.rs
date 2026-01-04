use crate::ast::UseAnchor;
use crate::context::Context;
use std::path::{Path, PathBuf};

/// 将 AST 中的 Use 路径转换为文件系统路径
/// import_path: ["utils", "math"]
/// current_file_dir: 当前正在解析的文件所在的目录
pub fn resolve_module_path(
    ctx: &Context,
    anchor: &UseAnchor,
    path_segments: &[String],
    current_file_dir: &Path,
) -> Option<PathBuf> {
    let mut base_path = match anchor {
        UseAnchor::Root => ctx.root_dir.clone(),
        UseAnchor::Current => current_file_dir.to_path_buf(),
        UseAnchor::Parent => current_file_dir.parent()?.to_path_buf(),
    };

    // 拼接路径片段
    for segment in path_segments {
        base_path.push(segment);
    }

    // Loom 约定：模块通常以 .lm 结尾
    // 先尝试直接拼接 .lm
    let mut file_path = base_path.clone();
    file_path.set_extension("lm");

    if file_path.exists() {
        return Some(file_path);
    }

    // 也可以支持包目录风格： use utils -> utils/mod.lm (类似于 Rust)
    let mut mod_path = base_path.clone();
    mod_path.push("mod.lm");
    if mod_path.exists() {
        return Some(mod_path);
    }

    None
}
