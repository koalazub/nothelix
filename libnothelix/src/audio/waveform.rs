use super::wav;
use crate::chart::braille::BrailleCanvas;
use crate::error::FFI_ERROR_PREFIX;
use crate::json_utils::{SECTION_SEP, SPAN_SEP};
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::UNIX_EPOCH;

const PLAYHEAD_COLOR: i64 = 1;
const BRACKET_COLOR: i64 = 7;
const FULL_SCALE: f64 = 32_768.0;

fn envelope_cache() -> MutexGuard<'static, HashMap<String, Vec<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();
    CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn envelope_lines(mono: &[i32], cols: usize, rows: usize) -> Vec<String> {
    let mut canvas = BrailleCanvas::new(cols.max(1), rows.max(1));
    let dot_width = canvas.dot_width();
    let dot_height = canvas.dot_height();
    if mono.is_empty() || dot_width == 0 || dot_height == 0 {
        return canvas.lines();
    }
    let samples = mono.len();
    let half = (dot_height as f64 - 1.0) / 2.0;
    for x in 0..dot_width {
        let start = x * samples / dot_width;
        let end = (((x + 1) * samples) / dot_width)
            .max(start + 1)
            .min(samples);
        let (mut lowest, mut highest) = (i32::MAX, i32::MIN);
        for &value in &mono[start..end] {
            lowest = lowest.min(value);
            highest = highest.max(value);
        }
        let top = ((half - (highest as f64 / FULL_SCALE).clamp(-1.0, 1.0) * half).round())
            .clamp(0.0, dot_height as f64 - 1.0) as usize;
        let bottom = ((half - (lowest as f64 / FULL_SCALE).clamp(-1.0, 1.0) * half).round())
            .clamp(0.0, dot_height as f64 - 1.0) as usize;
        for y in top..=bottom {
            canvas.set(x, y);
        }
    }
    canvas.lines()
}

fn column_runs(cols: usize, playhead: isize, lo: isize, hi: isize) -> Vec<(usize, usize, i64)> {
    let mut colors: Vec<Option<i64>> = vec![None; cols];
    if lo >= 0 && hi > lo {
        let start = (lo as usize).min(cols);
        let end = (hi as usize).min(cols);
        for slot in colors.iter_mut().take(end).skip(start) {
            *slot = Some(BRACKET_COLOR);
        }
    }
    if playhead >= 0 && (playhead as usize) < cols {
        colors[playhead as usize] = Some(PLAYHEAD_COLOR);
    }
    let mut runs = Vec::new();
    let mut i = 0;
    while i < cols {
        match colors[i] {
            Some(color) => {
                let mut j = i + 1;
                while j < cols && colors[j] == Some(color) {
                    j += 1;
                }
                runs.push((i, j, color));
                i = j;
            }
            None => i += 1,
        }
    }
    runs
}

fn encode_blob(lines: &[String], spans: &[(usize, usize, usize, i64)]) -> String {
    let rows_blob = lines.join("\n");
    let spans_blob = spans
        .iter()
        .map(|(row, start, end, color)| format!("{row},{start},{end},{color}"))
        .collect::<Vec<_>>()
        .join(&SPAN_SEP.to_string());
    format!("{rows_blob}{SECTION_SEP}{spans_blob}")
}

fn mtime_millis(path: &str) -> u128 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|when| when.duration_since(UNIX_EPOCH).ok())
        .map(|since| since.as_millis())
        .unwrap_or(0)
}

fn cached_lines(path: &str, cols: usize, rows: usize) -> Result<Vec<String>, String> {
    let key = format!("{path}\u{0}{}\u{0}{cols}\u{0}{rows}", mtime_millis(path));
    if let Some(hit) = envelope_cache().get(&key).cloned() {
        return Ok(hit);
    }
    let bytes = std::fs::read(path).map_err(|source| format!("cannot read {path}: {source}"))?;
    let wav = wav::parse_pcm16(&bytes).map_err(|error| error.message())?;
    let lines = envelope_lines(&wav::mono(&wav), cols, rows);
    envelope_cache().insert(key, lines.clone());
    Ok(lines)
}

pub fn audio_waveform(
    path: String,
    cols: isize,
    rows: isize,
    playhead_col: isize,
    bracket_lo_col: isize,
    bracket_hi_col: isize,
) -> String {
    let cols = cols.max(1) as usize;
    let rows = rows.max(1) as usize;
    let lines = match cached_lines(&path, cols, rows) {
        Ok(lines) => lines,
        Err(message) => return format!("{FFI_ERROR_PREFIX}{message}"),
    };
    let runs = column_runs(cols, playhead_col, bracket_lo_col, bracket_hi_col);
    let spans: Vec<(usize, usize, usize, i64)> = (0..rows)
        .flat_map(|row| {
            runs.iter()
                .map(move |&(start, end, color)| (row, start, end, color))
        })
        .collect();
    encode_blob(&lines, &spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::braille::BLANK;

    fn any_set(line: &str) -> bool {
        line.chars().any(|glyph| glyph != BLANK)
    }

    fn sine(samples: usize, cycles: f64) -> Vec<i32> {
        (0..samples)
            .map(|i| {
                let phase = std::f64::consts::TAU * cycles * i as f64 / samples as f64;
                (phase.sin() * 32_000.0).round() as i32
            })
            .collect()
    }

    #[test]
    fn envelope_has_one_line_per_row_each_cols_wide() {
        let lines = envelope_lines(&sine(4_096, 20.0), 48, 4);
        assert_eq!(lines.len(), 4);
        for line in &lines {
            assert_eq!(line.chars().count(), 48);
        }
    }

    #[test]
    fn a_full_scale_sine_reaches_the_top_and_bottom_rows() {
        let lines = envelope_lines(&sine(4_096, 20.0), 48, 4);
        assert!(any_set(&lines[0]), "peak should touch the top row");
        assert!(any_set(&lines[3]), "trough should touch the bottom row");
    }

    #[test]
    fn silence_draws_a_flat_line_off_the_top_and_bottom_rows() {
        let lines = envelope_lines(&vec![0; 4_096], 48, 4);
        assert!(!any_set(&lines[0]), "silence must not reach the top row");
        assert!(!any_set(&lines[3]), "silence must not reach the bottom row");
        assert!(
            lines.iter().any(|line| any_set(line)),
            "silence still draws a centre line"
        );
    }

    #[test]
    fn playhead_and_bracket_recolour_columns_without_overlap() {
        let runs = column_runs(20, 5, 2, 9);
        assert_eq!(
            runs,
            vec![
                (2, 5, BRACKET_COLOR),
                (5, 6, PLAYHEAD_COLOR),
                (6, 9, BRACKET_COLOR)
            ]
        );
    }

    #[test]
    fn negative_playhead_and_bracket_emit_no_runs() {
        assert!(column_runs(20, -1, -1, -1).is_empty());
    }

    #[test]
    fn blob_round_trips_through_a_hand_parsed_decode() {
        let lines = vec![
            "\u{2801}\u{2802}".to_string(),
            "\u{2804}\u{2840}".to_string(),
        ];
        let spans = vec![
            (0usize, 0usize, 1usize, PLAYHEAD_COLOR),
            (1, 0, 1, PLAYHEAD_COLOR),
        ];
        let blob = encode_blob(&lines, &spans);

        let mut sections = blob.split(SECTION_SEP);
        let rows_blob = sections.next().expect("rows section");
        let spans_blob = sections.next().expect("spans section");
        assert!(sections.next().is_none(), "exactly one section separator");

        assert_eq!(
            rows_blob.split('\n').collect::<Vec<_>>(),
            vec!["\u{2801}\u{2802}", "\u{2804}\u{2840}"]
        );
        let decoded: Vec<Vec<i64>> = spans_blob
            .split(SPAN_SEP)
            .map(|span| {
                span.split(',')
                    .map(|field| field.parse().expect("number"))
                    .collect()
            })
            .collect();
        assert_eq!(decoded, vec![vec![0, 0, 1, 1], vec![1, 0, 1, 1]]);
    }

    #[test]
    fn a_missing_file_reports_an_error_blob() {
        let blob = audio_waveform("/no/such/clip.wav".to_string(), 40, 4, -1, -1, -1);
        assert!(blob.starts_with(FFI_ERROR_PREFIX), "{blob}");
    }
}
