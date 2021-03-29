use std::sync::atomic::{AtomicBool, Ordering};

use thiserror::Error;

pub mod prelude {
    pub use super::{CancelToken, CancelResult, CancelError::Cancelled};
}

#[derive(Debug, Error)]
pub enum CancelError {
    #[error("Operation cancelled")]
    Cancelled,
    #[error("{0}")]
    Failed(#[from] crate::error::Error),
}

impl CancelError {
    pub fn into_result(self) -> Result<(), crate::error::Error> { self.into() }
}

impl From<CancelError> for Result<(), crate::error::Error> {
    fn from(err: CancelError) -> Self {
        match err {
            CancelError::Cancelled => Ok(()),
            CancelError::Failed(e) => Err(e),
        }
    }
}

pub type CancelResult<T> = Result<T, CancelError>;

pub struct CancelToken(AtomicBool);

impl CancelToken {
    pub fn new() -> Self { Self(AtomicBool::new(false)) }

    pub fn set(&self) { self.0.store(true, Ordering::SeqCst); }

    #[inline]
    fn try_impl(&self, ord: Ordering) -> CancelResult<()> {
        if self.0.load(ord) {
            Err(CancelError::Cancelled)
        } else {
            Ok(())
        }
    }

    pub fn try_weak(&self) -> CancelResult<()> { self.try_impl(Ordering::Relaxed) }

    pub fn try_strong(&self) -> CancelResult<()> { self.try_impl(Ordering::SeqCst) }
}
