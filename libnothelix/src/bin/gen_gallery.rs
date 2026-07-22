use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use nothelix::gallery;

fn main() {
    if let Err(err) = generate() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn generate() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let out_dir = args.next().map_or_else(default_out_dir, PathBuf::from);
    let readme = args.next().map_or_else(default_readme, PathBuf::from);

    fs::create_dir_all(&out_dir)
        .map_err(|e| format!("gen-gallery: cannot create {}: {e}", out_dir.display()))?;

    let cases = gallery::document_cases()?;
    let mut artifacts = HashMap::new();
    for case in &cases {
        let name = case.document_name();
        let path = out_dir.join(format!("{name}.txt"));
        fs::write(&path, &case.output)
            .map_err(|e| format!("gen-gallery: cannot write {}: {e}", path.display()))?;
        artifacts.insert(name, case.output.clone());
        println!("{}", path.display());
    }

    inject(&readme, &artifacts)?;
    println!("gen-gallery: {} artifacts", cases.len());
    Ok(())
}

fn inject(readme: &Path, artifacts: &HashMap<String, String>) -> Result<(), String> {
    let before = fs::read_to_string(readme)
        .map_err(|e| format!("gen-gallery: cannot read {}: {e}", readme.display()))?;
    let after = gallery::inject_regions(&before, artifacts)
        .map_err(|e| format!("{e} (in {})", readme.display()))?;
    if after == before {
        println!("{} unchanged", readme.display());
        return Ok(());
    }
    fs::write(readme, after)
        .map_err(|e| format!("gen-gallery: cannot write {}: {e}", readme.display()))?;
    println!("{}", readme.display());
    Ok(())
}

fn default_out_dir() -> PathBuf {
    repo_root().join("docs/_includes/engine")
}

fn default_readme() -> PathBuf {
    repo_root().join("README.md")
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}
