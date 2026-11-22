//! Library errors; Node status adaptation belongs to native/node-addon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind { InvalidInput, Failure }

#[derive(Debug)]
pub struct Error { pub kind: ErrorKind, pub message: String }

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into() }
    }
    pub(crate) fn failure(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Failure, message)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.message) }
}
impl std::error::Error for Error {}
