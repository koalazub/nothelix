mod fields;
mod images;
mod text_plots;

use serde_json::Value;

pub use fields::{json_get, json_get_bool, json_get_many, json_get_plot_data};
pub use images::{json_get_all_images, json_get_animated_mime, json_get_first_image_bytes};
pub use text_plots::json_get_text_plots;

fn document(json: &str) -> Option<Value> {
    serde_json::from_str(json).ok()
}
