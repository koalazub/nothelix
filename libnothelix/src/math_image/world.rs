use std::path::PathBuf;
use std::sync::OnceLock;

use typst::diag::{FileError, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::fonts::FontStore;

struct SharedAssets {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: FontStore,
}

fn shared_assets() -> &'static SharedAssets {
    static ASSETS: OnceLock<SharedAssets> = OnceLock::new();
    ASSETS.get_or_init(|| {
        let entries: Vec<_> = typst_kit::fonts::embedded().collect();
        let infos: Vec<_> = entries.iter().map(|(_, info)| info.clone()).collect();
        let mut fonts = FontStore::new();
        fonts.extend(entries);
        SharedAssets {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(FontBook::from_infos(infos)),
            fonts,
        }
    })
}

fn mitex_scope_sources() -> &'static [(FileId, Source)] {
    static SOURCES: OnceLock<Vec<(FileId, Source)>> = OnceLock::new();
    SOURCES.get_or_init(|| {
        [
            ("/mitex/mod.typ", include_str!("../mitex/mod.typ")),
            ("/mitex/prelude.typ", include_str!("../mitex/prelude.typ")),
            (
                "/mitex/latex/standard.typ",
                include_str!("../mitex/latex/standard.typ"),
            ),
            ("/mitex/compat.typ", include_str!("../mitex/compat.typ")),
        ]
        .into_iter()
        .map(|(path, text)| {
            let vpath = VirtualPath::new(path).expect("static mitex vpath");
            let id = FileId::new(RootedPath::new(VirtualRoot::Project, vpath));
            (id, Source::new(id, text.to_string()))
        })
        .collect()
    })
}

pub(super) struct MathWorld {
    assets: &'static SharedAssets,
    main: FileId,
    source: Source,
}

impl MathWorld {
    pub(super) fn detached(source: String) -> Self {
        let source = Source::detached(source);
        Self {
            assets: shared_assets(),
            main: source.id(),
            source,
        }
    }
}

impl World for MathWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.assets.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.assets.book
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> Result<Source, FileError> {
        if id == self.main {
            return Ok(self.source.clone());
        }
        mitex_scope_sources()
            .iter()
            .find(|(fid, _)| *fid == id)
            .map(|(_, src)| src.clone())
            .ok_or_else(|| FileError::NotFound(PathBuf::new()))
    }

    fn file(&self, id: FileId) -> Result<Bytes, FileError> {
        self.source(id)
            .map(|src| Bytes::from_string(src.text().to_string()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.assets.fonts.font(index)
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

pub(super) fn describe(diagnostics: &[SourceDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(|d| d.message.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}
