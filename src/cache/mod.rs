pub mod file;
use std::{borrow::Cow, convert::TryFrom, error::Error as StdError};

use file::FileCache;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    cli::{CacheMode, GlobalOpts},
    disson::map,
    error::prelude::*,
};

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
                $name(&'a $ty),
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
        #[derive(Debug, Serialize)]
        pub enum CacheKey<'a> { $($body)* }

        $($impls)*
    };

    (@process_body Value {} { $($body:tt)* } { $($impls:item)* }) => {
        #[derive(Debug, Serialize, Deserialize)]
        pub enum CacheValue<'a> { $($body)* }

        $($impls)*
    };

    (@from_impl Key $var:ident $ty:ty) => {
        impl<'__a> ::std::convert::From<&'__a $ty> for CacheKey<'__a> {
            fn from(__v: &'__a $ty) -> Self { Self::$var(__v) }
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
            @try_into_impl_item '__a (CacheKey<'__a> => &'__a $ty)
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

pub trait Cache {
    fn read_impl(&self, key: &CacheKey) -> Result<CacheValue>;

    fn write_impl(&self, key: &CacheKey, val: &CacheValue) -> Result<()>;

    fn clean(&self) -> Result<()>;
}

pub trait CacheExt {
    fn read<
        'a,
        K: 'a,
        V: for<'b> TryFrom<CacheValue<'b>, Error = E>,
        E: 'static + StdError + Send + Sync,
    >(
        &self,
        key: K,
    ) -> Result<V>
    where
        CacheKey<'a>: From<K>;

    fn write<'a, K: 'a, V: 'a>(&self, key: K, val: V) -> Result<()>
    where
        CacheKey<'a>: From<K>,
        CacheValue<'a>: From<V>;
}

impl<T: Cache + ?Sized> CacheExt for T {
    fn read<
        'a,
        K: 'a,
        V: for<'b> TryFrom<CacheValue<'b>, Error = E>,
        E: 'static + StdError + Send + Sync,
    >(
        &self,
        key: K,
    ) -> Result<V>
    where
        CacheKey<'a>: From<K>,
    {
        V::try_from(self.read_impl(&key.into())?).context("failed to unpack cache value")
    }

    fn write<'a, K: 'a, V: 'a>(&self, key: K, val: V) -> Result<()>
    where
        CacheKey<'a>: From<K>,
        CacheValue<'a>: From<V>,
    {
        self.write_impl(&key.into(), &val.into())
    }
}

pub struct NullCache;

// TODO: remove type parameters in favor of enums
impl Cache for NullCache {
    fn read_impl(&self, _: &CacheKey) -> Result<CacheValue> { Err(anyhow!("caching is disabled")) }

    fn write_impl(&self, _: &CacheKey, _: &CacheValue) -> Result<()> { Ok(()) }

    fn clean(&self) -> Result<()> { Ok(()) }
}

pub fn from_opts(mode: CacheMode) -> Box<dyn Cache> {
    match mode {
        CacheMode::Off => Box::new(NullCache),
        CacheMode::File(d) => Box::new(FileCache(d)),
    }
}

pub fn clean(cache_mode: CacheMode) -> Result<()> { from_opts(cache_mode).clean() }
