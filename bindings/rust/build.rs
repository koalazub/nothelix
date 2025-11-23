use std::path::PathBuf;

fn main() {
    let src_dir = PathBuf::from("../../src");

    let mut c_config = cc::Build::new();
    c_config.include(&src_dir);
    c_config
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .flag_if_supported("-Wno-trigraphs");

    let parser_path = src_dir.join("parser.c");
    c_config.file(&parser_path);

    // If there's a scanner.c file, include it
    let scanner_path = src_dir.join("scanner.c");
    if scanner_path.exists() {
        c_config.file(&scanner_path);
    }

    c_config.compile("parser");

    println!("cargo:rerun-if-changed=../../src/parser.c");
    println!("cargo:rerun-if-changed=../../src/scanner.c");
}
