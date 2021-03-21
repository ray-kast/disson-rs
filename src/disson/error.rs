pub mod prelude {
    pub use anyhow::{anyhow, Context};

    pub use super::{Error, Result};
}

pub type Error = anyhow::Error;
pub type Result<T, E = Error> = std::result::Result<T, E>;

pub mod cancel {
    use thiserror::Error;

    pub mod prelude {
        pub use super::{super::prelude::*, CancelError::Cancelled, CancelResult};
    }

    #[derive(Debug, Error)]
    pub enum CancelError {
        #[error("Operation cancelled")]
        Cancelled,
        #[error("{0}")]
        Failed(#[from] super::Error),
    }

    impl CancelError {
        pub fn into_result(self) -> Result<(), super::Error> { self.into() }
    }

    impl From<CancelError> for Result<(), super::Error> {
        fn from(err: CancelError) -> Self {
            match err {
                CancelError::Cancelled => Ok(()),
                CancelError::Failed(e) => Err(e),
            }
        }
    }

    pub type CancelResult<T> = Result<T, CancelError>;
}
