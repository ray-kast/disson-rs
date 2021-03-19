use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc},
};

use nalgebra::{Matrix3, Vector2};
use serde::{Deserialize, Serialize};

use super::algo::{OverlapCurve, PitchCurve};
use crate::{
    cache::prelude::*,
    config::MapConfig,
    error::prelude::*,
    tile_renderer::{DefaultTileRenderer, Tile, TileRenderFunction},
};

#[derive(Debug, Clone, Copy, Serialize)]
pub(super) struct Config {
    res: Vector2<u32>,
    view: Matrix3<f64>,
    pitch: PitchCurve,
    overlap: OverlapCurve,
}

impl Config {
    pub fn for_generate(cfg: MapConfig) -> (Self, ()) {
        let MapConfig {
            width,
            height,
            pitch_curve,
            overlap_curve,
        } = cfg;

        (
            Self {
                res: Vector2::new(width, height),
                view: Matrix3::identity(), // TODO
                pitch: pitch_curve,
                overlap: overlap_curve,
            },
            (),
        )
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CacheKey(Config);

pub(super) type DissonMap = Box<[f64]>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CacheValue {
    Block(()),
    Histogram(()),
}

struct RenderFunction;

impl TileRenderFunction for RenderFunction {
    type Input = Vector2<f64>;
    type Output = f64;

    fn process(&self, mut tile: Tile<Self::Input, Self::Output>) {
        for r in 0..tile.range().size.y {
            let (row_in, row_out) = tile.row_mut(r);

            for c in 0..row_in.len() {
                row_out[c] = row_in[c].x;
            }
        }
    }
}

pub(super) fn compute<C: for<'a> Cache<'a>>(
    cache: C,
    cfg: Config,
    cancel: Arc<AtomicBool>,
) -> Result<DissonMap> {
    let mut cache_entry = cache
        .entry(CacheKey(cfg))
        .context("couldn't open cache entry")?;

    // TODO: remove this type ascription
    let blocks: Vec<CacheValue> = cache_entry.read().context("couldn't read cache blocks")?;
    // TODO: process existing blocks

    // TODO
    let preload: HashMap<_, Vec<_>> = HashMap::new();

    // TODO
    let mut pitches = vec![Vector2::new(0.0, 0.0); cfg.res.x as usize * cfg.res.y as usize];

    let res = DefaultTileRenderer::new(RenderFunction).run(cfg.res, pitches, &preload, cancel);

    // TODO
    cache_entry
        .append(CacheValue::Block(()))
        .context("failed to cache map blocks")?;

    Ok(res)
}
