mod char_offsets;
mod conceal;
mod cursor;
mod environment;
mod escape;
mod font;
pub(crate) mod math_regions;
mod math_spans;
mod operators;
mod overlay;
mod scanner;
mod script;
mod symbol_table;
pub(crate) mod typst_conceal;

pub use conceal::{compute_conceal_overlays, compute_conceal_overlays_for_comments_with_options};
pub use math_spans::parse_math_spans_json;
pub use scanner::{latex_overlays, latex_overlays_with_options};
pub use symbol_table::{unicode_completions_for_prefix, unicode_lookup};
