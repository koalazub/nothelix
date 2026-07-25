use super::wav;
use crate::chart::braille::BrailleCanvas;
use crate::error::FFI_ERROR_PREFIX;
use crate::json_utils::{SECTION_SEP, SPAN_SEP};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::UNIX_EPOCH;

pub(super) const PLAYHEAD_COLOR: i64 = 1;
const BRACKET_COLOR: i64 = 7;
const FULL_SCALE: f64 = 32_768.0;
const SILENCE_FLOOR: f64 = FULL_SCALE * 0.01;

fn envelope_cache() -> MutexGuard<'static, HashMap<String, Vec<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();
    CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn clip_peak(mono: &[i32]) -> f64 {
    mono.iter()
        .map(|&value| value.unsigned_abs())
        .max()
        .unwrap_or(0) as f64
}

pub(crate) fn envelope_lines(mono: &[i32], cols: usize, rows: usize) -> Vec<String> {
    envelope_lines_with_peak(mono, cols, rows, clip_peak(mono))
}

pub(crate) fn envelope_lines_with_peak(
    mono: &[i32],
    cols: usize,
    rows: usize,
    peak: f64,
) -> Vec<String> {
    let mut canvas = BrailleCanvas::new(cols.max(1), rows.max(1));
    let dot_width = canvas.dot_width();
    let dot_height = canvas.dot_height();
    if mono.is_empty() || dot_width == 0 || dot_height == 0 {
        return canvas.lines();
    }
    if peak < SILENCE_FLOOR {
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
        let top = ((half - (highest as f64 / peak).clamp(-1.0, 1.0) * half).round())
            .clamp(0.0, dot_height as f64 - 1.0) as usize;
        let bottom = ((half - (lowest as f64 / peak).clamp(-1.0, 1.0) * half).round())
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

pub(super) fn encode_blob(lines: &[String], spans: &[(usize, usize, usize, i64)]) -> String {
    let rows_blob = lines.join("\n");
    let spans_blob = spans
        .iter()
        .map(|(row, start, end, color)| format!("{row},{start},{end},{color}"))
        .collect::<Vec<_>>()
        .join(&SPAN_SEP.to_string());
    format!("{rows_blob}{SECTION_SEP}{spans_blob}")
}

pub(super) fn mtime_millis(path: &str) -> u128 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|when| when.duration_since(UNIX_EPOCH).ok())
        .map(|since| since.as_millis())
        .unwrap_or(0)
}

pub(super) struct MonoClip {
    pub rate: u32,
    pub samples: Vec<i32>,
    pub peak: f64,
}

fn clip_cache() -> MutexGuard<'static, HashMap<String, Arc<MonoClip>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<MonoClip>>>> = OnceLock::new();
    CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

pub(super) fn cached_clip(path: &str) -> Result<Arc<MonoClip>, String> {
    let key = format!("{path}\u{0}{}", mtime_millis(path));
    if let Some(hit) = clip_cache().get(&key).cloned() {
        return Ok(hit);
    }
    let bytes = std::fs::read(path).map_err(|source| format!("cannot read {path}: {source}"))?;
    let wav = wav::parse_pcm16(&bytes).map_err(|error| error.message())?;
    let samples = wav::mono(&wav);
    let clip = Arc::new(MonoClip {
        rate: wav.rate,
        peak: clip_peak(&samples),
        samples,
    });
    clip_cache().insert(key, clip.clone());
    Ok(clip)
}

pub(super) fn window_bounds(
    clip: &MonoClip,
    position_ms: isize,
    window_ms: isize,
) -> (usize, usize) {
    let total = clip.samples.len();
    let per_ms = clip.rate as u64;
    let width = ((window_ms.max(1) as u64 * per_ms) / 1000).max(1) as usize;
    let centre = ((position_ms.max(0) as u64 * per_ms) / 1000) as usize;
    let start = centre.saturating_sub(width / 2).min(total);
    (start, (start + width).min(total))
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
    playhead_col_plus1: isize,
    bracket_lo_col_plus1: isize,
    bracket_hi_col_plus1: isize,
) -> String {
    let cols = cols.max(1) as usize;
    let rows = rows.max(1) as usize;
    let lines = match cached_lines(&path, cols, rows) {
        Ok(lines) => lines,
        Err(message) => return format!("{FFI_ERROR_PREFIX}{message}"),
    };
    let runs = column_runs(
        cols,
        playhead_col_plus1 - 1,
        bracket_lo_col_plus1 - 1,
        bracket_hi_col_plus1 - 1,
    );
    let spans: Vec<(usize, usize, usize, i64)> = (0..rows)
        .flat_map(|row| {
            runs.iter()
                .map(move |&(start, end, color)| (row, start, end, color))
        })
        .collect();
    encode_blob(&lines, &spans)
}

pub fn audio_window(
    path: String,
    cols: isize,
    rows: isize,
    position_ms: isize,
    window_ms: isize,
) -> String {
    let cols = cols.max(1) as usize;
    let rows = rows.max(1) as usize;
    let clip = match cached_clip(&path) {
        Ok(clip) => clip,
        Err(message) => return format!("{FFI_ERROR_PREFIX}{message}"),
    };
    let (start, end) = window_bounds(&clip, position_ms, window_ms);
    let lines = envelope_lines_with_peak(&clip.samples[start..end], cols, rows, clip.peak);
    let centre = cols / 2;
    let spans: Vec<(usize, usize, usize, i64)> = (0..rows)
        .map(|row| (row, centre, centre + 1, PLAYHEAD_COLOR))
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

    fn sine_amp(samples: usize, cycles: f64, amplitude: f64) -> Vec<i32> {
        (0..samples)
            .map(|i| {
                let phase = std::f64::consts::TAU * cycles * i as f64 / samples as f64;
                (phase.sin() * amplitude).round() as i32
            })
            .collect()
    }

    fn sine(samples: usize, cycles: f64) -> Vec<i32> {
        sine_amp(samples, cycles, 32_000.0)
    }

    fn lit_rows(lines: &[String]) -> Vec<bool> {
        lines.iter().map(|line| any_set(line)).collect()
    }

    fn clip_of(pattern: &[i32], rate: u32) -> MonoClip {
        MonoClip {
            rate,
            peak: clip_peak(pattern),
            samples: pattern.to_vec(),
        }
    }

    #[test]
    fn a_window_tracks_the_playing_position() {
        let mut samples = vec![0i32; 8_000];
        for (i, slot) in samples.iter_mut().enumerate().take(2_000).skip(1_000) {
            *slot = if i % 2 == 0 { 30_000 } else { -30_000 };
        }
        let clip = clip_of(&samples, 8_000);

        let (early_start, early_end) = window_bounds(&clip, 150, 200);
        let (late_start, late_end) = window_bounds(&clip, 800, 200);
        assert!(
            late_start > early_start,
            "the window must advance with time"
        );

        let loud = envelope_lines_with_peak(&samples[early_start..early_end], 20, 4, clip.peak);
        let quiet = envelope_lines_with_peak(&samples[late_start..late_end], 20, 4, clip.peak);
        assert_ne!(
            loud, quiet,
            "a moving window must redraw as the signal changes"
        );
        assert!(any_set(&loud[0]), "the burst window reaches the top row");
        assert!(
            !any_set(&quiet[0]) && !any_set(&quiet[3]),
            "a silent stretch rests on the centre line instead of filling the rows"
        );
    }

    #[test]
    fn window_bounds_stay_inside_the_clip() {
        let clip = clip_of(&vec![0i32; 4_000], 8_000);
        let (start, end) = window_bounds(&clip, 100_000, 1_000);
        assert!(start <= clip.samples.len() && end <= clip.samples.len());
        assert!(end >= start);
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
    fn pure_silence_lights_no_rows() {
        let lines = envelope_lines(&vec![0; 4_096], 48, 4);
        assert!(
            lines.iter().all(|line| !any_set(line)),
            "pure silence must light no rows"
        );
    }

    #[test]
    fn a_sub_one_percent_clip_is_treated_as_silence() {
        let lines = envelope_lines(&sine_amp(4_096, 20.0, 200.0), 48, 4);
        assert!(
            lines.iter().all(|line| !any_set(line)),
            "a clip under 1% full scale must not amplify noise into a waveform"
        );
    }

    #[test]
    fn a_half_amplitude_sine_lights_the_same_rows_as_a_full_scale_one() {
        let full = envelope_lines(&sine_amp(4_096, 20.0, 32_000.0), 48, 4);
        let half = envelope_lines(&sine_amp(4_096, 20.0, 16_000.0), 48, 4);
        assert_eq!(
            lit_rows(&full),
            lit_rows(&half),
            "peak-normalised envelope must fill the same rows regardless of clip loudness"
        );
        assert!(
            any_set(&half[0]) && any_set(&half[3]),
            "half-scale must reach both extremes"
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
        let blob = audio_waveform("/no/such/clip.wav".to_string(), 40, 4, 0, 0, 0);
        assert!(blob.starts_with(FFI_ERROR_PREFIX), "{blob}");
    }

    #[test]
    fn plus1_zero_means_no_playhead_and_no_bracket() {
        assert!(column_runs(10, -1, -1, -1).is_empty());
        assert!(!column_runs(10, 3, -1, -1).is_empty());
    }
}
