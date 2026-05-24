use std::fmt;
use std::io;

#[derive(Debug)]
pub enum AudioError {
    DeviceNotFound(String),
    FormatUnsupported(String),
    DecodeFailed(String),
    StreamError(String),
    InvalidHandle(String),
    Io(io::Error),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::DeviceNotFound(msg) => write!(f, "Audio device not found: {msg}"),
            AudioError::FormatUnsupported(msg) => write!(f, "Unsupported audio format: {msg}"),
            AudioError::DecodeFailed(msg) => write!(f, "Audio decode failed: {msg}"),
            AudioError::StreamError(msg) => write!(f, "Audio stream error: {msg}"),
            AudioError::InvalidHandle(msg) => write!(f, "Invalid audio handle: {msg}"),
            AudioError::Io(e) => write!(f, "IO error: {e}"),
        }
    }
}

impl std::error::Error for AudioError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AudioError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for AudioError {
    fn from(e: io::Error) -> Self {
        AudioError::Io(e)
    }
}
