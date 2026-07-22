const MATCHING: &str = "matching ";

pub(super) struct ErrorTokens {
    pub error_type: String,
    pub message: String,
    pub func_name: String,
    pub types: Vec<String>,
}

pub(super) fn tokenize_error(error_type: &str, message: &str) -> ErrorTokens {
    let full = if error_type.is_empty() {
        message.to_string()
    } else {
        format!("{error_type}: {message}")
    };

    let (scanned_type, body) = split_error_type(&full);

    ErrorTokens {
        error_type: if error_type.is_empty() {
            scanned_type
        } else {
            error_type.to_string()
        },
        func_name: scan_func_name(&body),
        types: scan_types(&body),
        message: body,
    }
}

fn split_error_type(s: &str) -> (String, String) {
    if let Some(colon) = s.find(": ") {
        let candidate = s[..colon].trim();
        if looks_like_type_name(candidate) {
            return (candidate.to_string(), s[colon + 2..].to_string());
        }
    }
    (String::new(), s.to_string())
}

fn looks_like_type_name(candidate: &str) -> bool {
    candidate
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_uppercase())
        && !candidate.contains(' ')
}

fn scan_func_name(msg: &str) -> String {
    let Some(idx) = msg.find(MATCHING) else {
        return String::new();
    };
    scan_word_or_operator(&msg[idx + MATCHING.len()..])
}

fn scan_word_or_operator(s: &str) -> String {
    let bytes = s.as_bytes();
    let Some(first) = bytes.first().copied() else {
        return String::new();
    };

    if !first.is_ascii_alphanumeric() && first != b'_' {
        let end = bytes
            .iter()
            .take_while(|b| **b != b'(' && **b != b' ')
            .count();
        if end > 0 {
            return s[..end].to_string();
        }
    }

    let end = bytes.iter().take_while(|b| is_name_byte(**b)).count();
    s[..end].to_string()
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'!'
}

fn scan_types(msg: &str) -> Vec<String> {
    let mut types = Vec::new();
    let bytes = msg.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b':' && bytes[i + 1] == b':' {
            i += 2;
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

fn scan_type_name(msg: &str, i: &mut usize) -> String {
    let bytes = msg.as_bytes();
    let start = *i;

    while *i < bytes.len()
        && (bytes[*i].is_ascii_alphanumeric() || bytes[*i] == b'_' || bytes[*i] == b'.')
    {
        *i += 1;
    }

    let raw = &msg[start..*i];
    if raw == "typeof" && bytes.get(*i) == Some(&b'(') {
        skip_balanced(bytes, i, b'(', b')');
        return msg[start..*i].to_string();
    }

    if bytes.get(*i) == Some(&b'{') {
        skip_balanced(bytes, i, b'{', b'}');
    }

    match raw.rfind('.') {
        Some(dot) => raw[dot + 1..].to_string(),
        None => raw.to_string(),
    }
}

fn skip_balanced(bytes: &[u8], i: &mut usize, open: u8, close: u8) {
    *i += 1;
    let mut depth = 1;
    while *i < bytes.len() && depth > 0 {
        if bytes[*i] == open {
            depth += 1;
        } else if bytes[*i] == close {
            depth -= 1;
        }
        *i += 1;
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
