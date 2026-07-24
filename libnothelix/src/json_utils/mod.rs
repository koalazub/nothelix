mod audio;
mod fields;
mod images;
mod text_plots;
mod widgets;

use crate::error::{Error, Result};
use serde_json::Value;

pub use audio::json_get_audio;
pub use fields::{
    json_get, json_get_bool, json_get_cell_states, json_get_many, json_get_notes,
    json_get_plot_data,
};
pub use images::{json_get_all_images, json_get_animated_mime, json_get_first_image_bytes};
pub use text_plots::json_get_text_plots;
pub(crate) use text_plots::{SECTION_SEP, SPAN_SEP};
pub use widgets::json_get_widgets;

fn document(subject: &'static str, json: &str) -> Result<Value> {
    serde_json::from_str(json).map_err(|source| Error::Json { subject, source })
}
