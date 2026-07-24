const DOT_BITS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

pub(crate) const BLANK: char = '\u{2800}';

pub(super) const DOTS_PER_CELL_X: usize = 2;
pub(super) const DOTS_PER_CELL_Y: usize = 4;

pub(crate) struct BrailleCanvas {
    cols: usize,
    rows: usize,
    cells: Vec<u8>,
}

impl BrailleCanvas {
    pub(crate) fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols,
            rows,
            cells: vec![0u8; cols * rows],
        }
    }

    pub(crate) fn dot_width(&self) -> usize {
        self.cols * DOTS_PER_CELL_X
    }

    pub(crate) fn dot_height(&self) -> usize {
        self.rows * DOTS_PER_CELL_Y
    }

    pub(crate) fn set(&mut self, px: usize, py: usize) {
        let col = px / DOTS_PER_CELL_X;
        let row = py / DOTS_PER_CELL_Y;
        if col >= self.cols || row >= self.rows {
            return;
        }
        self.cells[row * self.cols + col] |= DOT_BITS[py % DOTS_PER_CELL_Y][px % DOTS_PER_CELL_X];
    }

    pub(crate) fn lines(&self) -> Vec<String> {
        (0..self.rows)
            .map(|row| {
                let start = row * self.cols;
                self.cells[start..start + self.cols]
                    .iter()
                    .map(|mask| glyph(*mask))
                    .collect()
            })
            .collect()
    }
}

fn glyph(mask: u8) -> char {
    char::from_u32(BLANK as u32 + u32::from(mask)).unwrap_or(BLANK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn braille_canvas_set_corners() {
        let mut canvas = BrailleCanvas::new(2, 2);
        canvas.set(0, 0);
        canvas.set(3, 7);
        let lines = canvas.lines();
        assert_eq!(lines.len(), 2);
        assert_ne!(lines[0].chars().next().expect("top-left cell"), BLANK);
        assert_ne!(lines[1].chars().nth(1).expect("bottom-right cell"), BLANK);
    }

    #[test]
    fn braille_canvas_out_of_bounds_ignored() {
        let mut canvas = BrailleCanvas::new(2, 2);
        canvas.set(100, 100);
        let all_blank = canvas
            .lines()
            .iter()
            .all(|line| line.chars().all(|c| c == BLANK));
        assert!(all_blank);
    }
}
