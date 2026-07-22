use std::fs;
use std::path::PathBuf;

use nothelix::gallery;

fn main() {
    if let Err(err) = generate() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn generate() -> Result<(), String> {
    let out_dir = std::env::args()
        .nth(1)
        .map_or_else(default_out_dir, PathBuf::from);
    fs::create_dir_all(&out_dir)
        .map_err(|e| format!("gen-gallery: cannot create {}: {e}", out_dir.display()))?;

    let cases = gallery::document_cases()?;
    for case in &cases {
        let path = out_dir.join(format!("{}.txt", case.document_name()));
        fs::write(&path, &case.output)
            .map_err(|e| format!("gen-gallery: cannot write {}: {e}", path.display()))?;
        println!("{}", path.display());
    }
    println!("gen-gallery: {} artifacts", cases.len());
    Ok(())
}

fn default_out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/_includes/engine")
}
