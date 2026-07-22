use super::symbol_table::julia_repl_name;

pub(super) fn is_font_command(name: &str) -> bool {
    matches!(
        name,
        "mathsf"
            | "mathrm"
            | "text"
            | "mathtt"
            | "mathbf"
            | "mathit"
            | "mathbb"
            | "mathcal"
            | "boldsymbol"
    )
}

fn spelled_digit(s: &str) -> &str {
    match s {
        "0" => "zero",
        "1" => "one",
        "2" => "two",
        "3" => "three",
        "4" => "four",
        "5" => "five",
        "6" => "six",
        "7" => "seven",
        "8" => "eight",
        "9" => "nine",
        _ => s,
    }
}

pub(super) fn latex_font_to_julia(cmd: &str, letter: &str) -> Option<&'static str> {
    let prefix = match cmd {
        "mathbf" | "textbf" | "boldsymbol" => "bf",
        "mathbb" => "bb",
        "mathcal" | "cal" => "scr",
        "mathfrak" | "frak" => "frak",
        "mathit" | "textit" => "it",
        "mathsf" => "sf",
        "mathtt" => "tt",
        _ => return None,
    };
    julia_repl_name(&format!("{prefix}{}", spelled_digit(letter)))
}
