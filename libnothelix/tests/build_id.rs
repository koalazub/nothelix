//! Verifies that libnothelix's compile-time BUILD_ID is exposed and
//! non-empty. The build id format is "ci-<yyyymmdd>-<short-git-sha>"
//! in CI and "dev-<short-git-sha>-dirty" for local dev builds.

#[test]
fn build_id_is_non_empty() {
    let id = nothelix::build_id();
    assert!(!id.is_empty(), "build_id() must not be empty");
    assert!(id.len() >= 8, "build_id() must be at least 8 chars: {id}");
}

#[test]
fn build_id_starts_with_known_prefix() {
    let id = nothelix::build_id();
    assert!(
        id.starts_with("ci-") || id.starts_with("dev-"),
        "build_id() must start with 'ci-' or 'dev-', got: {id}"
    );
}
