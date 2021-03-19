use nalgebra::{Matrix3, Vector2};
use serde::{Deserialize, Serialize};

use super::algo::{OverlapCurve, PitchCurve};
use crate::{cache::prelude::*, config::MapConfig, error::prelude::*};

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

pub(super) type DissonMap = Vec<f64>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CacheValue {
    Block(()),
    Histogram(()),
}

pub(super) fn compute<C: for<'a> Cache<'a>>(cache: C, cfg: Config) -> Result<DissonMap> {
    let mut cache_entry = cache
        .entry(CacheKey(cfg))
        .context("couldn't open cache entry")?;

    // TODO: remove this type ascription
    let blocks: Vec<CacheValue> = cache_entry.read().context("couldn't read cache blocks")?;
    // TODO: process existing blocks

    let mut result = vec![0.0; cfg.res.x as usize * cfg.res.y as usize]; // TODO

    // TODO
    cache_entry
        .append(CacheValue::Block(()))
        .context("failed to cache map blocks")?;

    result[cfg.res.x as usize] = 1.0;

    Ok(result)
}
