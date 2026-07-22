use crate::error::{Error, Result};
use std::fs;
use std::path::PathBuf;

const SOURCES: [(&str, &str); 5] = [
    ("runner.jl", include_str!("../../../kernel/runner.jl")),
    (
        "cell_registry.jl",
        include_str!("../../../kernel/cell_registry.jl"),
    ),
    (
        "ast_analysis.jl",
        include_str!("../../../kernel/ast_analysis.jl"),
    ),
    (
        "output_capture.jl",
        include_str!("../../../kernel/output_capture.jl"),
    ),
    (
        "cell_macros.jl",
        include_str!("../../../kernel/cell_macros.jl"),
    ),
];

pub(super) fn install_runner() -> Result<PathBuf> {
    let dir = std::env::var("HOME")
        .map_or_else(|_| PathBuf::from("/tmp"), PathBuf::from)
        .join(".local/share/nothelix/kernel");
    fs::create_dir_all(&dir).map_err(|e| Error::creating(&dir, e))?;
    for (name, source) in SOURCES {
        let path = dir.join(name);
        fs::write(&path, source).map_err(|e| Error::writing(path, e))?;
    }
    Ok(dir.join("runner.jl"))
}
