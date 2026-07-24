use crate::error::{Error, Result};
use std::fs;
use std::path::PathBuf;

include!(concat!(env!("OUT_DIR"), "/kernel_sources.rs"));

pub(super) fn install_runner() -> Result<PathBuf> {
    let dir = std::env::var("HOME")
        .map_or_else(|_| PathBuf::from("/tmp"), PathBuf::from)
        .join(".local/share/nothelix/kernel");
    fs::create_dir_all(&dir).map_err(|e| Error::creating(&dir, e))?;
    for &(name, source) in SOURCES {
        let path = dir.join(name);
        fs::write(&path, source).map_err(|e| Error::writing(path, e))?;
    }
    Ok(dir.join("runner.jl"))
}

#[cfg(test)]
mod tests {
    use super::SOURCES;

    #[test]
    fn embeds_every_non_test_kernel_source_on_disk() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../kernel");
        let mut on_disk: Vec<String> = std::fs::read_dir(dir)
            .expect("kernel/ directory")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".jl") && !name.ends_with("_test.jl"))
            .collect();
        on_disk.sort();
        let mut embedded: Vec<String> = SOURCES.iter().map(|&(name, _)| name.to_string()).collect();
        embedded.sort();
        assert_eq!(on_disk, embedded);
    }
}
