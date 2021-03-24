use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use itertools::Itertools;
use log::{trace, warn};
use nalgebra::{Point2, Transform2, Vector2};
use serde::{Deserialize, Serialize};

use super::{
    algo::{OverlapCurve, PitchCurve},
    wave::{Partial, Wave},
};
use crate::{
    cache::prelude::*,
    config::MapConfig,
    error::cancel::prelude::*,
    tile_renderer::{DefaultTileRenderer, Tile, TileRange, TileRenderFunction},
};

#[derive(Debug, Clone, Copy, Serialize)]
pub(super) struct Config {
    size: Vector2<u32>,
    view: Transform2<f64>,
    base_hz: f64,
    pitch: PitchCurve,
    overlap: OverlapCurve,
}

impl Config {
    pub fn for_generate(cfg: &MapConfig) -> Self {
        let MapConfig {
            width,
            height,
            base_frequency,
            pitch_curve,
            overlap_curve,
        } = *cfg;

        Self {
            size: Vector2::new(width, height),
            view: Transform2::identity(), // TODO
            base_hz: base_frequency,
            pitch: pitch_curve,
            overlap: overlap_curve,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CacheKey(Config);

pub(super) struct DissonMap {
    pub size: Vector2<u32>,
    pub data: Box<[f64]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CacheValue<'a> {
    Block(TileRange, Cow<'a, [f64]>),
    Histogram(()),
}

struct RenderFunction<'a, E: CacheEntry> {
    cache_entry: &'a Mutex<E>,
    pitch: PitchCurve,
    overlap: OverlapCurve,
    wave: Wave,
    base_wave: &'a Wave,
}

impl<'a, E: CacheEntry + Send> TileRenderFunction for RenderFunction<'a, E> {
    type Input = Point2<f64>;
    type Output = f64;

    fn process(&self, mut tile: Tile<Self::Input, Self::Output>) {
        for r in 0..tile.range().size.y {
            let (row_in, row_out) = tile.row_mut(r);

            for (ins, out) in row_in.iter().zip(row_out.iter_mut()) {
                let wave_x: Wave<_> = self
                    .pitch
                    .collect_partials(self.wave.map_pitch(|p| p * ins.x));

                let wave_y: Wave<_> = self
                    .pitch
                    .collect_partials(self.wave.map_pitch(|p| p * ins.y));

                let it = self
                    .base_wave
                    .iter()
                    .chain(wave_x.iter())
                    .chain(wave_y.iter());

                *out = self
                    .overlap
                    .collect_partials::<_, Vec<_>>(it.clone().cartesian_product(it))
                    .into_iter()
                    .sum::<f64>();
            }
        }

        // TODO: run this asynchronously
        match self
            .cache_entry
            .lock()
            .unwrap()
            .append(CacheValue::Block(*tile.range(), Cow::Borrowed(tile.out())))
        {
            Ok(()) => (),
            Err(e) => {
                warn!("Error caching tile {}: {:?}", tile.range().pos, e);
            },
        }
    }
}

pub(super) fn compute<C: for<'a> Cache<'a>>(
    cache: C,
    cfg: Config,
    cancel: &AtomicBool,
) -> CancelResult<DissonMap> {
    let mut cache_entry = cache
        .entry(CacheKey(cfg))
        .context("couldn't open cache entry")?;

    let Config {
        size,
        view,
        base_hz,
        pitch,
        overlap,
    } = cfg;

    let mut blk_preload = HashMap::new();
    let mut hist_preload = None;

    for val in cache_entry.read().context("couldn't read cache blocks")? {
        match val {
            CacheValue::Block(k, v) => {
                if blk_preload.insert(k, v).is_some() {
                    warn!(
                        "Multiple blocks at {} stored in map cache; taking latest",
                        k.pos
                    );
                }
            },
            CacheValue::Histogram(h) => {
                if hist_preload.is_some() {
                    warn!("Multiple histograms stored in map cache; taking latest");
                }

                hist_preload = Some(h);
            },
        }
    }

    trace!("Computing map inputs...");

    let pitches: Vec<_> = {
        let denom = (size - Vector2::new(1, 1)).cast::<f64>();

        let coords = (0..size.x).into_iter().flat_map(move |r| {
            (0..size.y).into_iter().map(move |c| {
                view * Point2::from(Vector2::new(c, r).cast::<f64>().component_div(&denom))
            })
        });

        coords
            .map(|mut c| {
                c.x = base_hz * 2.0_f64.powf(c.x);
                c.y = base_hz * 2.0_f64.powf(c.y);
                c
            })
            .take_while(|_| !cancel.load(Ordering::Relaxed))
            .collect()
    };

    if cancel.load(Ordering::Relaxed) {
        return Err(Cancelled);
    }

    trace!("Rendering map...");

    // TODO
    let wave: Wave = (1..=32)
        .into_iter()
        .map(|i| Partial {
            pitch: i.into(),
            amp: 1.0 / f64::from(i),
        })
        .collect();

    let cache_mutex = Mutex::new(cache_entry);
    let base_wave = &pitch.collect_partials(wave.map_pitch(|p| p * base_hz));

    let data = DefaultTileRenderer::new(RenderFunction {
        cache_entry: &cache_mutex,
        pitch,
        overlap,
        wave,
        base_wave,
    })
    .run(size, pitches, &blk_preload, cancel)?;

    if cancel.load(Ordering::SeqCst) {
        return Err(Cancelled);
    }

    let mut cache_entry = cache_mutex.into_inner().unwrap();

    cache_entry
        .append(CacheValue::Histogram(()))
        .context("failed to cache map histogram")?;

    Ok(DissonMap { size, data })
}
