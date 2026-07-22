#[derive(Clone, Copy)]
struct Fence {
    open: &'static str,
    mid: &'static str,
    close: &'static str,
}

impl Fence {
    fn of(env: &str) -> Option<Self> {
        match env {
            "cases" => Some(Self {
                open: "⎧",
                mid: "⎨",
                close: "⎩",
            }),
            "pmatrix" => Some(Self {
                open: "⎛",
                mid: "⎜",
                close: "⎞",
            }),
            "bmatrix" => Some(Self {
                open: "⎡",
                mid: "⎢",
                close: "⎤",
            }),
            "vmatrix" => Some(Self {
                open: "│",
                mid: "│",
                close: "│",
            }),
            _ => None,
        }
    }
}

fn count_rows(body: &str) -> usize {
    let bytes = body.as_bytes();
    let mut rows = 1;
    let mut k = 0;
    while k + 1 < bytes.len() {
        if bytes[k] == b'\\' && bytes[k + 1] == b'\\' {
            rows += 1;
            k += 2;
        } else {
            k += 1;
        }
    }
    rows
}

pub(super) struct Environment {
    fence: Option<Fence>,
    row: usize,
    total_rows: usize,
}

impl Environment {
    pub fn opening(name: &str, body: &str) -> Self {
        Self {
            fence: Fence::of(name),
            row: 0,
            total_rows: count_rows(body),
        }
    }

    pub fn open_fence(&self) -> Option<&'static str> {
        self.fence.map(|fence| fence.open)
    }

    pub fn advance_row(&mut self) -> Option<&'static str> {
        self.row += 1;
        let last_row = self.row == self.total_rows.saturating_sub(1);
        self.fence
            .map(|fence| if last_row { fence.close } else { fence.mid })
    }

    pub fn close_fence(self) -> Option<&'static str> {
        if self.row == 0 {
            self.fence.map(|fence| fence.close)
        } else {
            None
        }
    }
}
