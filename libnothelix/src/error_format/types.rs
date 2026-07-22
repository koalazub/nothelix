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
    #[serde(default)]
    pub in_scope_variable_types: HashMap<String, Vec<ScopeVarEntry>>,
    #[serde(default)]
    pub method_candidates: Vec<MethodCandidate>,
    #[serde(skip)]
    pub undef_symbols: Vec<String>,
    #[serde(skip)]
    pub undef_guidance: Vec<String>,
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

#[derive(Debug, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum VarContext {
    Executed {
        defined_in_cell: i64,
        status: String,
    },
    PendingRegistered {
        defined_in_cell: i64,
    },
    StaticSource {
        defined_in_cell: i64,
        line_in_cell: i64,
        line_text: String,
    },
}

impl VarContext {
    pub fn defined_in_cell(&self) -> i64 {
        match self {
            Self::Executed {
                defined_in_cell, ..
            }
            | Self::PendingRegistered { defined_in_cell }
            | Self::StaticSource {
                defined_in_cell, ..
            } => *defined_in_cell,
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
