//! Verifies libnothelix's compile-time BUILD_ID contract. CI builds
//! stamp "ci-<yyyymmdd>-<short-git-sha>"; dev builds stamp the constant
//! "dev" so commits never force a rebuild.

#[test]
fn build_id_matches_the_contract() {
    let id = nothelix::build_id();
    assert!(
        id == "dev" || id.starts_with("ci-"),
        "build_id() must be 'dev' or start with 'ci-', got: {id}"
    );
    if let Some(rest) = id.strip_prefix("ci-") {
        assert!(rest.len() >= 8, "ci id must carry date and sha: {id}");
    }
}
