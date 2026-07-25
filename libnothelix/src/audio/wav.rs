pub(super) const CANONICAL_HEADER_LEN: usize = 44;

pub(super) struct WavData {
    pub channels: u16,
    pub rate: u32,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub(super) enum WavError {
    Unsupported(String),
    Malformed(String),
}

impl WavError {
    pub(super) fn message(&self) -> String {
        match self {
            Self::Unsupported(descriptor) => {
                format!("{descriptor} unsupported — PCM 8/16/24/32 and float 32/64 are read")
            }
            Self::Malformed(detail) => detail.clone(),
        }
    }
}

const FORMAT_PCM: u16 = 1;
const FORMAT_FLOAT: u16 = 3;
const FORMAT_EXTENSIBLE: u16 = 0xFFFE;

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    bytes
        .get(at..at + 2)
        .map(|slice| u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    bytes
        .get(at..at + 4)
        .map(|slice| u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn format_descriptor(audio_format: u16, bits: u16) -> String {
    match audio_format {
        FORMAT_PCM => format!("PCM{bits}"),
        FORMAT_FLOAT => format!("float{bits}"),
        other => format!("format {other}"),
    }
}

fn effective_format(bytes: &[u8], body_start: usize, body_end: usize, declared: u16) -> u16 {
    if declared != FORMAT_EXTENSIBLE || body_end.saturating_sub(body_start) < 26 {
        return declared;
    }
    read_u16(bytes, body_start + 24).unwrap_or(declared)
}

fn scale_float(value: f64) -> i16 {
    (value.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

fn to_pcm16_bytes(data: Vec<u8>, audio_format: u16, bits: u16) -> Result<Vec<u8>, WavError> {
    let converted: Vec<i16> = match (audio_format, bits) {
        (FORMAT_PCM, 16) => return Ok(data),
        (FORMAT_PCM, 8) => data
            .iter()
            .map(|&byte| ((byte as i16) - 128) << 8)
            .collect(),
        (FORMAT_PCM, 24) => data
            .chunks_exact(3)
            .map(|triple| i16::from_le_bytes([triple[1], triple[2]]))
            .collect(),
        (FORMAT_PCM, 32) => data
            .chunks_exact(4)
            .map(|quad| i16::from_le_bytes([quad[2], quad[3]]))
            .collect(),
        (FORMAT_FLOAT, 32) => data
            .chunks_exact(4)
            .map(
                |quad| scale_float(f32::from_le_bytes([quad[0], quad[1], quad[2], quad[3]]) as f64),
            )
            .collect(),
        (FORMAT_FLOAT, 64) => data
            .chunks_exact(8)
            .map(|octet| {
                scale_float(f64::from_le_bytes([
                    octet[0], octet[1], octet[2], octet[3], octet[4], octet[5], octet[6], octet[7],
                ]))
            })
            .collect(),
        _ => return Err(WavError::Unsupported(format_descriptor(audio_format, bits))),
    };
    Ok(converted
        .into_iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect())
}

pub(super) fn parse_pcm16(bytes: &[u8]) -> Result<WavData, WavError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(WavError::Malformed("not a RIFF/WAVE container".to_string()));
    }

    let mut format: Option<(u16, u16, u32, u16)> = None;
    let mut data: Option<Vec<u8>> = None;
    let mut pos = 12;

    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = read_u32(bytes, pos + 4)
            .ok_or_else(|| WavError::Malformed("truncated chunk header".to_string()))?
            as usize;
        let body_start = pos + 8;
        let body_end = body_start.saturating_add(size).min(bytes.len());
        match id {
            b"fmt " => {
                if body_end - body_start < 16 {
                    return Err(WavError::Malformed("short fmt chunk".to_string()));
                }
                let declared = read_u16(bytes, body_start).unwrap_or(0);
                format = Some((
                    effective_format(bytes, body_start, body_end, declared),
                    read_u16(bytes, body_start + 2).unwrap_or(0),
                    read_u32(bytes, body_start + 4).unwrap_or(0),
                    read_u16(bytes, body_start + 14).unwrap_or(0),
                ));
            }
            b"data" => {
                data = Some(bytes[body_start..body_end].to_vec());
                if format.is_some() {
                    break;
                }
            }
            _ => {}
        }
        pos = body_end + (size & 1);
    }

    let (audio_format, channels, rate, bits) =
        format.ok_or_else(|| WavError::Malformed("missing fmt chunk".to_string()))?;
    if channels == 0 || rate == 0 {
        return Err(WavError::Malformed(
            "zero channels or sample rate".to_string(),
        ));
    }
    let data = data.ok_or_else(|| WavError::Malformed("missing data chunk".to_string()))?;
    let data = to_pcm16_bytes(data, audio_format, bits)?;
    Ok(WavData {
        channels,
        rate,
        data,
    })
}

pub(super) fn samples_i16(wav: &WavData) -> Vec<i16> {
    wav.data
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect()
}

pub(super) fn mono(wav: &WavData) -> Vec<i32> {
    let channels = wav.channels.max(1) as usize;
    let samples = samples_i16(wav);
    if channels <= 1 {
        return samples.iter().map(|&value| value as i32).collect();
    }
    samples
        .chunks(channels)
        .map(|frame| frame.iter().map(|&value| value as i32).sum::<i32>() / frame.len() as i32)
        .collect()
}

pub(super) fn data_byte_offset(rate: u32, channels: u16, offset_ms: u64) -> usize {
    let block_align = channels.max(1) as u64 * 2;
    let frame = offset_ms.saturating_mul(rate as u64) / 1000;
    (frame * block_align) as usize
}

pub(super) fn write_canonical(channels: u16, rate: u32, data: &[u8]) -> Vec<u8> {
    let block_align = channels * 2;
    let byte_rate = rate * block_align as u32;
    let data_len = data.len() as u32;
    let mut out = Vec::with_capacity(CANONICAL_HEADER_LEN + data.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(data);
    out
}

pub(super) fn slice_pcm16(bytes: &[u8], offset_ms: u64) -> Result<Vec<u8>, WavError> {
    let wav = parse_pcm16(bytes)?;
    let block = wav.channels.max(1) as usize * 2;
    let raw = data_byte_offset(wav.rate, wav.channels, offset_ms).min(wav.data.len());
    let aligned = raw - (raw % block);
    Ok(write_canonical(
        wav.channels,
        wav.rate,
        &wav.data[aligned..],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pcm16_wav(channels: u16, rate: u32, frames: &[i16]) -> Vec<u8> {
        let mut data = Vec::new();
        for &sample in frames {
            data.extend_from_slice(&sample.to_le_bytes());
        }
        write_canonical(channels, rate, &data)
    }

    #[test]
    fn parses_a_canonical_pcm16_header() {
        let bytes = pcm16_wav(2, 48_000, &[1, -1, 2, -2]);
        let wav = parse_pcm16(&bytes).unwrap_or_else(|error| panic!("{}", error.message()));
        assert_eq!(wav.channels, 2);
        assert_eq!(wav.rate, 48_000);
        assert_eq!(samples_i16(&wav), vec![1, -1, 2, -2]);
    }

    #[test]
    fn mono_mixdown_averages_the_channels() {
        let bytes = pcm16_wav(2, 8_000, &[10, 30, -100, -200]);
        let wav = parse_pcm16(&bytes).expect("pcm16");
        assert_eq!(mono(&wav), vec![20, -150]);
    }

    fn wav_with_format(audio_format: u16, bits: u16, channels: u16, data: &[u8]) -> Vec<u8> {
        let block_align = channels * bits / 8;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&audio_format.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&8_000u32.to_le_bytes());
        out.extend_from_slice(&(8_000 * block_align as u32).to_le_bytes());
        out.extend_from_slice(&block_align.to_le_bytes());
        out.extend_from_slice(&bits.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    fn extensible_wav(sub_format: u16, bits: u16, channels: u16, data: &[u8]) -> Vec<u8> {
        let block_align = channels * bits / 8;
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&FORMAT_EXTENSIBLE.to_le_bytes());
        fmt.extend_from_slice(&channels.to_le_bytes());
        fmt.extend_from_slice(&8_000u32.to_le_bytes());
        fmt.extend_from_slice(&(8_000 * block_align as u32).to_le_bytes());
        fmt.extend_from_slice(&block_align.to_le_bytes());
        fmt.extend_from_slice(&bits.to_le_bytes());
        fmt.extend_from_slice(&22u16.to_le_bytes());
        fmt.extend_from_slice(&bits.to_le_bytes());
        fmt.extend_from_slice(&0u32.to_le_bytes());
        fmt.extend_from_slice(&sub_format.to_le_bytes());
        fmt.extend_from_slice(&[0u8; 14]);

        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(20 + fmt.len() as u32 + data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        out.extend_from_slice(&fmt);
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn eight_bit_pcm_centres_on_the_unsigned_midpoint() {
        let bytes = wav_with_format(FORMAT_PCM, 8, 1, &[128, 255, 0]);
        let wav = parse_pcm16(&bytes).unwrap_or_else(|e| panic!("{}", e.message()));
        let samples = samples_i16(&wav);
        assert_eq!(samples[0], 0);
        assert!(samples[1] > 32_000, "0xFF is near full positive");
        assert!(samples[2] < -32_000, "0x00 is near full negative");
    }

    #[test]
    fn twenty_four_and_thirty_two_bit_pcm_keep_the_high_bytes() {
        let wav24 = parse_pcm16(&wav_with_format(FORMAT_PCM, 24, 1, &[0x00, 0x00, 0x40]))
            .unwrap_or_else(|e| panic!("{}", e.message()));
        assert_eq!(samples_i16(&wav24), vec![0x4000]);
        let wav32 = parse_pcm16(&wav_with_format(
            FORMAT_PCM,
            32,
            1,
            &[0x00, 0x00, 0x00, 0x40],
        ))
        .unwrap_or_else(|e| panic!("{}", e.message()));
        assert_eq!(samples_i16(&wav32), vec![0x4000]);
    }

    #[test]
    fn ieee_float_scales_into_the_pcm16_range() {
        let mut data = Vec::new();
        for value in [0.0f32, 1.0, -1.0, 0.5, 2.0] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        let wav = parse_pcm16(&wav_with_format(FORMAT_FLOAT, 32, 1, &data))
            .unwrap_or_else(|e| panic!("{}", e.message()));
        assert_eq!(samples_i16(&wav), vec![0, 32_767, -32_767, 16_384, 32_767]);
    }

    #[test]
    fn extensible_float32_is_read_through_its_subformat_guid() {
        let mut data = Vec::new();
        for value in [0.0f32, 0.5, -0.5] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        let wav = parse_pcm16(&extensible_wav(FORMAT_FLOAT, 32, 1, &data))
            .unwrap_or_else(|e| panic!("{}", e.message()));
        assert_eq!(wav.channels, 1);
        assert_eq!(samples_i16(&wav), vec![0, 16_384, -16_384]);
    }

    #[test]
    fn extensible_pcm16_is_read_through_its_subformat_guid() {
        let data: Vec<u8> = [100i16, -100]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let wav = parse_pcm16(&extensible_wav(FORMAT_PCM, 16, 1, &data))
            .unwrap_or_else(|e| panic!("{}", e.message()));
        assert_eq!(samples_i16(&wav), vec![100, -100]);
    }

    #[test]
    fn a_genuinely_unknown_format_still_reports_unsupported() {
        let bytes = wav_with_format(7, 16, 1, &[0, 0]);
        let error = parse_pcm16(&bytes).err().expect("unsupported");
        assert!(error.message().starts_with("format 7 unsupported"));
    }

    #[test]
    fn byte_offset_lands_on_the_frame_boundary() {
        assert_eq!(data_byte_offset(8_000, 1, 0), 0);
        assert_eq!(data_byte_offset(8_000, 1, 1_000), 16_000);
        assert_eq!(data_byte_offset(8_000, 2, 1_000), 32_000);
        assert_eq!(data_byte_offset(44_100, 2, 500), 88_200);
    }

    #[test]
    fn slice_drops_the_leading_samples_and_rewrites_the_header() {
        let bytes = pcm16_wav(1, 8_000, &(0..8_000i16).collect::<Vec<_>>());
        let sliced = slice_pcm16(&bytes, 500).expect("slice");
        let wav = parse_pcm16(&sliced).expect("sliced pcm16");
        assert_eq!(sliced.len(), CANONICAL_HEADER_LEN + wav.data.len());
        assert_eq!(wav.data.len(), 8_000);
        assert_eq!(samples_i16(&wav).first().copied(), Some(4_000));
    }

    #[test]
    fn slice_past_the_end_yields_an_empty_data_chunk() {
        let bytes = pcm16_wav(1, 8_000, &[1, 2, 3, 4]);
        let sliced = slice_pcm16(&bytes, 10_000).expect("slice");
        let wav = parse_pcm16(&sliced).expect("sliced pcm16");
        assert!(wav.data.is_empty());
    }
}
