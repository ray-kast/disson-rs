use log::warn;
use nalgebra::{Matrix3, Vector2};
use serde::{Deserialize, Serialize};

use super::algo::{OverlapCurve, PitchCurve};
use crate::{cache::prelude::*, config::MapConfig, error::prelude::*};

#[derive(Debug, Clone, Copy, Serialize)]
pub(super) struct Config {
    res: Vector2<u32>,
    view: Matrix3<f64>,
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
            },
            (),
        )
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CacheKey {
    cfg: Config,
    pitch_curve: &'static str,
    overlap_curve: &'static str,
}

pub(super) type DissonMap = Vec<f64>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CacheValue {
    Block(()),
    Histogram(()),
}

// TODO: stop using type variables for curves
pub(super) fn compute<P: PitchCurve, O: OverlapCurve, C: for<'a> Cache<'a>>(
    cache: C,
    cfg: Config,
) -> Result<DissonMap> {
    let mut cache_entry = cache
        .entry(CacheKey {
            cfg,
            pitch_curve: P::ID,
            overlap_curve: O::ID,
        })
        .context("couldn't open cache entry")?;

    // TODO: remove this type ascription
    let blocks: Vec<CacheValue> = cache_entry.read().context("couldn't read cache blocks")?;
    // TODO: process existing blocks

    let mut result = vec![0.0; cfg.res.x as usize * cfg.res.y as usize]; // TODO

    // cache_entry.write(CacheKey

    result[cfg.res.x as usize] = 1.0;

    Ok(result)
}
