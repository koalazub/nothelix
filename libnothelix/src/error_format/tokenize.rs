//! Error message tokenizer.
//!
//! Walks a raw Julia error string and extracts:
//!   - The error type (`MethodError`, `BoundsError`, etc.)
//!   - The message body
//!   - The function name from "matching f(…)" patterns
//!   - Argument types from `::TypeName` patterns
//!
//! Token-based: no regex. The output `ErrorTokens` is consumed by the
//! hint matcher and the per-enricher scanners.

/// Tokens extracted from an error message by scanning.
pub(super) struct ErrorTokens {
    /// The Julia error type: "`MethodError`", "`BoundsError`", etc.
    pub error_type: String,
    /// The message body (everything after "`ErrorType`: ").
    pub message: String,
    /// Function name from "matching func(" or similar.
    pub func_name: String,
    /// Type names extracted from `::TypeName` patterns.
    pub types: Vec<String>,
}

/// Scan an error string into structured tokens.
/// Handles both "`ErrorType`: message" format and bare messages.
pub(super) fn tokenize_error(error_type: &str, message: &str) -> ErrorTokens {
    let full = if error_type.is_empty() {
        message.to_string()
    } else {
        format!("{error_type}: {message}")
    };

    let (etype, msg) = split_error_type(&full);

    ErrorTokens {
        error_type: if error_type.is_empty() {
            etype
        } else {
            error_type.to_string()
        },
        func_name: scan_func_name(&msg),
        types: scan_types(&msg),
        message: msg,
    }
}

/// Split "`ErrorType`: message" → (type, message).
/// Falls back to ("", `full_string`) if no colon found.
fn split_error_type(s: &str) -> (String, String) {
    if let Some(colon) = s.find(": ") {
        let etype = s[..colon].trim();
        // Only treat as error type if it looks like a type name (starts with uppercase, no spaces)
        if !etype.is_empty()
            && etype.bytes().next().is_some_and(|b| b.is_ascii_uppercase())
            && !etype.contains(' ')
        {
            return (etype.to_string(), s[colon + 2..].to_string());
        }
    }
    (String::new(), s.to_string())
}

/// Extract function name from patterns like "matching funcname(" or "funcname(".
fn scan_func_name(msg: &str) -> String {
    // "no method matching funcname(" → "funcname"
    if let Some(idx) = msg.find("matching ") {
        let after = &msg[idx + 9..];
        // Skip operator characters to get to the function name
        let name = scan_word_or_operator(after);
        if !name.is_empty() {
            return name;
        }
    }
    String::new()
}

/// Scan a word (alphanumeric + _ + !) or operator symbol at the start of a string.
pub(super) fn scan_word_or_operator(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return String::new();
    }

    // Operator: single non-alpha char before '('
    // e.g., "+(" → "+", "*(..." → "*"
    if !bytes[0].is_ascii_alphanumeric() && bytes[0] != b'_' {
        let mut i = 0;
        while i < bytes.len() && bytes[i] != b'(' && bytes[i] != b' ' {
            i += 1;
        }
        if i > 0 {
            return s[..i].to_string();
        }
    }

    // Word: alphanumeric + _ + !
    let mut i = 0;
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'!')
    {
        i += 1;
    }
    s[..i].to_string()
}

/// Extract all type names from `::TypeName` patterns in the message.
/// "no method matching +(`::Vector{Float64`}, `::Int64`)" → ["Vector", "Int64"]
pub(super) fn scan_types(msg: &str) -> Vec<String> {
    let mut types = Vec::new();
    let bytes = msg.as_bytes();
    let mut i = 0;

    while i + 1 < bytes.len() {
        if bytes[i] == b':' && bytes[i + 1] == b':' {
            i += 2;
            // Skip optional qualifiers like "Base." or "Union{"
            // Scan the type name
            let type_name = scan_type_name(msg, &mut i);
            if !type_name.is_empty() {
                types.push(type_name);
            }
        } else {
            i += 1;
        }
    }

    types
}

/// Scan a type name starting at position i. Handles:
/// - Simple: "Int64" → "Int64"
/// - Qualified: "Base.Missing" → "Missing"
/// - Parametric: "Vector{Float64}" → "Vector"
/// - typeof: "typeof(sqrt)" → "typeof(sqrt)"
pub(super) fn scan_type_name(msg: &str, i: &mut usize) -> String {
    let bytes = msg.as_bytes();
    let start = *i;

    // Scan alphanumeric + _ + .
    while *i < bytes.len()
        && (bytes[*i].is_ascii_alphanumeric() || bytes[*i] == b'_' || bytes[*i] == b'.')
    {
        *i += 1;
    }

    // Handle typeof(func) specially
    let raw = &msg[start..*i];
    if raw == "typeof" && *i < bytes.len() && bytes[*i] == b'(' {
        // Scan to closing paren
        *i += 1;
        let mut depth = 1;
        while *i < bytes.len() && depth > 0 {
            if bytes[*i] == b'(' {
                depth += 1;
            } else if bytes[*i] == b')' {
                depth -= 1;
            }
            *i += 1;
        }
        return msg[start..*i].to_string();
    }

    // Skip parametric part {T} — we only want the base type name
    if *i < bytes.len() && bytes[*i] == b'{' {
        *i += 1;
        let mut depth = 1;
        while *i < bytes.len() && depth > 0 {
            if bytes[*i] == b'{' {
                depth += 1;
            } else if bytes[*i] == b'}' {
                depth -= 1;
            }
            *i += 1;
        }
    }

    // Take only the last segment after dots: "Base.Missing" → "Missing"
    if let Some(dot) = raw.rfind('.') {
        raw[dot + 1..].to_string()
    } else {
        raw.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_method_error_with_types() {
        let tokens = tokenize_error(
            "MethodError",
            "no method matching +(::Vector{Float64}, ::Int64)",
        );
        assert_eq!(tokens.error_type, "MethodError");
        assert_eq!(tokens.func_name, "+");
        assert_eq!(tokens.types, vec!["Vector", "Int64"]);
    }

    #[test]
    fn tokenize_undef_var() {
        let tokens = tokenize_error("UndefVarError", "myvar not defined");
        assert_eq!(tokens.error_type, "UndefVarError");
        assert_eq!(tokens.message, "myvar not defined");
    }

    #[test]
    fn tokenize_qualified_type() {
        let tokens = tokenize_error("MethodError", "no method matching norm(::Base.Missing)");
        assert_eq!(tokens.func_name, "norm");
        assert!(tokens.types.contains(&"Missing".to_string()));
    }

    #[test]
    fn tokenize_typeof() {
        let tokens = tokenize_error(
            "MethodError",
            "no method matching /(::Int64, ::typeof(sqrt))",
        );
        assert_eq!(tokens.func_name, "/");
        assert!(tokens.types.iter().any(|t| t.contains("typeof(sqrt)")));
    }

    #[test]
    fn tokenize_raw_string() {
        let tokens = tokenize_error(
            "",
            "BoundsError: attempt to access 3-element Vector{Int64} at index [0]",
        );
        assert_eq!(tokens.error_type, "BoundsError");
        assert!(tokens.message.contains("index [0]"));
    }
}
