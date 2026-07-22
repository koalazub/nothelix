use super::path;
use super::types::ErrorFrame;

const MAX_ENTRIES: usize = 6;

const RUNTIME_FRAMES: [&str; 5] = [
    "top-level scope",
    "eval",
    "include_string",
    "exec_options",
    "_start",
];

pub(super) fn build(frames: &[ErrorFrame]) -> Vec<String> {
    let mut chain: Vec<String> = Vec::new();
    let mut collapsed = 0u32;

    for frame in frames.iter().filter(|f| !is_runtime_noise(f)) {
        if frame.is_user_code {
            if collapsed > 0 {
                chain.push(format!("[{collapsed} stdlib frames]"));
                collapsed = 0;
            }
            chain.push(user_entry(frame));
        } else {
            if chain.is_empty() || collapsed == 0 {
                chain.push(library_entry(frame));
            }
            collapsed += 1;
        }
    }
    if collapsed > 1 {
        chain.push(format!("[{} more stdlib frames]", collapsed - 1));
    }
    chain.truncate(MAX_ENTRIES);
    chain
}

fn is_runtime_noise(frame: &ErrorFrame) -> bool {
    RUNTIME_FRAMES.contains(&frame.func.as_str()) || frame.func.starts_with('#')
}

fn user_entry(frame: &ErrorFrame) -> String {
    if frame.line > 0 {
        format!("{}() at cell line {}", frame.func, frame.line)
    } else {
        format!("{}()", frame.func)
    }
}

fn library_entry(frame: &ErrorFrame) -> String {
    let file = path::clean(&frame.file);
    if frame.line > 0 {
        format!("{}() at {}:{}", frame.func, file, frame.line)
    } else {
        format!("{}() at {}", frame.func, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(func: &str, file: &str, line: i64, is_user_code: bool) -> ErrorFrame {
        ErrorFrame {
            file: file.to_string(),
            line,
            func: func.to_string(),
            is_user_code,
        }
    }

    #[test]
    fn empty_frames_yield_empty_chain() {
        assert!(build(&[]).is_empty());
    }

    #[test]
    fn runtime_frames_are_dropped() {
        let frames = [
            frame("top-level scope", "<cell>", 1, true),
            frame("#kw##foo", "<cell>", 2, true),
        ];
        assert!(build(&frames).is_empty());
    }

    #[test]
    fn consecutive_library_frames_collapse_after_the_first() {
        let frames = [
            frame("compute", "<cell>", 5, true),
            frame("norm", "/a/b/c/generic.jl", 760, false),
            frame("inner", "/a/b/c/inner.jl", 12, false),
            frame("deeper", "/a/b/c/deep.jl", 3, false),
        ];
        let chain = build(&frames);
        assert_eq!(chain[0], "compute() at cell line 5");
        assert!(chain[1].starts_with("norm() at"), "{chain:?}");
        assert_eq!(chain[2], "[2 more stdlib frames]");
    }
}
