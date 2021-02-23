pub mod file;

use file::FileCache;
use serde::{Deserialize, Serialize};

use crate::{
    cli::{CacheMode, GlobalOpts},
    error::prelude::*,
};

pub trait Cache<K, V> {
    fn read_checked(&self, key: &K, check: &dyn Fn(&K, &V) -> Result<()>) -> Result<V>;

    fn read(&self, key: &K) -> Result<V> { self.read_checked(key, &|_, _| Ok(())) }

    fn write(&self, key: &K, val: &V) -> Result<()>;

    fn clean(&self) -> Result<()>;
}

pub struct NullCache;

// TODO: remove type parameters in favor of enums
impl<K, V> Cache<K, V> for NullCache {
    fn read_checked(&self, _: &K, _: &dyn Fn(&K, &V) -> Result<()>) -> Result<V> {
        Err(anyhow!("caching is disabled"))
    }

    fn write(&self, _: &K, _: &V) -> Result<()> { Ok(()) }

    fn clean(&self) -> Result<()> { Ok(()) }
}

pub fn from_opts<K: Serialize, V: Serialize + for<'de> Deserialize<'de>>(
    mode: CacheMode,
) -> Box<dyn Cache<K, V>> {
    match mode {
        CacheMode::Off => Box::new(NullCache),
        CacheMode::File(d) => Box::new(FileCache(d)),
    }
}

pub fn clean(opts: GlobalOpts) -> Result<()> {
    let GlobalOpts { cache_mode } = opts;
    let cache = from_opts::<(), ()>(cache_mode);

    cache.clean()
}
