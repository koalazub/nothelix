use std::fs::File;
use std::path::PathBuf;

static SYMBOLSERVER_PATCH: &str = include_str!("../../lsp/symbolserver-1.14.patch");
static LSP_BOOTSTRAP_JL: &str = include_str!("../../lsp/bootstrap.jl");

const LSP_PATCH_MARKER: &str = "val === vr ? nothing";

fn nothelix_data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("nothelix")
}

pub fn lsp_project_dir() -> String {
    nothelix_data_dir()
        .join("lsp")
        .to_string_lossy()
        .into_owned()
}

pub fn lsp_depot_dir() -> String {
    nothelix_data_dir()
        .join("lsp")
        .join("depot")
        .to_string_lossy()
        .into_owned()
}

fn lsp_dev_patched(dir: &std::path::Path) -> bool {
    let utils = dir
        .join("dev")
        .join("SymbolServer")
        .join("src")
        .join("utils.jl");
    std::fs::read_to_string(utils)
        .map(|s| s.contains(LSP_PATCH_MARKER))
        .unwrap_or(false)
}

pub fn lsp_environment_ready() -> String {
    let dir = nothelix_data_dir().join("lsp");
    let manifest = dir.join("Manifest.toml");
    let depot_packages = dir.join("depot").join("packages");
    if manifest.exists() && depot_packages.exists() && lsp_dev_patched(&dir) {
        "yes".into()
    } else {
        "no".into()
    }
}

pub fn ensure_lsp_environment() -> String {
    let dir = nothelix_data_dir().join("lsp");
    let project_toml = dir.join("Project.toml");
    let manifest_toml = dir.join("Manifest.toml");
    let depot_packages = dir.join("depot").join("packages");

    if project_toml.exists()
        && manifest_toml.exists()
        && depot_packages.exists()
        && lsp_dev_patched(&dir)
    {
        return String::new();
    }

    if let Err(e) = std::fs::create_dir_all(&dir) {
        return format!("ERROR: Cannot create LSP directory: {e}");
    }

    let project_content = "[deps]\nLanguageServer = \"2b0e0bc5-e4fd-59b4-8912-456d1b03d8d7\"\n";
    if let Err(e) = std::fs::write(&project_toml, project_content) {
        return format!("ERROR: Cannot write Project.toml: {e}");
    }

    let patch_path = dir.join("symbolserver-1.14.patch");
    if let Err(e) = std::fs::write(&patch_path, SYMBOLSERVER_PATCH) {
        return format!("ERROR: Cannot write SymbolServer patch: {e}");
    }
    let bootstrap_path = dir.join("bootstrap.jl");
    if let Err(e) = std::fs::write(&bootstrap_path, LSP_BOOTSTRAP_JL) {
        return format!("ERROR: Cannot write LSP bootstrap: {e}");
    }

    let depot = dir.join("depot");
    if let Err(e) = std::fs::create_dir_all(&depot) {
        return format!("ERROR: Cannot create depot directory: {e}");
    }

    let project_path = dir.to_string_lossy().into_owned();
    let depot_path = depot.to_string_lossy().into_owned();
    let macros_path = dir.join("NothelixMacros").to_string_lossy().into_owned();
    let log_path = dir.join("setup.log").to_string_lossy().into_owned();

    match std::process::Command::new("julia")
        .args([
            "--startup-file=no",
            "--history-file=no",
            "--quiet",
            &format!("--project={project_path}"),
            &bootstrap_path.to_string_lossy(),
            &project_path,
            &patch_path.to_string_lossy(),
            &macros_path,
        ])
        .env("JULIA_DEPOT_PATH", &depot_path)
        .stdout(File::create(&log_path).unwrap_or_else(|_| File::create("/dev/null").unwrap()))
        .stderr(File::create(&log_path).unwrap_or_else(|_| File::create("/dev/null").unwrap()))
        .spawn()
    {
        Ok(_) => String::new(),
        Err(e) => format!("ERROR: Cannot spawn julia: {e}"),
    }
}
