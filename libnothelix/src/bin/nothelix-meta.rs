//! Tiny CLI that prints libnothelix's compile-time metadata as
//! key=value lines. Used by CI to generate the `libnothelix.meta`
//! sidecar, and by `nothelix doctor` to verify build consistency.
//!
//! Output format:
//!   BUILD_ID=<build-id-from-libnothelix>
//!   LIBNOTHELIX_VERSION=<cargo pkg version>
//!
//! CI writes this output to `libnothelix.meta` next to the dylib in
//! the release tarball; the install script copies it to
//! $STEEL_HOME/native/libnothelix.meta.

fn main() {
    println!("BUILD_ID={}", nothelix::build_id());
    println!("LIBNOTHELIX_VERSION={}", env!("CARGO_PKG_VERSION"));
}
