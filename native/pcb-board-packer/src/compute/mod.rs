//! Internal GPU infrastructure in the existing addon. No solver dependencies.
#[cfg(feature = "gpu")]
pub(crate) mod gpu;
#[cfg(feature = "gpu")]
mod queue;
#[cfg(feature = "gpu")]
pub(crate) mod f32_runtime;
#[cfg(feature = "gpu")]
pub(crate) use crate::numerics;
#[cfg(feature = "gpu")]
mod workspace;
#[cfg(feature = "gpu")]
pub(crate) use workspace::ScratchKey;

#[cfg(feature = "gpu")]
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub(crate) struct Capabilities {
    pub f32: bool,
    pub u64: bool,
}

#[cfg(feature = "gpu")]
#[derive(Clone, Copy)]
pub(crate) struct Requirements {
    pub f32: bool,
    pub u64: bool,
}

#[cfg(feature = "gpu")]
impl Capabilities {
    fn check(self, required: Requirements) -> Result<(), Error> {
        if (required.f32 && !self.f32) || (required.u64 && !self.u64) {
            Err(Error::new(
                ErrorKind::MissingCapabilities,
                "GPU lacks requested capabilities",
            ))
        } else {
            Ok(())
        }
    }
}

#[cfg(feature = "gpu")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ErrorKind {
    Busy,
    Disabled,
    NoDevice,
    MissingCapabilities,
    Lease,
    AdapterMismatch,
    ArithmeticIncompatibility,
    RuntimeFailure,
    InvalidInput,
}

#[cfg(feature = "gpu")]
#[derive(Clone, Debug)]
pub(crate) struct Error {
    pub kind: ErrorKind,
    message: String,
}

#[cfg(feature = "gpu")]
impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    fn disables_runtime(&self) -> bool {
        !matches!(
            self.kind,
            ErrorKind::Busy | ErrorKind::InvalidInput | ErrorKind::MissingCapabilities
        )
    }
}

#[cfg(feature = "gpu")]
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}

#[cfg(feature = "gpu")]
impl std::error::Error for Error {}

#[cfg(feature = "gpu")]
impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::new(ErrorKind::RuntimeFailure, message)
    }
}

#[cfg(feature = "gpu")]
impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self::new(ErrorKind::RuntimeFailure, message)
    }
}

#[cfg(all(test, feature = "gpu"))]
mod tests {
    use super::*;

    #[test]
    fn missing_consumer_capability_is_a_rejection_not_a_runtime_failure() {
        let required = Requirements {
            f32: true,
            u64: true,
        };
        for available in [
            Capabilities {
                f32: false,
                u64: true,
            },
            Capabilities {
                f32: true,
                u64: false,
            },
        ] {
            let error = available.check(required).unwrap_err();
            assert_eq!(error.kind, ErrorKind::MissingCapabilities);
            assert!(!error.disables_runtime());
        }
        Capabilities {
            f32: true,
            u64: false,
        }
        .check(Requirements {
            f32: true,
            u64: false,
        })
        .unwrap();
        assert!(Error::new(ErrorKind::RuntimeFailure, "failed readback").disables_runtime());
    }
}
