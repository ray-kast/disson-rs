use std::{
    convert::TryFrom,
    fs,
    fs::{DirBuilder, File},
    io::prelude::*,
    marker::PhantomData,
    path::PathBuf,
};

use bincode::Options;
use log::{error, info, warn};
use sha2::{Digest, Sha256};

use super::{Cache, CacheEntry, CacheKey, CacheValue};
use crate::error::prelude::*;

const GLOBAL_MAGIC: &str = "\x00diss";

fn magic() -> Vec<u8> {
    let ver = env!("CARGO_PKG_VERSION").as_bytes();

    let mut out = vec![];

    out.write_all(GLOBAL_MAGIC.as_ref()).unwrap();
    out.write_all(&(u8::try_from(ver.len()).unwrap().to_le_bytes()))
        .unwrap();
    out.write_all(ver).unwrap();

    out
}

fn file_name(hash: impl AsRef<[u8]>) -> (PathBuf, PathBuf) {
    let hash = hash.as_ref();
    (
        format!("{:02x}", hash[0]).into(),
        hash[1..]
            .iter()
            .map(|b| format!("{:02x}", b))
            .fold(String::new(), |mut s, h| {
                s.push_str(&h);
                s
            })
            .into(),
    )
}

fn key_bin_opts() -> impl bincode::Options {
    bincode::options()
        .with_varint_encoding()
        .reject_trailing_bytes()
}

fn val_bin_opts() -> impl bincode::Options {
    bincode::options()
        .with_fixint_encoding()
        .reject_trailing_bytes()
}

pub struct FileCache(pub Option<PathBuf>);

pub struct FileCacheEntry<'a>(Entry, PhantomData<&'a FileCache>);

enum Entry {
    Unopened(PathBuf, CacheKey, Vec<u8>),
}

impl FileCache {
    fn locate_cache(&self) -> Result<PathBuf> {
        self.0
            .as_ref()
            .map_or_else(
                || dirs::cache_dir().map(|d| d.join("disson-rs")),
                |p| Some(p.clone()),
            )
            .ok_or_else(|| anyhow!("couldn't locate cache directory, please specify manually"))
    }
}

impl<'a> Cache<'a> for FileCache {
    type Entry = FileCacheEntry<'a>;

    fn entry_impl(&'a self, key: CacheKey) -> Result<Self::Entry> {
        let cache_dir = self.locate_cache()?;

        let key_bytes = key_bin_opts()
            .serialize(&key)
            .context("failed to serialize cache key")?;

        let mut hasher = Sha256::new();
        hasher.update(&key_bytes);
        let hash = hasher.finalize();

        let (dir, file) = file_name(hash);

        Ok(FileCacheEntry(
            Entry::Unopened(cache_dir.join(dir).join(file), key, key_bytes),
            PhantomData,
        ))
    }

    // fn read_impl(&self, key: &CacheKey) -> Result<CacheValue> {
    //     let cache_dir = self.locate_cache()?;

    //     let key_bytes = key_bin_opts()
    //         .serialize(key)
    //         .context("failed to serialize cache key")?;

    //     let mut hasher = Sha256::new();
    //     hasher.update(&key_bytes);
    //     let hash = hasher.finalize();

    //     let (dir, file) = file_name(hash);
    //     let mut file =
    //         File::open(cache_dir.join(dir).join(file)).context("failed to open
    // cache file")?;

    //     let magic = magic();
    //     let mut file_magic = vec![0_u8; magic.len()];

    //     file.read_exact(file_magic.as_mut())
    //         .context("failed to read cache header")?;

    //     if file_magic != magic {
    //         return Err(anyhow!(
    //             "cache header mismatch (possibly a version change?)"
    //         ));
    //     }

    //     let mut file = zstd::Decoder::new(file).context("failed to initialize
    // zstd decoder")?;

    //     let mut file_key_bytes = vec![0_u8; key_bytes.len()];

    //     file.read_exact(file_key_bytes.as_mut())
    //         .context("failed to read cache key")?;

    //     if key_bytes != file_key_bytes {
    //         return Err(anyhow!("cache key mismatch (shouldn't happen)"));
    //     }

    //     let val = val_bin_opts()
    //         .deserialize_from(file)
    //         .context("failed to read cache contents")?;

    //     Ok(val)
    // }

    // #[allow(clippy::shadow_unrelated)] // TODO: ?????
    // fn write_impl(&self, key: &CacheKey, val: &CacheValue) -> Result<()> {
    //     let cache_dir = self.locate_cache()?;

    //     let key_bytes = key_bin_opts()
    //         .serialize(key)
    //         .context("failed to serialize cache key")?;

    //     let mut hasher = Sha256::new();
    //     hasher.update(&key_bytes);
    //     let hash = hasher.finalize();

    //     let dir = cache_dir.join(format!("{:02x}", hash[0]));

    //     DirBuilder::new()
    //         .recursive(true)
    //         .create(&dir)
    //         .context("failed to create cache (sub)directory")?;

    //     let mut file = File::create(dir.join(
    //         PathBuf::from(hash[1..].iter().map(|b| format!("{:02x}", b)).fold(
    //             String::new(),
    //             |mut s, h| {
    //                 s.push_str(&h);
    //                 s
    //             },
    //         )),
    //     ))
    //     .context("failed to create cache file")?;

    //     file.write_all(magic().as_ref())
    //         .context("failed to write cache header")?;

    //     let mut file = zstd::Encoder::new(file, 0).context("failed to initialize
    // zstd encoder")?;

    //     file.write_all(key_bytes.as_ref())
    //         .context("failed to write cache key")?;

    //     val_bin_opts()
    //         .serialize_into(&mut file, val)
    //         .context("failed to write cache contents")?;

    //     let _file = file.finish()?;

    //     Ok(())
    // }

    fn clean(&self) -> Result<()> {
        enum QType {
            Explore,
            Delete,
        }

        let cache_dir = self.locate_cache()?;

        if !cache_dir.exists() {
            warn!("Cache directory doesn't exist, nothing to do.");

            return Ok(());
        }

        let mut magic_buf = vec![0_u8; GLOBAL_MAGIC.len()];
        let mut stack = vec![(QType::Explore, cache_dir)];

        while let Some((ty, dir)) = stack.pop() {
            if let QType::Explore = ty {
                stack.push((QType::Delete, dir.clone()));
            }

            let mut any = false;

            for entry in fs::read_dir(&dir)
                .with_context(|| format!("failed to open directory {:?}", dir.to_string_lossy()))?
            {
                any = true;

                if let QType::Delete = ty {
                    break;
                }

                let entry = entry.with_context(|| {
                    format!("failed to read from directory {:?}", dir.to_string_lossy())
                })?;
                let path = dir.join(entry.file_name());
                let ty = entry.file_type()?;

                if ty.is_file() {
                    let mut file = File::open(&path).with_context(|| {
                        format!(
                            "failed to open possible cache file {:?}",
                            path.to_string_lossy()
                        )
                    })?;

                    file.read_exact(magic_buf.as_mut()).with_context(|| {
                        format!(
                            "failed to check possible cache file {:?}",
                            path.to_string_lossy()
                        )
                    })?;

                    if magic_buf == GLOBAL_MAGIC.as_bytes() {
                        let s = path.to_string_lossy();

                        info!("Removing file {}...", s);

                        fs::remove_file(&path)
                            .with_context(|| format!("failed to delete cache file {:?}", s))?;
                    }
                } else if ty.is_dir() {
                    stack.push((QType::Explore, path));
                }
            }

            if !any {
                let s = dir.to_string_lossy();

                info!("Removing dir {}...", s);

                fs::remove_dir(&dir)
                    .with_context(|| format!("failed to delete empty directory {:?}", s))?;
            }
        }

        Ok(())
    }
}

impl<'a> CacheEntry for FileCacheEntry<'a> {
    fn read_impl(&mut self) -> Result<Vec<CacheValue<'static>>> {
        error!("Read not yet implemented");
        Ok(vec![])
    }

    fn write_impl(&mut self, _val: &CacheValue) -> Result<()> {
        error!("Write not yet implemented");
        Ok(())
    }
}