use std::fmt;
use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

pub const FFI_ERROR_PREFIX: &str = "ERROR: ";

pub fn ffi(result: Result<String>) -> String {
    match result {
        Ok(value) => value,
        Err(error) => format!("{FFI_ERROR_PREFIX}{error}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAction {
    Read,
    Write,
    Create,
    Remove,
    Resolve,
}

impl fmt::Display for FileAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Create => "create",
            Self::Remove => "remove",
            Self::Resolve => "resolve",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderStage {
    LatexConversion,
    TypstCompile,
    PdfExport,
    SvgParse,
    Rasterize,
}

impl fmt::Display for RenderStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::LatexConversion => "latex conversion",
            Self::TypstCompile => "typst compile",
            Self::PdfExport => "pdf export",
            Self::SvgParse => "svg parse",
            Self::Rasterize => "rasterize",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelFault {
    NoPidFile,
    ProcessNotAlive { pid: u32 },
    NotReady,
    NoReply,
    MalformedReply { detail: String },
    InterpreterMissing { name: String },
}

impl fmt::Display for KernelFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoPidFile => f.write_str("no pid file"),
            Self::ProcessNotAlive { pid } => write!(f, "process {pid} is not alive"),
            Self::NotReady => f.write_str("kernel not ready"),
            Self::NoReply => f.write_str("no reply from runner"),
            Self::MalformedReply { detail } => write!(f, "malformed reply: {detail}"),
            Self::InterpreterMissing { name } => write!(f, "{name} not found in PATH"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot {action} {}: {source}", path.display())]
    File {
        action: FileAction,
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("{} has no parent directory", path.display())]
    OrphanPath { path: PathBuf },

    #[error("{} does not exist", path.display())]
    AbsentPath { path: PathBuf },

    #[error("{subject}: invalid base64 ({length} bytes): {source}")]
    Base64 {
        subject: &'static str,
        length: usize,
        source: base64::DecodeError,
    },

    #[error("{subject}: invalid JSON: {source}")]
    Json {
        subject: &'static str,
        source: serde_json::Error,
    },

    #[error("{subject}: invalid TOML: {source}")]
    Toml {
        subject: &'static str,
        source: toml::de::Error,
    },

    #[error("{subject}: {detail}")]
    Malformed {
        subject: &'static str,
        detail: String,
    },

    #[cfg(feature = "native")]
    #[error("cannot decode image ({length} bytes): {source}")]
    ImageDecode {
        length: usize,
        source: ::image::ImageError,
    },

    #[cfg(feature = "native")]
    #[error("cannot encode image as {format}: {source}")]
    ImageEncode {
        format: &'static str,
        source: ::image::ImageError,
    },

    #[cfg(feature = "native")]
    #[error(transparent)]
    Decoder(#[from] crate::animation::decoder::DecoderError),

    #[error("{stage} failed for {subject}: {detail}")]
    Render {
        stage: RenderStage,
        subject: String,
        detail: String,
    },

    #[error("`{command}` failed: {detail}")]
    Subprocess { command: String, detail: String },

    #[error("kernel at {}: {fault}", directory.display())]
    Kernel {
        directory: PathBuf,
        fault: KernelFault,
    },

    #[error("{subject}: lock poisoned")]
    LockPoisoned { subject: &'static str },
}

impl Error {
    pub fn reading(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::File {
            action: FileAction::Read,
            path: path.into(),
            source,
        }
    }

    pub fn writing(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::File {
            action: FileAction::Write,
            path: path.into(),
            source,
        }
    }

    pub fn creating(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::File {
            action: FileAction::Create,
            path: path.into(),
            source,
        }
    }

    pub fn removing(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::File {
            action: FileAction::Remove,
            path: path.into(),
            source,
        }
    }

    pub fn resolving(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::File {
            action: FileAction::Resolve,
            path: path.into(),
            source,
        }
    }

    pub fn orphan(path: impl Into<PathBuf>) -> Self {
        Self::OrphanPath { path: path.into() }
    }

    pub fn absent(path: impl Into<PathBuf>) -> Self {
        Self::AbsentPath { path: path.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, FFI_ERROR_PREFIX, KernelFault, RenderStage, ffi};
    use std::io::{Error as IoError, ErrorKind};

    #[test]
    fn ffi_passes_success_through_untouched() {
        assert_eq!(ffi(Ok("payload".to_string())), "payload");
    }

    #[test]
    fn ffi_renders_failures_with_the_prefix_the_plugin_matches() {
        let rendered = ffi(Err(Error::writing(
            "/tmp/notebook.jl",
            IoError::new(ErrorKind::PermissionDenied, "denied"),
        )));
        assert!(rendered.starts_with("ERROR:"), "{rendered}");
        assert!(rendered.starts_with(FFI_ERROR_PREFIX), "{rendered}");
        assert_eq!(
            rendered,
            "ERROR: cannot write /tmp/notebook.jl: denied".to_string()
        );
    }

    #[test]
    fn kernel_failures_name_the_directory_and_the_fault() {
        let rendered = ffi(Err(Error::Kernel {
            directory: "/w/.nothelix/kernel".into(),
            fault: KernelFault::ProcessNotAlive { pid: 42 },
        }));
        assert_eq!(
            rendered,
            "ERROR: kernel at /w/.nothelix/kernel: process 42 is not alive"
        );
    }

    #[test]
    fn render_failures_name_the_stage_and_the_input() {
        let rendered = ffi(Err(Error::Render {
            stage: RenderStage::TypstCompile,
            subject: "\\frac{1}{2}".to_string(),
            detail: "unknown variable".to_string(),
        }));
        assert_eq!(
            rendered,
            "ERROR: typst compile failed for \\frac{1}{2}: unknown variable"
        );
    }
}
