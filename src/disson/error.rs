pub mod prelude {
    pub use anyhow::{anyhow, Context};

    pub use super::{Error, Result};
}

pub type Error = anyhow::Error;
pub type Result<T, E = Error> = std::result::Result<T, E>;

