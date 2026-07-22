use std::collections::HashMap;
use std::sync::Mutex;

use super::color::HexColor;
use super::reply::RenderedSvg;
use crate::error::{Error, Result};

const SUBJECT: &str = "math image cache";

#[derive(PartialEq, Eq, Hash)]
pub(super) struct RenderRequest {
    latex: String,
    font_size_pt: isize,
    color: HexColor,
}

impl RenderRequest {
    pub(super) fn new(latex: &str, font_size_pt: isize, color: &HexColor) -> Self {
        Self {
            latex: latex.to_string(),
            font_size_pt,
            color: color.clone(),
        }
    }
}

static RENDERED: Mutex<Option<HashMap<RenderRequest, RenderedSvg>>> = Mutex::new(None);

pub(super) fn lookup(request: &RenderRequest) -> Result<Option<RenderedSvg>> {
    let mut guard = RENDERED
        .lock()
        .map_err(|_| Error::LockPoisoned { subject: SUBJECT })?;
    Ok(guard.get_or_insert_with(HashMap::new).get(request).cloned())
}

pub(super) fn store(request: RenderRequest, svg: &RenderedSvg) -> Result<()> {
    let mut guard = RENDERED
        .lock()
        .map_err(|_| Error::LockPoisoned { subject: SUBJECT })?;
    guard
        .get_or_insert_with(HashMap::new)
        .insert(request, svg.clone());
    Ok(())
}
