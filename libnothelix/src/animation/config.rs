use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct AnimationConfig {
    pub enabled: bool,
    pub max_fps: u32,
    pub decode_cache_mb: u32,
    pub max_dimensions: [u32; 2],
    pub max_duration_seconds: u32,
    pub preferred_renderer: String,
    pub first_run_hint: bool,
    pub show_indicator: bool,
    pub pause_on_focus_lost: bool,
    pub pause_off_viewport: bool,
    pub formats: AnimationFormats,
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_fps: 60,
            decode_cache_mb: 64,
            max_dimensions: [3840, 2160],
            max_duration_seconds: 600,
            preferred_renderer: "auto".to_string(),
            first_run_hint: true,
            show_indicator: true,
            pause_on_focus_lost: true,
            pause_off_viewport: true,
            formats: AnimationFormats::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct AnimationFormats {
    pub gif: bool,
    pub apng: bool,
    pub webp: bool,
    pub mp4: bool,
    pub webm: bool,
    pub lottie: bool,
}

impl Default for AnimationFormats {
    fn default() -> Self {
        Self {
            gif: true,
            apng: true,
            webp: true,
            mp4: true,
            webm: true,
            lottie: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_partial_toml_keeps_defaults() {
        let toml_str = r#"
            enabled = false
            max_fps = 144
            [formats]
            mp4 = false
        "#;
        let config: AnimationConfig = toml::from_str(toml_str).unwrap();
        assert!(!config.enabled);
        assert_eq!(config.max_fps, 144);
        assert!(!config.formats.mp4);
        assert!(config.formats.gif);
        assert_eq!(config.decode_cache_mb, 64);
    }
}
