use nalgebra::{Matrix3, Vector2};
use serde::Serialize;

use super::algo::{OverlapCurve, PitchCurve};
use crate::{cache::Cache, error::prelude::*};
use crate::config::MapConfig;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Config {
    res: Vector2<usize>,
    view: Matrix3<f64>,
}

impl Config {
    pub fn for_generate(cfg: MapConfig) -> (Self, ()) {
        let MapConfig { width, height, pitch_curve, overlap_curve } = cfg;

        (Self {
            res: Vector2::new(width, height),
            view: Matrix3::identity(), // TODO
        }, ())
    }
}

#[derive(Debug, Serialize)]
pub struct CacheKey {
    cfg: Config,
    pitch_curve: &'static str,
    overlap_curve: &'static str,
}

pub type DissonMap = Vec<f64>;

pub fn compute<P: PitchCurve, O: OverlapCurve>(
    cache: &dyn Cache<CacheKey, DissonMap>,
    cfg: Config,
) -> Result<DissonMap> {
    let key = CacheKey {
        cfg,
        pitch_curve: P::ID,
        overlap_curve: O::ID,
    };

    match cache.read_checked(&key, &|_, v| {
        if v.len() != cfg.res.x * cfg.res.y {
            return Err(anyhow!("size mismatch for cached map"));
        }

        Ok(())
    }) {
        Ok(r) => return Ok(r),
        Err(e) => (), // TODO: log error?
    }

    let mut result = vec![0.0; cfg.res.x * cfg.res.y]; // TODO

    result[500] = 1.0;

    cache.write(&key, &result)?;

    Ok(result)
}
