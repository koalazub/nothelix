mod cells;
mod convert;
mod embed;
mod export;
mod ffi;
mod ipynb;
mod marker;
#[cfg(test)]
mod roundtrip;
mod scan;

pub use convert::{convert_to_ipynb, notebook_convert_sync};
pub use export::{export_to_markdown, export_to_typst};
pub use ffi::{
    get_cell_at_line, get_cell_code_from_jl, list_jl_code_cells, notebook_cell_count,
    notebook_get_cell_code, notebook_validate,
};
pub use scan::{ScanCell, scan_code_cells, scan_variable_definition};

#[cfg(test)]
mod fixture {
    pub(super) fn path(name: &str) -> String {
        format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
    }
}
