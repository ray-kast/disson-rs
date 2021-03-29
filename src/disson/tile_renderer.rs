use std::collections::HashMap;

use backbuf::BackBuffer;
use log::trace;
use nalgebra::Vector2;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::cancel::prelude::*;

mod backbuf {
    use std::{mem, ptr, ptr::NonNull, slice, sync::RwLock};

    use dispose::{Disposable, Dispose};
    use nalgebra::Vector2;

    use super::TileRange;

    struct Slice<T: Sync>(NonNull<T>);
    struct Inner<T: Sync>(Vector2<usize>, RwLock<Slice<T>>);
    pub(super) struct BackBuffer<T: Sync>(Disposable<Inner<T>>);

    // Isolate the unsafe threading markers to get stronger static guarantees
    // from RwLock
    unsafe impl<T: Sync> Send for Slice<T> {}
    unsafe impl<T: Sync> Sync for Slice<T> {}

    impl<T: Default + Copy + Sync> BackBuffer<T> {
        pub fn new(size: Vector2<u32>) -> Self {
            let size = size.cast::<usize>();
            // TODO: eventually box literals will be a thing, I think...
            Self(Disposable::new(Inner(
                size,
                RwLock::new(Slice(
                    NonNull::new(
                        Box::leak(vec![Default::default(); size.x * size.y].into_boxed_slice())
                            .as_mut_ptr(),
                    )
                    .expect("back buffer slice was null"),
                )),
            )))
        }

        pub fn into_inner(self) -> Box<[T]> { unsafe { Disposable::leak(self.0).into_inner() } }

        /// This is sound if and only if you call it once for every element of a
        /// set of non-overlapping tile ranges.
        pub unsafe fn blit(&self, range: &TileRange, tile: impl AsRef<[T]>) {
            let this = self.0.as_ref();
            let tile = tile.as_ref();
            let TileRange { pos, size } = range;
            let pos = pos.cast::<usize>();
            let size = size.cast::<usize>();
            let end = pos + size;

            assert!(end.x <= this.0.x, "Tile X coordinate out-of-bounds");
            assert!(end.y <= this.0.y, "Tile Y coordinate out-of-bounds");
            assert_eq!(tile.len(), size.x * size.y, "Tile buffer size mismatch");

            let buf = this.1.read().expect("back buffer was poisoned");

            let mut buf_r = pos.y * this.0.x;
            for r in 0..size.y {
                let tile_i = r * size.x;
                let buf_i = buf_r + pos.x;
                slice::from_raw_parts_mut(buf.0.as_ptr().add(buf_i), size.x)
                    .copy_from_slice(tile.get_unchecked(tile_i..tile_i + size.x));

                buf_r += this.0.x;
            }
        }
    }

    impl<T: Sync> Inner<T> {
        pub fn into_inner(self) -> Box<[T]> {
            unsafe {
                Box::from_raw(ptr::slice_from_raw_parts_mut(
                    self.1
                        .into_inner()
                        .expect("back buffer was poisoned")
                        .0
                        .as_ptr(),
                    self.0.x * self.0.y,
                ))
            }
        }
    }

    impl<T: Sync> Dispose for Inner<T> {
        fn dispose(self) { mem::drop(self.into_inner()) }
    }
}

pub trait TileRenderFunction: Send + Sync {
    type Input;
    type Output: Copy + Default + Send + Sync;

    fn process(&self, tile: Tile<Self::Input, Self::Output>);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileRange {
    pub pos: Vector2<u32>,
    pub size: Vector2<u32>,
}

pub struct Tile<'a, I, O> {
    range: TileRange,
    in_stride: usize,
    buf_in: &'a [I],
    buf_out: &'a mut [O],
}

impl<'a, I, O> Tile<'a, I, O> {
    pub fn range(&self) -> &TileRange { &self.range }

    pub fn out(&self) -> &[O] { &self.buf_out }

    pub fn row_mut<'b>(&'b mut self, y: u32) -> (&'b [I], &'b mut [O])
    where 'a: 'b {
        let offs = self.range.pos.cast::<usize>();
        let y = y as usize;
        let row_len = self.range.size.x as usize;
        let out_stride = row_len;
        let in_i = offs.x + (y + offs.y) * self.in_stride;
        let out_i = y * out_stride;

        (
            &self.buf_in[in_i..in_i + row_len],
            &mut self.buf_out[out_i..out_i + row_len],
        )
    }
}

pub struct TileRenderer<F: Send + Sync, const TW: u32, const TH: u32>(F);

pub const DEFAULT_TILE_WIDTH: u32 = 128;
pub const DEFAULT_TILE_HEIGHT: u32 = 128;
pub type DefaultTileRenderer<F> = TileRenderer<F, DEFAULT_TILE_WIDTH, DEFAULT_TILE_HEIGHT>;

impl<F: TileRenderFunction, const TW: u32, const TH: u32> TileRenderer<F, TW, TH> {
    pub fn new(f: F) -> Self { Self(f) }

    pub fn run<
        I: AsRef<[F::Input]> + Sync,
        P: AsRef<[F::Output]> + Sync,
        C: std::borrow::Borrow<CancelToken> + Sync,
    >(
        &self,
        size: Vector2<u32>,
        buf_in: I,
        preload: &HashMap<TileRange, P>,
        cancel: C,
    ) -> CancelResult<Box<[F::Output]>> {
        assert_eq!(
            buf_in.as_ref().len(),
            size.x as usize * size.y as usize,
            "Input buffer size mismatch"
        );

        let tiles_x = size.x / TW + (size.x % TW).min(1);
        let tiles_y = size.y / TH + (size.y % TH).min(1);

        let mut tiles: Vec<_> = (0..tiles_x)
            .into_iter()
            .flat_map(|r| {
                (0..tiles_y).into_iter().map(move |c| {
                    let pos = Vector2::new(c * TW, r * TH);
                    let max = size - pos;
                    TileRange {
                        pos: Vector2::new(c * TW, r * TH),
                        size: Vector2::new(TW.min(max.x), TH.min(max.y)),
                    }
                })
            })
            .collect();

        let ctr = size / 2;
        let bbuf = BackBuffer::new(size);

        tiles.par_sort_by(|a, b| {
            let ca = a.pos + a.size / 2;
            let cb = b.pos + b.size / 2;

            let da = (ctr - ca).cast::<f64>().norm();
            let db = (ctr - cb).cast::<f64>().norm();

            da.partial_cmp(&db)
                .unwrap()
                .then_with(|| a.pos.y.cmp(&b.pos.y))
                .then_with(|| a.pos.x.cmp(&b.pos.x))
        });

        tiles
            .par_drain(..)
            .map(|range| {
                if let Some(out) = preload.get(&range) {
                    trace!("Preloading tile at {}", range.pos);

                    unsafe {
                        bbuf.blit(&range, out);
                    }
                } else {
                    // TODO: I could probably pool-allocate vectors, but IDK if
                    // that would actually help
                    let mut buf_out =
                        vec![Default::default(); range.size.x as usize * range.size.y as usize];

                    self.0.process(Tile {
                        range,
                        in_stride: size.x as usize,
                        buf_in: buf_in.as_ref(),
                        buf_out: buf_out.as_mut(),
                    });

                    unsafe {
                        bbuf.blit(&range, buf_out);
                    }
                }

                cancel.borrow().try_weak().ok()
            })
            .while_some()
            .for_each(|()| ());

        cancel.borrow().try_strong().map(|()| bbuf.into_inner())
    }
}
