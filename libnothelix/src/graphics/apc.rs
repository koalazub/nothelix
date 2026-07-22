use std::fmt::Write;

const CHUNK_BYTES: usize = 4096;

#[derive(Clone, Copy)]
pub(crate) enum Placement {
    AtCursor { image_id: u32, rows: u32 },
    UnicodePlaceholder { image_id: u32 },
}

impl Placement {
    fn opening(self, more: u32) -> String {
        match self {
            Self::AtCursor { image_id, rows } => {
                format!("a=T,f=100,t=d,q=2,I={image_id},r={rows},m={more}")
            }
            Self::UnicodePlaceholder { image_id } => {
                format!("a=T,f=100,t=d,q=2,U=1,i={image_id},m={more}")
            }
        }
    }
}

pub(crate) fn transmit(b64: &str, placement: Placement) -> String {
    let chunks = split_on_char_boundaries(b64);
    let mut out = String::with_capacity(b64.len() + chunks.len() * 64);

    for (index, chunk) in chunks.iter().enumerate() {
        let more = u32::from(index + 1 < chunks.len());
        if index == 0 {
            let _ = write!(out, "\x1b_G{};{chunk}\x1b\\", placement.opening(more));
        } else {
            let _ = write!(out, "\x1b_Gm={more};{chunk}\x1b\\");
        }
    }

    out
}

fn split_on_char_boundaries(payload: &str) -> Vec<&str> {
    let mut chunks = Vec::with_capacity(payload.len().div_ceil(CHUNK_BYTES));
    let mut start = 0;
    while start < payload.len() {
        let mut end = (start + CHUNK_BYTES).min(payload.len());
        while !payload.is_char_boundary(end) {
            end -= 1;
        }
        chunks.push(&payload[start..end]);
        start = end;
    }
    chunks
}
