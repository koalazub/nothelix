//! Structured-error data types exchanged with the Julia kernel.
//!
//! The kernel emits errors in two shapes: a fully structured JSON
//! payload (`StructuredError`) and a plain stderr blob. Both flow into
//! the same formatter, but the structured form carries cross-cell
//! context (`VarContext`, `MethodCandidate`, `ScopeVarEntry`) that the
//! formatter uses to enrich the rendered message.

use std::collections::HashMap;

use serde::Deserialize;

#[derive(Deserialize, Default)]
pub struct StructuredError {
    #[serde(default)]
    pub error_type: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub frames: Vec<ErrorFrame>,
    #[serde(default)]
    pub source_line: String,
    #[serde(default)]
    pub cell_index: i64,
    #[serde(default)]
    pub cell_line: i64,
    #[serde(default)]
    pub cell_context: HashMap<String, VarContext>,
    #[serde(default)]
    pub unexecuted_deps: Vec<i64>,
    /// Runtime type → list of in-scope variables currently of that type.
    /// Populated by the kernel on `MethodError`. Empty when the kernel
    /// isn't running or nothing has been executed yet.
    #[serde(default)]
    pub in_scope_variable_types: HashMap<String, Vec<ScopeVarEntry>>,
    /// In-scope values the failing `MethodError`'s function *does* have
    /// a method for. Populated for single-arg `MethodErrors`.
    #[serde(default)]
    pub method_candidates: Vec<MethodCandidate>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ScopeVarEntry {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub cell: i64,
}

#[derive(Debug, Deserialize, Default)]
pub struct MethodCandidate {
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "type")]
    pub type_name: String,
    #[serde(default)]
    pub cell: i64,
}

/// Where the formatter learned about a variable's defining cell. Each
/// variant carries exactly the fields meaningful for its provenance.
/// Serialized form uses a `source` tag — kernel must emit one of:
///   {"`source":"executed","defined_in_cell":N,"status":"done`"}
///   {"`source":"pending_registered","defined_in_cell":N`}
///   {"`source":"static_source","defined_in_cell":N,"line_in_cell":L,"line_text"`:"…"}
#[derive(Debug, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum VarContext {
    /// Cell ran (success or error) — kernel `VARIABLE_SOURCES` had the
    /// binding. `status` is `"done"` or `"error"`.
    Executed {
        defined_in_cell: i64,
        status: String,
    },
    /// Cell is in the kernel's `CELLS` registry (source parsed by
    /// `@cell`) but hasn't executed — AST says it would define the var.
    PendingRegistered {
        defined_in_cell: i64,
    },
    /// Static `.jl` scan found an assignment in a cell the kernel hasn't
    /// seen yet. Carries the exact line for user navigation.
    StaticSource {
        defined_in_cell: i64,
        line_in_cell: i64,
        line_text: String,
    },
}

impl VarContext {
    pub fn defined_in_cell(&self) -> i64 {
        match self {
            Self::Executed { defined_in_cell, .. }
            | Self::PendingRegistered { defined_in_cell }
            | Self::StaticSource { defined_in_cell, .. } => *defined_in_cell,
        }
    }
}

#[derive(Deserialize, Default)]
pub struct ErrorFrame {
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub line: i64,
    #[serde(default)]
    pub func: String,
    #[serde(default)]
    pub is_user_code: bool,
}
