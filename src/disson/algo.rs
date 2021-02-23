pub trait PitchCurve {
    const ID: &'static str;

    fn eval(hz: f64) -> f64;
}

pub trait OverlapCurve {
    const ID: &'static str;

    fn eval_abs(x: f64) -> f64;

    fn eval(a: f64, b: f64) -> f64 {
        let abs = (b - a).abs();
        Self::eval_abs(abs)
    }
}

macro_rules! curve {
    (pitch, $name:ident, $var:ident => $fn:expr) => {
        curve! { $name, PitchCurve, eval, $var => $fn }
    };

    (overlap, $name:ident, $var:ident => $fn:expr) => {
        curve! { $name, OverlapCurve, eval_abs, $var => $fn }
    };

    ($name:ident, $trait:ident, $fname:ident, $var:ident => $fn:expr) => {
        pub struct $name;

        impl $trait for $name {
            const ID: &'static str = stringify!($name);

            fn $fname($var: f64) -> f64 { $fn }
        }
    };
}

curve!(pitch, EdoPitch, hz => hz.log2());
curve!(pitch, ErbPitch, hz => 11.17268 * (1.0 + (hz * 46.06538) / (hz + 14678.49)).ln());

curve!(overlap, ExpDiss, x => x * (1.0 - x).exp());
curve!(overlap, TrapDiss, x => (3.0 * x).min(1.0) * (2.0 - x).max(0.0).min(1.0));

curve!(overlap, TriCons, x => (1.0 - x).max(0.0));
curve!(overlap, TrapCons, x => (2.0 - x).max(0.0).min(1.0));
