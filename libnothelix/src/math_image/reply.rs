use crate::error::Error;

#[derive(Clone)]
pub(crate) struct RenderedSvg {
    pub(crate) b64: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl RenderedSvg {
    pub(crate) fn to_json(&self) -> String {
        math_json(&self.b64, self.width, self.height, "")
    }
}

pub(crate) fn failure_json(error: &Error) -> String {
    math_json("", 0, 0, &error.to_string())
}

pub(crate) fn math_json(b64: &str, width: u32, height: u32, error: &str) -> String {
    format!(
        "{{\"b64\":\"{b64}\",\"width\":{width},\"height\":{height},\"error\":\"{}\"}}",
        json_string_body(error)
    )
}

fn json_string_body(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{RenderedSvg, math_json};

    #[test]
    fn a_successful_render_reports_an_empty_error_field() {
        let json = RenderedSvg {
            b64: "PHN2Zy".to_string(),
            width: 120,
            height: 40,
        }
        .to_json();
        assert_eq!(
            json,
            r#"{"b64":"PHN2Zy","width":120,"height":40,"error":""}"#
        );
    }

    #[test]
    fn a_failure_message_stays_parseable_json() {
        let json = math_json("", 0, 0, r#"latex conversion failed for `\frac{"a"}{2}`"#);
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(
            parsed["error"].as_str().expect("error field"),
            r#"latex conversion failed for `\frac{"a"}{2}`"#
        );
    }
}
