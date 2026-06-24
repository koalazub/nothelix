use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use nothelix::{render_math_to_png, CORPUS};

#[test]
fn render_full_math_corpus() {
    let mut failures: Vec<(String, String)> = Vec::new();
    let mut total_bytes: usize = 0;
    let start = std::time::Instant::now();

    for (name, latex) in CORPUS {
        let json = render_math_to_png(latex.to_string(), 14);
        if !json.contains("\"error\":\"\"") {
            failures.push((name.to_string(), json));
        } else if let Some(b64_start) = json.find("\"b64\":\"") {
            let b64_end = json[b64_start + 7..].find('"').unwrap_or(0) + b64_start + 7;
            let b64 = &json[b64_start + 7..b64_end];
            total_bytes += BASE64.decode(b64).map(|v| v.len()).unwrap_or(0);
        }
    }

    let elapsed = start.elapsed();
    let count = CORPUS.len();
    let success = count - failures.len();

    eprintln!(
        "math corpus: {success}/{count} rendered, {elapsed:?}, total PNG bytes: {total_bytes}"
    );

    if !failures.is_empty() {
        for (name, json) in &failures {
            eprintln!("FAIL {name}: {json}");
        }
        panic!("{} math corpus expression(s) failed to render", failures.len());
    }
}

#[test]
fn corpus_names_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for (name, _) in CORPUS {
        assert!(seen.insert(*name), "duplicate corpus entry: {name}");
    }
}
