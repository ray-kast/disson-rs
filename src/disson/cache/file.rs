use std::{
    convert::TryFrom,
    fs,
    fs::{DirBuilder, File, OpenOptions},
    io::{prelude::*, SeekFrom},
    marker::PhantomData,
    mem,
    path::{Path, PathBuf},
};

use bincode::Options;
use fs2::FileExt;
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
    Unopened {
        path: PathBuf,
        key_bytes: Vec<u8>,
    },
    Open {
        file: File,
        header_len: usize,
    },
    Streaming {
        stream: zstd::Encoder<'static, File>,
        header_len: usize,
    },
    Closed,
}

impl Default for Entry {
    fn default() -> Self { Self::Closed }
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
            Entry::Unopened {
                path: cache_dir.join(dir).join(file),
                key_bytes,
            },
            PhantomData,
        ))
    }

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

fn open_file(path: impl AsRef<Path>, key_bytes: &[u8]) -> Result<(File, usize)> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(false)
        .open(path)
        .context("failed to open file")?;

    file.try_lock_exclusive()
        .context("failed to acquire file lock")?;

    let header_len = check_header(&mut file, &key_bytes).context("failed to check file header")?;

    Ok((file, header_len))
}

fn create_file(path: impl AsRef<Path>, key_bytes: &[u8]) -> Result<(File, usize)> {
    DirBuilder::new()
        .recursive(true)
        .create(path.as_ref().parent().unwrap())
        .context("failed to create cache (sub)directory")?;

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .context("failed to create file")?;

    file.try_lock_exclusive()
        .context("failed to acquire file lock")?;

    let header_len = write_header(&mut file, &key_bytes).context("failed to write file header")?;

    Ok((file, header_len))
}

fn check_header(file: &mut File, key_bytes: &[u8]) -> Result<usize> {
    let magic = magic();
    let mut file_magic = vec![0_u8; magic.len()];

    file.read_exact(file_magic.as_mut())
        .context("failed to read cache magic number")?;

    if file_magic != magic {
        return Err(anyhow!(
            "cache magic number mismatch (possibly a version change?)"
        ));
    }

    let mut file_key_bytes = vec![0_u8; key_bytes.len()];

    file.read_exact(file_key_bytes.as_mut())
        .context("failed to read cache key")?;

    if key_bytes != file_key_bytes {
        return Err(anyhow!("cache key mismatch (this shouldn't happen)"));
    }

    Ok(magic.len() + key_bytes.len())
}

fn write_header(file: &mut File, key_bytes: &[u8]) -> Result<usize> {
    let magic = magic();

    file.write_all(magic.as_ref())
        .context("failed to write cache magic number")?;

    file.write_all(key_bytes.as_ref())
        .context("failed to write cache key")?;

    Ok(magic.len() + key_bytes.len())
}

fn is_at_eof(mut file: &File) -> Result<(bool, u64)> {
    let pos = file
        .stream_position()
        .context("failed to get file position")?;
    let len = file
        .seek(SeekFrom::End(0))
        .context("failed to get file length")?;
    let eof = pos == len;

    if !eof {
        file.seek(SeekFrom::Start(pos))
            .context("failed to restore file position")?;
    }

    Ok((eof, pos))
}

enum Block {
    /// A block was successfully read, and more blocks may be available
    Good(Vec<CacheValue<'static>>),
    /// A block may have been partially read, but the rest of the file is not
    /// recoverable - the file should be truncated to the length given and the
    /// data should be recovered
    Corrupt(Vec<CacheValue<'static>>, u64),
    /// No more blocks are available
    Eof,
}

fn read_block(file: &File) -> Block {
    let pos = match is_at_eof(file) {
        Ok((true, _)) => return Block::Eof,
        Ok((false, p)) => p,
        Err(e) => {
            warn!("Failed to check for end of cache file: {:?}", e);
            return Block::Eof; // Can't return Corrupt because it requires pos
        },
    };

    let mut dec = match zstd::Decoder::new(file) {
        Ok(d) => d,
        Err(e) => {
            warn!("Failed to open zstd decoder on cache file: {:?}", e);
            return Block::Corrupt(vec![], pos);
        },
    };

    let mut ret = vec![];

    loop {
        match val_bin_opts().deserialize_from(&mut dec) {
            Ok(Some(val)) => ret.push(val),
            Ok(None) => return Block::Good(ret),
            Err(e) => {
                // TODO: either mark the file as partially unreadable or
                // attempt to stream a corrupted block back into the file

                warn!("Failed to read cache value: {:?}", e);

                return Block::Corrupt(ret, pos);
            },
        }
    }
}

fn make_stream(file: File) -> Result<zstd::Encoder<'static, File>> {
    zstd::Encoder::new(file, 0).context("failed to open zstd encoder on cache file")
}

impl<'a> CacheEntry for FileCacheEntry<'a> {
    fn read_impl(&mut self) -> Vec<CacheValue<'static>> {
        fn recover(
            mut file: File,
            pos: u64,
            blk: &[CacheValue],
        ) -> Result<zstd::Encoder<'static, File>> {
            file.set_len(pos).context("failed to truncate file")?;

            file.seek(SeekFrom::End(0))
                .context("failed to seek to end-of-file")?;

            let mut stream = make_stream(file)?;

            for val in blk {
                val_bin_opts()
                    .serialize_into(&mut stream, &Some(val))
                    .context("failed to write recovered value")?;
            }

            Ok(stream)
        }

        self.0 = match mem::take(&mut self.0) {
            Entry::Unopened { path, key_bytes } => match open_file(&path, &key_bytes) {
                Ok((file, header_len)) => Entry::Open { file, header_len },
                Err(e) => {
                    warn!("Failed to open cache file: {:?}", e);

                    Entry::Unopened { path, key_bytes }
                },
            },
            e @ Entry::Open { .. } | e @ Entry::Streaming { .. } => e,
            Entry::Closed => unreachable!("Attempted to read from dropped entry"),
        };

        if let Entry::Open { ref mut file, .. } = self.0 {
            let mut ret = vec![];

            let recover_from = loop {
                match read_block(file) {
                    Block::Good(mut b) => ret.append(&mut b),
                    Block::Corrupt(mut b, p) => {
                        let i = ret.len();
                        ret.append(&mut b);
                        break Some((p, &ret[i..]));
                    },
                    Block::Eof => break None,
                }
            };

            if let Some((pos, blk)) = recover_from {
                if let Entry::Open { file, header_len } = mem::take(&mut self.0) {
                    match recover(file, pos, blk) {
                        Ok(stream) => self.0 = Entry::Streaming { stream, header_len },
                        Err(e) => {
                            warn!("Failed to recover corrupted cache block: {:?}", e);
                        },
                    }
                }
            }

            ret
        } else {
            debug_assert!(!matches!(self.0, Entry::Closed));

            vec![]
        }
    }

    #[allow(clippy::shadow_unrelated)] // TODO: ?????
    fn append_impl(&mut self, val: &CacheValue) -> Result<()> {
        self.0 = match mem::take(&mut self.0) {
            Entry::Unopened { path, key_bytes } => {
                let (file, header_len) = create_file(path, &key_bytes)?;

                Entry::Streaming {
                    stream: make_stream(file)?,
                    header_len,
                }
            },
            Entry::Open { file, header_len } => Entry::Streaming {
                stream: make_stream(file)?,
                header_len,
            },
            e @ Entry::Streaming { .. } => e,
            Entry::Closed => unreachable!("Attempted to write to dropped entry"),
        };

        if let Entry::Streaming { ref mut stream, .. } = self.0 {
            val_bin_opts()
                .serialize_into(stream, &Some(val))
                .context("failed to write cache value")?;
        } else {
            unreachable!();
        }

        Ok(())
    }

    fn truncate(&mut self) -> Result<()> {
        self.0 = match mem::take(&mut self.0) {
            Entry::Unopened { path, key_bytes } => {
                let (file, header_len) = create_file(path, &key_bytes)?;

                Entry::Open { file, header_len }
            },
            e @ Entry::Open { .. } => e,
            Entry::Streaming { stream, header_len } => {
                warn!("Truncating cache file that was open for streaming - this is wasteful!");

                Entry::Open {
                    file: stream.finish().context("failed to close zstd encoder")?,
                    header_len,
                }
            },
            Entry::Closed => unreachable!("Attempted to truncate dropped entry"),
        };

        if let Entry::Open {
            ref mut file,
            header_len,
        } = self.0
        {
            file.set_len(header_len as u64)
                .context("failed to truncate file")?;

            file.seek(SeekFrom::End(0))
                .context("failed to seek to end-of-file")?;
        } else {
            unreachable!();
        }

        Ok(())
    }
}

impl<'a> Drop for FileCacheEntry<'a> {
    fn drop(&mut self) {
        if let Entry::Streaming { mut stream, .. } = mem::take(&mut self.0) {
            match val_bin_opts()
                .serialize_into(&mut stream, &None::<CacheValue<'static>>)
                .context("failed to serialize sentinel")
                .and_then(|()| stream.finish().context("failed to close zstd encoder"))
            {
                Ok(_) => (),
                Err(e) => error!("Failed to write cache block sentinel: {:?}", e),
            }
        }
    }
}
