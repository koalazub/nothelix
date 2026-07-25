use super::wav;
use super::waveform::{PLAYHEAD_COLOR, encode_blob, mtime_millis};
use crate::error::FFI_ERROR_PREFIX;
use std::collections::HashMap;
use std::f64::consts::{PI, TAU};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

const WINDOW: usize = 512;
const DYNAMIC_RANGE_DB: f64 = 60.0;
const FULL_SCALE: f64 = 32_768.0;
const SILENCE_FLOOR: f64 = FULL_SCALE * 0.01;
const BLOCK: char = '\u{2588}';
const PALETTE: [i64; 7] = [0, 2, 3, 4, 5, 6, 7];

fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    if n <= 1 {
        return;
    }
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -TAU / len as f64;
        let (step_re, step_im) = (angle.cos(), angle.sin());
        let half = len / 2;
        let mut start = 0;
        while start < n {
            let (mut wr, mut wi) = (1.0f64, 0.0f64);
            for k in 0..half {
                let a = start + k;
                let b = start + k + half;
                let tr = wr * re[b] - wi * im[b];
                let ti = wr * im[b] + wi * re[b];
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let next_wr = wr * step_re - wi * step_im;
                wi = wr * step_im + wi * step_re;
                wr = next_wr;
            }
            start += len;
        }
        len <<= 1;
    }
}

fn hann(n: usize) -> Vec<f64> {
    if n <= 1 {
        return vec![1.0; n];
    }
    (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / (n - 1) as f64).cos())
        .collect()
}

fn frame_magnitudes(frame: &[f64]) -> Vec<f64> {
    let window = hann(WINDOW);
    let mut re = vec![0.0f64; WINDOW];
    let mut im = vec![0.0f64; WINDOW];
    for (i, slot) in re.iter_mut().enumerate() {
        *slot = frame.get(i).copied().unwrap_or(0.0) * window[i];
    }
    fft(&mut re, &mut im);
    (0..=WINDOW / 2)
        .map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt())
        .collect()
}

fn quantize(db: f64, floor: f64, range: f64) -> i64 {
    let norm = ((db - floor) / range).clamp(0.0, 1.0);
    let top = PALETTE.len() - 1;
    let level = (norm * top as f64).round() as usize;
    PALETTE[level.min(top)]
}

const FRAMES_PER_COLUMN: usize = 6;

fn column_power(mono: &[i32], start: usize, end: usize) -> Vec<f64> {
    let mut acc = vec![0.0f64; WINDOW / 2 + 1];
    let stride = (end.saturating_sub(start) / FRAMES_PER_COLUMN).max(1);
    let mut taken = 0usize;
    let mut pos = start;
    while taken < FRAMES_PER_COLUMN && pos < end {
        let frame: Vec<f64> = (0..WINDOW)
            .map(|i| mono.get(pos + i).copied().unwrap_or(0) as f64)
            .collect();
        for (bin, magnitude) in frame_magnitudes(&frame).iter().enumerate() {
            acc[bin] += magnitude * magnitude;
        }
        taken += 1;
        pos += stride;
    }
    let divisor = taken.max(1) as f64;
    acc.iter().map(|value| value / divisor).collect()
}

fn band_edges(nbins: usize, rows: usize) -> Vec<usize> {
    let hi = nbins.max(2) as f64;
    let mut edges: Vec<usize> = (0..=rows)
        .map(|band| {
            let ratio = band as f64 / rows as f64;
            hi.powf(ratio).round() as usize
        })
        .collect();
    for index in 1..edges.len() {
        if edges[index] <= edges[index - 1] {
            edges[index] = edges[index - 1] + 1;
        }
    }
    edges
}

fn spectrogram_cells(mono: &[i32], cols: usize, rows: usize) -> Vec<Vec<i64>> {
    let blank = vec![vec![PALETTE[0]; cols]; rows];
    if mono.is_empty() || cols == 0 || rows == 0 {
        return blank;
    }
    let peak_amplitude = mono
        .iter()
        .map(|&value| value.unsigned_abs())
        .max()
        .unwrap_or(0) as f64;
    if peak_amplitude < SILENCE_FLOOR {
        return blank;
    }
    let nbins = WINDOW / 2;
    let edges = band_edges(nbins, rows);
    let usable = mono.len().saturating_sub(WINDOW).max(1);
    let mut columns: Vec<Vec<f64>> = Vec::with_capacity(cols);
    let mut peak_db = f64::NEG_INFINITY;
    for x in 0..cols {
        let slice_start = x * usable / cols;
        let slice_end = ((x + 1) * usable / cols).max(slice_start + 1);
        let power = column_power(mono, slice_start, slice_end);
        let mut column = vec![f64::NEG_INFINITY; rows];
        for band in 0..rows {
            let lo = edges[band].min(power.len());
            let hi = edges[band + 1].max(lo + 1).min(power.len());
            let band_power = if lo < hi {
                power[lo..hi].iter().sum::<f64>() / (hi - lo) as f64
            } else {
                0.0
            };
            let value_db = 10.0 * band_power.max(1e-18).log10();
            peak_db = peak_db.max(value_db);
            column[rows - 1 - band] = value_db;
        }
        columns.push(column);
    }
    let quietest_db = columns
        .iter()
        .flatten()
        .copied()
        .filter(|value| value.is_finite())
        .fold(f64::INFINITY, f64::min);
    let floor = (peak_db - DYNAMIC_RANGE_DB).max(quietest_db);
    let range = (peak_db - floor).max(1.0);
    let mut grid = vec![vec![PALETTE[0]; cols]; rows];
    for (col, column) in columns.iter().enumerate() {
        for (row, &value) in column.iter().enumerate() {
            grid[row][col] = quantize(value, floor, range);
        }
    }
    grid
}

fn spectrogram_spans(
    cells: &[Vec<i64>],
    cols: usize,
    playhead: isize,
) -> Vec<(usize, usize, usize, i64)> {
    let mut spans = Vec::new();
    for (row, colors) in cells.iter().enumerate() {
        let mut effective = colors.clone();
        if playhead >= 0 && (playhead as usize) < cols {
            effective[playhead as usize] = PLAYHEAD_COLOR;
        }
        let mut i = 0;
        while i < cols {
            let color = effective[i];
            let mut j = i + 1;
            while j < cols && effective[j] == color {
                j += 1;
            }
            spans.push((row, i, j, color));
            i = j;
        }
    }
    spans
}

fn cell_cache() -> MutexGuard<'static, HashMap<String, Vec<Vec<i64>>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Vec<Vec<i64>>>>> = OnceLock::new();
    CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn cache_key(path: &str, mtime: u128, cols: usize, rows: usize) -> String {
    format!("{path}\u{0}{mtime}\u{0}{cols}\u{0}{rows}")
}

fn cached_cells(path: &str, cols: usize, rows: usize) -> Result<Vec<Vec<i64>>, String> {
    let key = cache_key(path, mtime_millis(path), cols, rows);
    if let Some(hit) = cell_cache().get(&key).cloned() {
        return Ok(hit);
    }
    let bytes = std::fs::read(path).map_err(|source| format!("cannot read {path}: {source}"))?;
    let wav = wav::parse_pcm16(&bytes).map_err(|error| error.message())?;
    let cells = spectrogram_cells(&wav::mono(&wav), cols, rows);
    cell_cache().insert(key, cells.clone());
    Ok(cells)
}

pub fn audio_spectrogram(
    path: String,
    cols: isize,
    rows: isize,
    playhead_col_plus1: isize,
) -> String {
    let cols = cols.max(1) as usize;
    let rows = rows.max(1) as usize;
    let cells = match cached_cells(&path, cols, rows) {
        Ok(cells) => cells,
        Err(message) => return format!("{FFI_ERROR_PREFIX}{message}"),
    };
    let lines: Vec<String> = (0..rows).map(|_| BLOCK.to_string().repeat(cols)).collect();
    let spans = spectrogram_spans(&cells, cols, playhead_col_plus1 - 1);
    encode_blob(&lines, &spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json_utils::{SECTION_SEP, SPAN_SEP};

    fn tone_wav(rate: u32, cycles_per_second: f64, seconds: usize) -> Vec<u8> {
        let samples = rate as usize * seconds;
        let mut data = Vec::with_capacity(samples * 2);
        for i in 0..samples {
            let value = (TAU * cycles_per_second * i as f64 / rate as f64).sin() * 20_000.0;
            data.extend_from_slice(&(value as i16).to_le_bytes());
        }
        wav::write_canonical(1, rate, &data)
    }

    fn fixture_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("nothelix-spectrogram-{tag}.wav"))
    }

    fn spans_of(blob: &str) -> Vec<Vec<i64>> {
        blob.split(SECTION_SEP)
            .nth(1)
            .unwrap_or("")
            .split(SPAN_SEP)
            .filter(|section| !section.is_empty())
            .map(|span| {
                span.split(',')
                    .map(|field| field.parse().expect("number"))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn fft_of_a_1khz_sine_at_8khz_peaks_in_the_1khz_bin() {
        let rate = 8_000.0;
        let freq = 1_000.0;
        let frame: Vec<f64> = (0..WINDOW)
            .map(|i| (TAU * freq * i as f64 / rate).sin())
            .collect();
        let mags = frame_magnitudes(&frame);
        let peak_bin = (1..=WINDOW / 2)
            .max_by(|&a, &b| mags[a].partial_cmp(&mags[b]).expect("finite"))
            .expect("a peak bin");
        assert_eq!(peak_bin, (freq * WINDOW as f64 / rate).round() as usize);
        assert_eq!(peak_bin, 64);
    }

    fn brightest_row_from_bottom(freq: f64, rows: usize) -> usize {
        let mono: Vec<i32> = (0..16_000)
            .map(|i| ((TAU * freq * i as f64 / 8_000.0).sin() * 20_000.0) as i32)
            .collect();
        let cells = spectrogram_cells(&mono, 12, rows);
        let brightest = (0..rows)
            .max_by_key(|&row| cells[row].iter().copied().max().unwrap_or(0))
            .expect("a brightest row");
        rows - 1 - brightest
    }

    #[test]
    fn the_frequency_axis_separates_a_low_tone_from_a_high_one() {
        let low = brightest_row_from_bottom(200.0, 6);
        let high = brightest_row_from_bottom(3_000.0, 6);
        assert!(
            low < high,
            "a 200 Hz tone must sit below a 3 kHz one, got {low} and {high}"
        );
    }

    #[test]
    fn geometric_band_edges_never_collapse() {
        for rows in 1..=16usize {
            let edges = band_edges(WINDOW / 2, rows);
            assert_eq!(edges.len(), rows + 1);
            for pair in edges.windows(2) {
                assert!(pair[1] > pair[0], "edges must strictly increase: {edges:?}");
            }
        }
    }

    #[test]
    fn a_pure_tone_clamps_quiet_bands_to_the_floor_level() {
        let mono: Vec<i32> = (0..16_000)
            .map(|i| ((TAU * 1_000.0 * i as f64 / 8_000.0).sin() * 20_000.0) as i32)
            .collect();
        let cells = spectrogram_cells(&mono, 20, 8);
        let flat: Vec<i64> = cells.iter().flatten().copied().collect();
        assert!(
            flat.contains(&PALETTE[0]),
            "quiet bands clamp to the floor colour"
        );
        assert!(
            flat.contains(&PALETTE[PALETTE.len() - 1]),
            "the tone's band reaches the top intensity"
        );
    }

    #[test]
    fn silence_renders_every_cell_at_the_floor_level() {
        let cells = spectrogram_cells(&vec![0; 16_000], 20, 8);
        assert!(
            cells.iter().flatten().all(|&color| color == PALETTE[0]),
            "silence must not synthesise intensity"
        );
    }

    #[test]
    fn a_spectrogram_blob_round_trips_through_a_hand_parse() {
        let cells = vec![vec![0i64, 0, 3, 3], vec![5, 5, 5, 5]];
        let cols = 4;
        let lines: Vec<String> = (0..cells.len())
            .map(|_| BLOCK.to_string().repeat(cols))
            .collect();
        let blob = encode_blob(&lines, &spectrogram_spans(&cells, cols, -1));

        let mut sections = blob.split(SECTION_SEP);
        let rows_blob = sections.next().expect("rows section");
        let spans_blob = sections.next().expect("spans section");
        assert!(sections.next().is_none(), "exactly one section separator");
        assert_eq!(
            rows_blob.split('\n').collect::<Vec<_>>(),
            vec!["████", "████"]
        );

        let decoded: Vec<Vec<i64>> = spans_blob
            .split(SPAN_SEP)
            .map(|span| {
                span.split(',')
                    .map(|field| field.parse().expect("number"))
                    .collect()
            })
            .collect();
        assert_eq!(
            decoded,
            vec![vec![0, 0, 2, 0], vec![0, 2, 4, 3], vec![1, 0, 4, 5]]
        );
    }

    #[test]
    fn the_plus1_playhead_sentinel_is_off_by_one_from_the_column() {
        let path = fixture_path("plus1");
        std::fs::write(&path, tone_wav(8_000, 1_000.0, 1)).expect("write fixture");
        let none = spans_of(&audio_spectrogram(
            path.to_string_lossy().into_owned(),
            12,
            4,
            0,
        ));
        assert!(
            none.iter().all(|span| span[3] != PLAYHEAD_COLOR),
            "plus1 = 0 recolours no column"
        );
        let at_two = spans_of(&audio_spectrogram(
            path.to_string_lossy().into_owned(),
            12,
            4,
            3,
        ));
        assert!(
            at_two
                .iter()
                .any(|span| span[3] == PLAYHEAD_COLOR && span[1] == 2),
            "plus1 = 3 recolours column 2 like the waveform playhead"
        );
    }

    #[test]
    fn the_cache_key_separates_path_mtime_and_dimensions() {
        assert_eq!(cache_key("a.wav", 1, 20, 4), cache_key("a.wav", 1, 20, 4));
        assert_ne!(cache_key("a.wav", 1, 20, 4), cache_key("b.wav", 1, 20, 4));
        assert_ne!(cache_key("a.wav", 1, 20, 4), cache_key("a.wav", 2, 20, 4));
        assert_ne!(cache_key("a.wav", 1, 20, 4), cache_key("a.wav", 1, 21, 4));
        assert_ne!(cache_key("a.wav", 1, 20, 4), cache_key("a.wav", 1, 20, 5));
    }

    #[test]
    fn a_missing_file_reports_an_error_blob() {
        let blob = audio_spectrogram("/no/such/clip.wav".to_string(), 20, 4, 0);
        assert!(blob.starts_with(FFI_ERROR_PREFIX), "{blob}");
    }
}
