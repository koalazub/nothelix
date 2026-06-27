//! Guards against the manifest corruption that poisons the kernel's default
//! Julia environment.
//!
//! NothelixMacros shipped with the placeholder UUID
//! `a1b2c3d4-e5f6-7890-abcd-ef1234567890`, which collides with an unrelated
//! package registered in General. A `Pkg.add("NothelixMacros")` (or any resolve
//! that consults the registry) then pulls that foreign package into the active
//! environment's manifest without instantiating it, and every subsequent
//! `using` fails with "required but does not seem to be installed". The fix is a
//! real, random v4 UUID. These tests fail if the placeholder ever returns or if
//! the package's UUID drifts out of sync with the env that references it.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("libnothelix has a parent directory")
        .to_path_buf()
}

fn read_toml(rel: &str) -> toml::Value {
    let path = repo_root().join(rel);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    toml::from_str(&text).unwrap_or_else(|e| panic!("cannot parse {}: {e}", path.display()))
}

fn package_uuid() -> String {
    read_toml("lsp/NothelixMacros/Project.toml")
        .get("uuid")
        .and_then(toml::Value::as_str)
        .expect("NothelixMacros/Project.toml has a uuid")
        .to_string()
}

/// True for a canonical RFC-4122 version-4 UUID (8-4-4-4-12 lowercase hex with
/// version nibble `4` and variant nibble in `8..=b`). Sequential placeholders
/// like `a1b2c3d4-...-7890-abcd-...` fail because their version nibble is not 4.
fn is_random_v4_uuid(s: &str) -> bool {
    let groups: Vec<&str> = s.split('-').collect();
    if groups.len() != 5 {
        return false;
    }
    let lengths = [8, 4, 4, 4, 12];
    for (g, want) in groups.iter().zip(lengths) {
        if g.len() != want || !g.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
            return false;
        }
    }
    let version = groups[2].as_bytes()[0];
    let variant = groups[3].as_bytes()[0];
    version == b'4' && matches!(variant, b'8' | b'9' | b'a' | b'b')
}

#[test]
fn nothelix_macros_uuid_is_not_the_placeholder() {
    let uuid = package_uuid();
    assert_ne!(
        uuid, "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
        "NothelixMacros must not use the placeholder UUID — it collides with a \
         General-registry package and poisons the kernel's default env on Pkg.add"
    );
}

#[test]
fn nothelix_macros_uuid_is_a_random_v4() {
    let uuid = package_uuid();
    assert!(
        is_random_v4_uuid(&uuid),
        "NothelixMacros UUID {uuid} is not a canonical random v4 UUID; a \
         non-random/sequential UUID risks colliding with a registered package"
    );
}

#[test]
fn lsp_env_references_the_package_uuid() {
    let env = read_toml("lsp/Project.toml");
    let referenced = env
        .get("deps")
        .and_then(|d| d.get("NothelixMacros"))
        .and_then(toml::Value::as_str)
        .expect("lsp/Project.toml [deps] references NothelixMacros");
    assert_eq!(
        referenced,
        package_uuid(),
        "lsp/Project.toml references a different NothelixMacros UUID than the \
         package declares — Pkg will fail to resolve the develop'd path"
    );
}
