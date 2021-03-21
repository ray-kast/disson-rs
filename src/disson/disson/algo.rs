use std::iter::FromIterator;

use serde::{Deserialize, Serialize};

use super::wave::Partial;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PitchCurve {
    #[serde(rename = "Logarithmic")]
    Edo,
    #[serde(rename = "ErbRate")]
    Erb,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum OverlapCurve {
    #[serde(rename = "ExponentialDissonance")]
    ExpDiss,
    #[serde(rename = "TrapezoidDissonance")]
    TrapDiss,
    #[serde(rename = "TriangleConsonance")]
    TriCons,
    #[serde(rename = "TrapezoidConsonance")]
    TrapCons,
}

impl PitchCurve {
    fn edo(hz: f64) -> f64 { hz.log2() }

    fn erb(hz: f64) -> f64 { 11.17268 * (1.0 + (hz * 46.06538) / (hz + 14678.49)).ln() }

    #[inline]
    fn partial(f: impl Fn(f64) -> f64) -> impl Fn(&Partial) -> Partial {
        move |p| Partial {
            pitch: f(p.pitch),
            ..*p
        }
    }

    pub fn eval(self, hz: f64) -> f64 {
        match self {
            Self::Edo => Self::edo(hz),
            Self::Erb => Self::erb(hz),
        }
    }

    pub fn collect<I: IntoIterator<Item = f64>>(self, it: I) -> Vec<f64> {
        match self {
            Self::Edo => it.into_iter().map(Self::edo).collect(),
            Self::Erb => it.into_iter().map(Self::erb).collect(),
        }
    }

    pub fn collect_partials<'a, I: IntoIterator<Item = &'a Partial>, F: FromIterator<Partial>>(
        self,
        it: I,
    ) -> F {
        match self {
            Self::Edo => it.into_iter().map(Self::partial(Self::edo)).collect(),
            Self::Erb => it.into_iter().map(Self::partial(Self::erb)).collect(),
        }
    }
}

impl OverlapCurve {
    fn exp_diss(x: f64) -> f64 { x * (1.0 - x).exp() }

    fn trap_diss(x: f64) -> f64 { (3.0 * x).min(1.0) * (2.0 - x).max(0.0).min(1.0) }

    fn tri_cons(x: f64) -> f64 { (1.0 - x).max(0.0) }

    fn trap_cons(x: f64) -> f64 { (2.0 - x).max(0.0).min(1.0) }

    #[inline]
    fn overlap(f: impl Fn(f64) -> f64) -> impl Fn((f64, f64)) -> f64 {
        move |(a, b)| f((b - a).abs())
    }

    pub fn eval(self, pair: (f64, f64)) -> f64 {
        match self {
            Self::ExpDiss => Self::overlap(Self::exp_diss)(pair),
            Self::TrapDiss => Self::overlap(Self::trap_diss)(pair),
            Self::TriCons => Self::overlap(Self::tri_cons)(pair),
            Self::TrapCons => Self::overlap(Self::trap_cons)(pair),
        }
    }

    pub fn collect<I: IntoIterator<Item = (f64, f64)>>(self, it: I) -> Vec<f64> {
        match self {
            Self::ExpDiss => it.into_iter().map(Self::overlap(Self::exp_diss)).collect(),
            Self::TrapDiss => it.into_iter().map(Self::overlap(Self::trap_diss)).collect(),
            Self::TriCons => it.into_iter().map(Self::overlap(Self::tri_cons)).collect(),
            Self::TrapCons => it.into_iter().map(Self::overlap(Self::trap_cons)).collect(),
        }
    }
}
