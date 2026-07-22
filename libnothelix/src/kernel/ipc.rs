use crate::error::{Error, KernelFault, Result};
use serde_json::Value;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
pub(super) enum Artifact {
    Pid,
    Ready,
    Input,
    JsonOutput,
    JsonDone,
    MsgpackOutput,
    MsgpackDone,
    Log,
}

impl Artifact {
    pub(super) const IN_FLIGHT: [Self; 5] = [
        Self::Input,
        Self::JsonOutput,
        Self::JsonDone,
        Self::MsgpackOutput,
        Self::MsgpackDone,
    ];
    pub(super) const SESSION: [Self; 7] = [
        Self::Pid,
        Self::Ready,
        Self::Input,
        Self::JsonOutput,
        Self::JsonDone,
        Self::MsgpackOutput,
        Self::MsgpackDone,
    ];
    pub(super) const SPENT: [Self; 6] = [
        Self::Input,
        Self::JsonOutput,
        Self::JsonDone,
        Self::MsgpackOutput,
        Self::MsgpackDone,
        Self::Ready,
    ];

    fn file_name(self) -> &'static str {
        match self {
            Self::Pid => "pid",
            Self::Ready => "ready",
            Self::Input => "input.json",
            Self::JsonOutput => "output.json",
            Self::JsonDone => "output.json.done",
            Self::MsgpackOutput => "output.msgpack",
            Self::MsgpackDone => "output.done",
            Self::Log => "kernel.log",
        }
    }
}

pub(super) enum Reply {
    Pending,
    Ready(Value),
}

enum Encoding {
    MsgPack,
    Json,
}

pub(super) struct KernelDir {
    path: PathBuf,
}

impl KernelDir {
    pub(super) fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn file(&self, artifact: Artifact) -> PathBuf {
        self.path.join(artifact.file_name())
    }

    pub(super) fn holds(&self, artifact: Artifact) -> bool {
        self.file(artifact).exists()
    }

    pub(super) fn fault(&self, fault: KernelFault) -> Error {
        Error::Kernel {
            directory: self.path.clone(),
            fault,
        }
    }

    pub(super) fn create(&self) -> Result<()> {
        fs::create_dir_all(&self.path).map_err(|e| Error::creating(&self.path, e))
    }

    pub(super) fn discard(&self, artifacts: &[Artifact]) -> Result<()> {
        for artifact in artifacts {
            let path = self.file(*artifact);
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => return Err(Error::removing(path, e)),
            }
        }
        Ok(())
    }

    pub(super) fn recorded_pid(&self) -> Option<u32> {
        fs::read_to_string(self.file(Artifact::Pid))
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    pub(super) fn record_pid(&self, pid: u32) -> Result<()> {
        let path = self.file(Artifact::Pid);
        fs::write(&path, pid.to_string()).map_err(|e| Error::writing(path, e))
    }

    pub(super) fn send(&self, command: &Value) -> Result<()> {
        self.discard(&[Artifact::JsonDone, Artifact::MsgpackDone])?;
        let path = self.file(Artifact::Input);
        fs::write(&path, command.to_string()).map_err(|e| Error::writing(path, e))
    }

    pub(super) fn collect(&self) -> Result<Reply> {
        let (done, output, encoding) = if self.holds(Artifact::MsgpackDone) {
            (
                Artifact::MsgpackDone,
                Artifact::MsgpackOutput,
                Encoding::MsgPack,
            )
        } else if self.holds(Artifact::JsonDone) {
            (Artifact::JsonDone, Artifact::JsonOutput, Encoding::Json)
        } else {
            return Ok(Reply::Pending);
        };
        self.discard(&[done])?;

        let path = self.file(output);
        let bytes = fs::read(&path).map_err(|e| Error::reading(path, e))?;
        let malformed = |detail: String| self.fault(KernelFault::MalformedReply { detail });
        let parsed = match encoding {
            Encoding::MsgPack => {
                rmp_serde::from_slice(&bytes).map_err(|e| malformed(e.to_string()))?
            }
            Encoding::Json => {
                serde_json::from_slice(&bytes).map_err(|e| malformed(e.to_string()))?
            }
        };
        Ok(Reply::Ready(parsed))
    }
}
