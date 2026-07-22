use super::font::is_font_command;

static SUPER_MAP: &[(&str, &str)] = &[
    ("0", "⁰"),
    ("1", "¹"),
    ("2", "²"),
    ("3", "³"),
    ("4", "⁴"),
    ("5", "⁵"),
    ("6", "⁶"),
    ("7", "⁷"),
    ("8", "⁸"),
    ("9", "⁹"),
    ("+", "⁺"),
    ("-", "⁻"),
    ("=", "⁼"),
    ("(", "⁽"),
    (")", "⁾"),
    ("n", "ⁿ"),
    ("i", "ⁱ"),
    ("T", "ᵀ"),
    ("*", "*"),
    ("a", "ᵃ"),
    ("b", "ᵇ"),
    ("c", "ᶜ"),
    ("d", "ᵈ"),
    ("e", "ᵉ"),
    ("g", "ᵍ"),
    ("h", "ʰ"),
    ("j", "ʲ"),
    ("k", "ᵏ"),
    ("l", "ˡ"),
    ("m", "ᵐ"),
    ("o", "ᵒ"),
    ("p", "ᵖ"),
    ("r", "ʳ"),
    ("s", "ˢ"),
    ("t", "ᵗ"),
    ("u", "ᵘ"),
    ("v", "ᵛ"),
    ("w", "ʷ"),
    ("x", "ˣ"),
    ("y", "ʸ"),
    ("z", "ᶻ"),
    ("A", "ᴬ"),
    ("B", "ᴮ"),
    ("D", "ᴰ"),
    ("E", "ᴱ"),
    ("G", "ᴳ"),
    ("H", "ᴴ"),
    ("I", "ᴵ"),
    ("J", "ᴶ"),
    ("K", "ᴷ"),
    ("L", "ᴸ"),
    ("M", "ᴹ"),
    ("N", "ᴺ"),
    ("O", "ᴼ"),
    ("P", "ᴾ"),
    ("R", "ᴿ"),
    ("U", "ᵁ"),
    ("V", "ⱽ"),
    ("W", "ᵂ"),
];

static SUB_MAP: &[(&str, &str)] = &[
    ("0", "₀"),
    ("1", "₁"),
    ("2", "₂"),
    ("3", "₃"),
    ("4", "₄"),
    ("5", "₅"),
    ("6", "₆"),
    ("7", "₇"),
    ("8", "₈"),
    ("9", "₉"),
    ("+", "₊"),
    ("-", "₋"),
    ("=", "₌"),
    ("(", "₍"),
    (")", "₎"),
    ("n", "ₙ"),
    ("i", "ᵢ"),
    ("k", "ₖ"),
    ("j", "ⱼ"),
    ("e", "ₑ"),
    ("a", "ₐ"),
    ("o", "ₒ"),
    ("x", "ₓ"),
    ("r", "ᵣ"),
    ("u", "ᵤ"),
    ("v", "ᵥ"),
    ("s", "ₛ"),
    ("t", "ₜ"),
    ("l", "ₗ"),
    ("m", "ₘ"),
    ("p", "ₚ"),
    ("h", "ₕ"),
];

type AsciiLut = [Option<&'static str>; 128];

const fn build_lut(map: &[(&'static str, &'static str)]) -> AsciiLut {
    let mut lut: AsciiLut = [None; 128];
    let mut i = 0;
    while i < map.len() {
        let key = map[i].0.as_bytes();
        assert!(key.len() == 1, "sub/super map key must be one ASCII byte");
        lut[key[0] as usize] = Some(map[i].1);
        i += 1;
    }
    lut
}

static SUPER_LUT: AsciiLut = build_lut(SUPER_MAP);
static SUB_LUT: AsciiLut = build_lut(SUB_MAP);

#[inline]
fn lut_lookup(lut: &AsciiLut, ch: char) -> Option<&'static str> {
    let b = ch as u32;
    if b < 128 { lut[b as usize] } else { None }
}

#[inline]
fn super_lookup(ch: char) -> Option<&'static str> {
    lut_lookup(&SUPER_LUT, ch)
}

#[inline]
fn sub_lookup(ch: char) -> Option<&'static str> {
    lut_lookup(&SUB_LUT, ch)
}

static SUPER_COMMAND_MAP: &[(&str, &str)] = &[
    ("ast", "∗"),
    ("bullet", "∙"),
    ("circ", "°"),
    ("dag", "†"),
    ("dagger", "†"),
    ("ddag", "‡"),
    ("ddagger", "‡"),
    ("intercal", "ᵀ"),
    ("perp", "⊥"),
    ("prime", "′"),
    ("star", "⋆"),
    ("top", "ᵀ"),
    ("transpose", "ᵀ"),
];

static SUB_COMMAND_MAP: &[(&str, &str)] = &[
    ("ast", "∗"),
    ("bullet", "∙"),
    ("circ", "∘"),
    ("dagger", "†"),
    ("intercal", "ᵀ"),
    ("parallel", "∥"),
    ("perp", "⊥"),
    ("prime", "′"),
    ("star", "⋆"),
    ("top", "ᵀ"),
];

fn command_map_lookup(map: &[(&str, &'static str)], name: &str) -> Option<&'static str> {
    map.iter().find(|(k, _)| *k == name).map(|&(_, v)| v)
}

#[inline]
fn super_command_lookup(name: &str) -> Option<&'static str> {
    command_map_lookup(SUPER_COMMAND_MAP, name)
}

#[inline]
fn sub_command_lookup(name: &str) -> Option<&'static str> {
    command_map_lookup(SUB_COMMAND_MAP, name)
}

fn braced_command(
    content: &str,
    cmd_lookup: fn(&str) -> Option<&'static str>,
    char_lookup: fn(char) -> Option<&'static str>,
) -> Option<&'static str> {
    let rest = content.trim().strip_prefix('\\')?;
    let name_len = rest.chars().take_while(char::is_ascii_alphabetic).count();
    let (name, after) = rest.split_at(name_len);
    if after.is_empty() {
        return cmd_lookup(name);
    }
    if is_font_command(name) && after.starts_with('{') && after.ends_with('}') {
        let inner = &after[1..after.len() - 1];
        let mut it = inner.chars();
        if let (Some(ch), None) = (it.next(), it.next()) {
            return char_lookup(ch);
        }
    }
    None
}

#[inline]
fn braced_super_command(content: &str) -> Option<&'static str> {
    braced_command(content, super_command_lookup, super_lookup)
}

#[inline]
fn braced_sub_command(content: &str) -> Option<&'static str> {
    braced_command(content, sub_command_lookup, sub_lookup)
}

#[derive(Clone, Copy)]
pub(super) struct Script {
    glyph: fn(char) -> Option<&'static str>,
    command: fn(&str) -> Option<&'static str>,
    braced_command: fn(&str) -> Option<&'static str>,
}

const SUPERSCRIPT: Script = Script {
    glyph: super_lookup,
    command: super_command_lookup,
    braced_command: braced_super_command,
};

const SUBSCRIPT: Script = Script {
    glyph: sub_lookup,
    command: sub_command_lookup,
    braced_command: braced_sub_command,
};

impl Script {
    pub(super) fn marked_by(byte: u8) -> Option<Self> {
        match byte {
            b'^' => Some(SUPERSCRIPT),
            b'_' => Some(SUBSCRIPT),
            _ => None,
        }
    }

    pub(super) fn of_char(self, ch: char) -> Option<&'static str> {
        (self.glyph)(ch)
    }

    pub(super) fn of_command(self, name: &str) -> Option<&'static str> {
        (self.command)(name)
    }

    pub(super) fn of_braced_command(self, content: &str) -> Option<&'static str> {
        (self.braced_command)(content)
    }
}
