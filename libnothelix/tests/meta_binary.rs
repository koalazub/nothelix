//! Smoke test for the nothelix-meta helper. Invokes the compiled
//! binary and checks the output shape. Relies on cargo's ability to
//! locate the binary via env!("CARGO_BIN_EXE_nothelix-meta").

#[test]
fn meta_binary_prints_build_id() {
    let bin = env!("CARGO_BIN_EXE_nothelix-meta");
    let output = std::process::Command::new(bin)
        .output()
        .expect("run nothelix-meta");
    assert!(
        output.status.success(),
        "nothelix-meta failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("BUILD_ID="),
        "stdout must contain BUILD_ID= line, got: {stdout}"
    );
    assert!(
        stdout.contains("LIBNOTHELIX_VERSION="),
        "stdout must contain LIBNOTHELIX_VERSION= line, got: {stdout}"
    );
}
