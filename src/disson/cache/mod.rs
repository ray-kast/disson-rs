pub mod file;
use std::{
    borrow::Cow,
    convert::{TryFrom, TryInto},
    error::Error as StdError,
    ops::{Deref, DerefMut},
};

use file::{FileCache, FileCacheEntry};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    cli::{CacheMode, GlobalOpts},
    disson::map,
    error::prelude::*,
};

pub mod prelude {
    pub use super::{Cache, CacheEntry, CacheEntryExt, CacheExt};
}

#[derive(Debug, Error)]
#[error("failed to unwrap cache {0}, expected {1}")]
pub struct ConvertError(&'static str, &'static str);

macro_rules! cache_enum {
    () => ();

    (enum $name:ident $body:tt $($rest:tt)*) => {
        cache_enum!(@process_body $name, $body);
        cache_enum!($($rest)*);
    };

    (@process_body $name:ident, $body:tt) => {
        cache_enum!(@process_body $name $body {} {});
    };

    (@process_body Key
        { $name:ident($ty:ty) $(, $($rest:tt)*)? }
        { $($out:tt)* }
        { $($impls:item)* }
    ) => {
        cache_enum!(@process_body Key
            { $($($rest)*)? }
            {
                $($out)*
                $name($ty),
            }
            {
                $($impls)*
                cache_enum!(@from_impl Key $name $ty);
                cache_enum!(@try_into_impl Key $name $ty);
            }
        );
    };

    (@process_body Value
        { $name:ident($ty:ty) $(, $($rest:tt)*)? }
        { $($out:tt)* }
        { $($impls:item)* }
    ) => {
        cache_enum!(@process_body Value
            { $($($rest)*)? }
            {
                $($out)*
                $name(Cow<'a, $ty>),
            }
            {
                $($impls)*
                cache_enum!(@from_impl Value $name $ty);
                cache_enum!(@try_into_impl Value $name $ty);
            }
        );
    };

    (@process_body Key {} { $($body:tt)* } { $($impls:item)* }) => {
        #[derive(Debug, Clone, Serialize)]
        pub enum CacheKey { $($body)* }

        $($impls)*
    };

    (@process_body Value {} { $($body:tt)* } { $($impls:item)* }) => {
        #[derive(Debug, Serialize, Deserialize)]
        pub enum CacheValue<'a> { $($body)* }

        $($impls)*
    };

    (@from_impl Key $var:ident $ty:ty) => {
        impl<'__a> ::std::convert::From<$ty> for CacheKey {
            fn from(__v: $ty) -> Self { Self::$var(__v) }
        }
    };

    (@from_impl Value $var:ident $ty:ty) => {
        cache_enum! {
            @from_impl_item '__a (::std::borrow::Cow<'__a, $ty> => CacheValue<'__a>)
            |__v| Self::$var(__v)
        }

        cache_enum! {
            @from_impl_item '__a ($ty => CacheValue<'__a>)
            |__v| Self::$var(::std::borrow::Cow::Owned(__v))
        }

        cache_enum! {
            @from_impl_item '__a (&'__a $ty => CacheValue<'__a>)
            |__v| Self::$var(::std::borrow::Cow::Borrowed(__v))
        }
    };

    (@try_into_impl Key $var:ident $ty:ty) => {
        cache_enum! {
            @try_into_impl_item (CacheKey => $ty)
            |__v| match __v {
                CacheKey::$var(__v) => Ok(__v),
                #[allow(unreachable_patterns)]
                _ => Err(ConvertError("key", stringify!($var))),
            }
        }
    };

    (@try_into_impl Value $var:ident $ty:ty) => {
        cache_enum! {
            @try_into_impl_item '__a (CacheValue<'__a> => ::std::borrow::Cow<'__a, $ty>)
            |__v| match __v {
                CacheValue::$var(__v) => Ok(__v),
                #[allow(unreachable_patterns)]
                _ => Err(ConvertError("value", stringify!($var))),
            }
        }

        cache_enum! {
            @try_into_impl_item '__a (CacheValue<'__a> => $ty)
            |__v| match __v {
                CacheValue::$var(__v) => Ok(__v.into_owned()),
                #[allow(unreachable_patterns)]
                _ => Err(ConvertError("value", stringify!($var))),
            }
        }
    };

    (@from_impl_item $($lt:lifetime)? ($ty:ty => $name:ty) |$val:ident| $body:expr) => {
        impl $(<$lt>)? ::std::convert::From<$ty> for $name {
            fn from($val: $ty) -> Self { $body }
        }
    };

    (
        @try_into_impl_item $($lt:lifetime)? ($name:ty => $ty:ty)
        |$val:ident| $body:expr
    ) => {
        impl $(<$lt>)? ::std::convert::TryFrom<$name> for $ty {
            type Error = self::ConvertError;

            fn try_from($val: $name) -> ::std::result::Result<Self, Self::Error> { $body }
        }
    }
}

cache_enum! {
    enum Key {
        Map(map::CacheKey),
    }

    enum Value {
        Map(map::CacheValue),
    }
}

pub trait Cache<'a>: Send + Sync {
    type Entry: CacheEntry + 'a;

    fn entry_impl(&'a self, key: CacheKey) -> Result<Self::Entry>;

    fn clean(&self) -> Result<()>;
}

impl<'a, T: Cache<'a> + ?Sized + 'a, U: Deref<Target = T> + Send + Sync> Cache<'a> for U {
    type Entry = T::Entry;

    fn entry_impl(&'a self, key: CacheKey) -> Result<Self::Entry> {
        (<Self as Deref>::deref(self) as &T).entry_impl(key)
    }

    fn clean(&self) -> Result<()> { (<Self as Deref>::deref(self) as &T).clean() }
}

pub trait CacheEntry {
    fn read_impl(&mut self) -> Result<Vec<CacheValue<'static>>>;

    fn write_impl(&mut self, val: &CacheValue) -> Result<()>;
}

impl<T: CacheEntry + ?Sized, U: Deref<Target = T> + DerefMut> CacheEntry for U {
    fn read_impl(&mut self) -> Result<Vec<CacheValue<'static>>> {
        (<Self as DerefMut>::deref_mut(self) as &mut T).read_impl()
    }

    fn write_impl(&mut self, val: &CacheValue) -> Result<()> {
        (<Self as DerefMut>::deref_mut(self) as &mut T).write_impl(val)
    }
}

pub trait CacheExt<'a>: Cache<'a> {
    fn entry<K: 'a + Into<CacheKey>>(&'a self, key: K) -> Result<Self::Entry>;
}

impl<'a, T: Cache<'a> + ?Sized> CacheExt<'a> for T {
    fn entry<K: 'a + Into<CacheKey>>(&'a self, key: K) -> Result<Self::Entry> {
        self.entry_impl(key.into())
    }
}

pub trait CacheEntryExt<'a>: CacheEntry {
    fn read<V: for<'v> TryFrom<CacheValue<'v>, Error = E>, E: 'static + StdError + Send + Sync>(
        &'a mut self,
    ) -> Result<Vec<V>>;

    fn write<V: Into<CacheValue<'static>>>(&'a mut self, val: V) -> Result<()>;
}

impl<'a, T: CacheEntry + ?Sized + 'a> CacheEntryExt<'a> for T {
    fn read<V: for<'v> TryFrom<CacheValue<'v>, Error = E>, E: 'static + StdError + Send + Sync>(
        &'a mut self,
    ) -> Result<Vec<V>> {
        self.read_impl().and_then(|v| {
            v.into_iter()
                .map(|v| v.try_into().context("failed to unpack cache value"))
                .collect()
        })
    }

    fn write<V: Into<CacheValue<'static>>>(&'a mut self, val: V) -> Result<()> {
        self.write_impl(&val.into())
    }
}

pub struct NullCache;

impl<'a> Cache<'a> for NullCache {
    type Entry = NullCache;

    fn entry_impl(&'a self, _: CacheKey) -> Result<Self::Entry> { Ok(Self) }

    fn clean(&self) -> Result<()> { Ok(()) }
}

impl CacheEntry for NullCache {
    fn read_impl(&mut self) -> Result<Vec<CacheValue<'static>>> { Ok(vec![]) }

    fn write_impl(&mut self, _: &CacheValue) -> Result<()> { Ok(()) }
}

pub enum DynamicCache {
    File(FileCache),
    Null(NullCache),
}

pub enum DynamicCacheEntry<'a> {
    File(FileCacheEntry<'a>),
    Null(NullCache),
}

impl<'a> Cache<'a> for DynamicCache {
    type Entry = DynamicCacheEntry<'a>;

    fn entry_impl(&'a self, key: CacheKey) -> Result<Self::Entry> {
        Ok(match self {
            Self::File(f) => Self::Entry::File(f.entry(key)?),
            Self::Null(n) => Self::Entry::Null(n.entry(key)?),
        })
    }

    fn clean(&self) -> Result<()> {
        match self {
            Self::File(f) => f.clean(),
            Self::Null(n) => n.clean(),
        }
    }
}

impl<'a> CacheEntry for DynamicCacheEntry<'a> {
    fn read_impl(&mut self) -> Result<Vec<CacheValue<'static>>> {
        match self {
            Self::File(f) => f.read_impl(),
            Self::Null(n) => n.read_impl(),
        }
    }

    fn write_impl(&mut self, val: &CacheValue) -> Result<()> {
        match self {
            Self::File(f) => f.write_impl(val),
            Self::Null(n) => n.write_impl(val),
        }
    }
}

pub fn from_opts(mode: CacheMode) -> DynamicCache {
    match mode {
        CacheMode::Off => DynamicCache::Null(NullCache),
        CacheMode::File(d) => DynamicCache::File(FileCache(d)),
    }
}

pub fn clean(cache_mode: CacheMode) -> Result<()> { from_opts(cache_mode).clean() }
