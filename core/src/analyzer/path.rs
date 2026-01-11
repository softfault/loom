use crate::ast::UseAnchor;
use crate::context::Context;
use std::path::{Path, PathBuf};

/// 将 AST 中的 Use 路径转换为文件系统路径
/// 强制 Modern Style: 模块必须对应一个 .lm 文件
pub fn resolve_module_path(
    ctx: &Context,
    anchor: &UseAnchor,
    path_segments: &[String],
    current_file_dir: &Path,
) -> Option<PathBuf> {
    // 1. 确定基准目录
    let mut target_path = match anchor {
        UseAnchor::Root => ctx.root_dir.clone(),
        UseAnchor::Current => current_file_dir.to_path_buf(),
        UseAnchor::Parent => current_file_dir.parent()?.to_path_buf(),
    };

    // 2. 拼接路径片段
    // e.g. use utils.math -> path/to/root/utils/math
    for segment in path_segments {
        target_path.push(segment);
    }

    // 3. 加上扩展名 .lm
    // e.g. path/to/root/utils/math.lm
    target_path.set_extension("lm");

    // 4. 检查是否存在
    // 只有当它是一个真实存在的文件时才返回
    if target_path.exists() && target_path.is_file() {
        Some(target_path)
    } else {
        None
    }
}
