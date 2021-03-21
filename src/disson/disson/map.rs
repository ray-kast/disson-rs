use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
};

use log::{trace, warn};
use nalgebra::{Point2, Transform2, Vector2};
use serde::{Deserialize, Serialize};

use super::algo::{OverlapCurve, PitchCurve};
use crate::{
    cache::prelude::*,
    config::MapConfig,
    error::cancel::prelude::*,
    tile_renderer::{DefaultTileRenderer, Tile, TileRange, TileRenderFunction},
};

#[derive(Debug, Clone, Copy, Serialize)]
pub(super) struct Config {
    res: Vector2<u32>,
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
            res: Vector2::new(width, height),
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
pub enum CacheValue {
    Block(TileRange, Vec<f64>),
    Histogram(()),
}

struct RenderFunction(PitchCurve, OverlapCurve);

impl TileRenderFunction for RenderFunction {
    type Input = Point2<f64>;
    type Output = f64;

    fn process(&self, mut tile: Tile<Self::Input, Self::Output>) {
        for r in 0..tile.range().size.y {
            let (row_in, row_out) = tile.row_mut(r);

            for c in 0..row_in.len() {
                row_out[c] = row_in[c].x; // TODO
            }
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
        let denom = (cfg.res - Vector2::new(1, 1)).cast::<f64>();

        let coords = (0..cfg.res.x).into_iter().flat_map(move |r| {
            (0..cfg.res.y).into_iter().map(move |c| {
                cfg.view * Point2::from(Vector2::new(c, r).cast::<f64>().component_div(&denom))
            })
        });

        coords
            .map(|mut c| {
                c.x = cfg.base_hz * 2.0_f64.powf(c.x);
                c.y = cfg.base_hz * 2.0_f64.powf(c.y);
                c
            })
            .take_while(|_| !cancel.load(Ordering::Relaxed))
            .collect()
    };

    if cancel.load(Ordering::Relaxed) {
        return Err(Cancelled);
    }

    trace!("Rendering map...");

    let data = DefaultTileRenderer::new(RenderFunction(cfg.pitch, cfg.overlap)).run(
        cfg.res,
        pitches,
        &blk_preload,
        cancel,
    );

    if cancel.load(Ordering::SeqCst) {
        return Err(Cancelled);
    }

    // TODO
    cache_entry
        .append(CacheValue::Histogram(()))
        .context("failed to cache map histogram")?;

    Ok(DissonMap {
        size: cfg.res,
        data,
    })
}
