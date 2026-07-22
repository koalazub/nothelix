pub(super) fn combining_mark(name: &str) -> Option<&'static str> {
    match name {
        "tilde" | "widetilde" => Some("\u{0303}"),
        "bar" | "overline" => Some("\u{0304}"),
        "hat" | "widehat" => Some("\u{0302}"),
        "vec" => Some("\u{20D7}"),
        "dot" => Some("\u{0307}"),
        "ddot" => Some("\u{0308}"),
        "mathring" => Some("\u{030A}"),
        _ => None,
    }
}

pub(super) fn spacing_glyph(byte: u8) -> Option<&'static str> {
    match byte {
        b'|' => Some("‖"),
        b'{' => Some("{"),
        b'}' => Some("}"),
        b',' => Some("\u{2006}"),
        b';' => Some("\u{2005}"),
        b'!' => Some("\u{200B}"),
        b' ' => Some(" "),
        _ => None,
    }
}
