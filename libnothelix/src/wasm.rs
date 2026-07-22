use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn conceal_overlays(text: String) -> String {
    crate::unicode::compute_conceal_overlays(text)
}

#[wasm_bindgen]
pub fn unicode_completions(prefix: String) -> String {
    crate::unicode::unicode_completions_for_prefix(prefix)
}

#[wasm_bindgen]
pub fn unicode_lookup(name: String) -> String {
    crate::unicode::unicode_lookup(name)
}

#[wasm_bindgen]
pub fn markdown_to_typst(markdown: String) -> String {
    match crate::typst_export::md_to_typst(&markdown) {
        Ok(typst) => typst,
        Err(e) => format!("// export error: {e}"),
    }
}

#[wasm_bindgen]
pub fn format_julia_error(error_json: String, raw_error: String) -> String {
    crate::error_format::format_error(&crate::error_format::FormatContext {
        error_json: &error_json,
        raw_error: &raw_error,
        notebook_path: None,
    })
}
