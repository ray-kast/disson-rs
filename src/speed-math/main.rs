#![warn(clippy::all, clippy::pedantic)]
#![feature(test)]

use std::{
    borrow::Borrow,
    hint::black_box,
    io::{prelude::*, stderr},
    time::{Duration, Instant},
};

use rand::{prelude::*, rngs::StdRng};

//// Test harness functions and plumbing code

macro_rules! time {
    ($($expr:expr);+) => {
        {
            let __start = Instant::now();
            let __ret = { $($expr)* };
            let __end = Instant::now();

            (__ret, __end - __start)
        }
    };

    ($($expr:expr);+;) => { time! { $($expr)* }.1 };

    () => { time! { (); } };
}

fn run_test(
    len: usize,
    tries: usize,
    check: Option<fn(f64, f64) -> f64>,
    seed: <StdRng as SeedableRng>::Seed,
    f: impl Fn(&[f64], &[f64], &mut [f64]),
) -> Vec<Duration> {
    let mut times = Vec::with_capacity(tries);
    let mut cerr = stderr();
    let mut rng = StdRng::from_seed(seed);

    for run in 0..tries {
        write!(cerr, "\r\x1b[2K  Run {}...", run + 1).unwrap();
        cerr.flush().unwrap();

        let mut a = vec![0_f64; len].into_boxed_slice();
        let mut b = vec![0_f64; len].into_boxed_slice();
        let mut out = vec![0_f64; len].into_boxed_slice();

        rng.fill(&mut *a);
        rng.fill(&mut *b);

        assert_eq!(a.len(), out.len());
        assert_eq!(b.len(), out.len());

        times.push(time! { f(black_box(&a), black_box(&b), black_box(&mut out)); });

        if let Some(check) = check {
            write!(cerr, " (checking)").unwrap();
            cerr.flush().unwrap();

            for (i, out) in out.iter().enumerate() {
                assert_eq!(check(a[i], b[i]), *out, "mismatch at index {}", i);
            }
        }
    }

    writeln!(cerr).unwrap();

    times
}

fn time_fmt(d: impl Borrow<Duration>) -> String {
    let d = d.borrow();
    let nanos = d.subsec_nanos();
    let secs = d.as_secs();
    let mins = secs / 60;
    let secs = secs % 60;
    let hrs = mins / 60;

    if hrs > 0 {
        let mins = mins % 60;
        format!("{:01}:{:02}:{:02}.{:09}", hrs, mins, secs, nanos)
    } else {
        format!("{:01}:{:02}.{:09}", mins, secs, nanos)
    }
}

fn print_stats(samples: impl AsRef<[Duration]>) {
    let samples: Vec<_> = samples.as_ref().iter().map(|d| d.as_secs_f64()).collect();
    let avg = samples.iter().sum::<f64>() / samples.len() as f64;
    let var = samples.iter().map(|x| (x - avg).powi(2)).sum::<f64>() / (samples.len() - 1) as f64;

    println!("  Mean time: {}", time_fmt(Duration::from_secs_f64(avg)));
    println!("  Variance: {}", time_fmt(Duration::from_secs_f64(var)));
}

//// Functions to be benchmarked

// Using inline(always) because in practice these will not be separate functions

#[inline(always)]
fn expon(a: f64, b: f64) -> f64 {
    let x = (b - a).abs();
    x * (1.0 - x).exp()
}

#[inline(always)]
fn linear(a: f64, b: f64) -> f64 {
    let x = (b - a).abs();
    (3.0 * x).min(1.0) * (2.0 - x).max(0.0).min(1.0)
}

#[inline(always)]
fn expon_tup((a, b): (f64, f64)) -> f64 { expon(a, b) }

#[inline(always)]
fn linear_tup((a, b): (f64, f64)) -> f64 { linear(a, b) }

#[inline(always)]
fn map_slice<
    'a,
    F: FnOnce(
        std::iter::Zip<
            std::iter::Copied<std::slice::Iter<'a, f64>>,
            std::iter::Copied<std::slice::Iter<'a, f64>>,
        >,
    ) -> J,
    J: IntoIterator<Item = f64> + 'a,
>(
    f: F,
    a: &'a [f64],
    b: &'a [f64],
    out: &mut [f64],
) {
    for (i, o) in f(a.iter().copied().zip(b.iter().copied()))
        .into_iter()
        .enumerate()
    {
        // TODO: check for excessive bounds-checking
        out[i] = o;
    }
}

#[inline(always)]
fn eval_slice<F: Fn((f64, f64)) -> f64>(f: F, a: &[f64], b: &[f64], out: &mut [f64]) {
    map_slice(move |i| i.map(f), a, b, out)
}

trait DynFunction {
    fn eval(&self, a: f64, b: f64) -> f64;

    fn map(&self, a: &[f64], b: &[f64], out: &mut [f64]);
}

struct Expon;

impl DynFunction for Expon {
    fn eval(&self, a: f64, b: f64) -> f64 { expon(a, b) }

    fn map(&self, a: &[f64], b: &[f64], out: &mut [f64]) { eval_slice(expon_tup, a, b, out) }
}

struct Linear;

impl DynFunction for Linear {
    fn eval(&self, a: f64, b: f64) -> f64 { linear(a, b) }

    fn map(&self, a: &[f64], b: &[f64], out: &mut [f64]) { eval_slice(linear_tup, a, b, out) }
}

#[derive(Debug, Clone, Copy)]
enum FunctionEnum {
    Expon,
    Linear,
}

impl FunctionEnum {
    fn into_dyn(self) -> Box<dyn DynFunction> {
        match self {
            Self::Expon => Box::new(Expon),
            Self::Linear => Box::new(Linear),
        }
    }

    fn eval(self, a: f64, b: f64) -> f64 {
        match self {
            Self::Expon => expon(a, b),
            Self::Linear => linear(a, b),
        }
    }

    fn map<I: IntoIterator<Item = (f64, f64)>>(
        self,
        it: I,
    ) -> std::iter::Map<I::IntoIter, fn((f64, f64)) -> f64> {
        it.into_iter().map(match self {
            Self::Expon => expon_tup,
            Self::Linear => linear_tup,
        })
    }

    fn map_slice(self, a: &[f64], b: &[f64], out: &mut [f64]) {
        eval_slice(
            match self {
                Self::Expon => expon_tup,
                Self::Linear => linear_tup,
            },
            a,
            b,
            out,
        );
    }
}

//// Test suite

fn main() {
    use std::array::IntoIter;
    const TRIES: usize = 1000;
    const CHECK: bool = true;

    let mut seed = [0_u8; 32];
    StdRng::from_entropy().fill_bytes(&mut seed);

    for len in IntoIter::new([10, 1_000, 100_000]) {
        println!("//// LENGTH: {}", len);

        for ty in IntoIter::new([FunctionEnum::Expon, FunctionEnum::Linear]) {
            let ty = black_box(ty);

            let check = if CHECK {
                Some(match ty {
                    FunctionEnum::Expon => expon,
                    FunctionEnum::Linear => linear,
                })
            } else {
                None
            };

            println!("//// TYPE: {:?}", ty);

            {
                println!("Running baseline...");
                let times_base = match ty {
                    FunctionEnum::Expon => run_test(len, TRIES, check, seed, |a, b, o| {
                        eval_slice(expon_tup, a, b, o)
                    }),
                    FunctionEnum::Linear => run_test(len, TRIES, check, seed, |a, b, o| {
                        eval_slice(linear_tup, a, b, o)
                    }),
                };
                print_stats(times_base);
            }

            {
                println!("Running non-batched enum...");
                let times_enum_eval = run_test(len, TRIES, check, seed, |a, b, o| {
                    eval_slice(|(a, b)| ty.eval(a, b), a, b, o)
                });
                print_stats(times_enum_eval);
            }

            {
                println!("Running batched enum (iterator version)...");
                let times_enum_map = run_test(len, TRIES, check, seed, |a, b, o| {
                    map_slice(|i| ty.map(i), a, b, o)
                });
                print_stats(times_enum_map);
            }

            {
                println!("Running batched enum (slice version)...");
                let times_enum_map_slice =
                    run_test(len, TRIES, check, seed, |a, b, o| ty.map_slice(a, b, o));
                print_stats(times_enum_map_slice);
            }

            {
                println!("Running non-batched dyn...");
                let dy = ty.into_dyn();
                let times_dyn_eval = run_test(len, TRIES, check, seed, |a, b, o| {
                    eval_slice(|(a, b)| dy.eval(a, b), a, b, o)
                });
                print_stats(times_dyn_eval);
            }

            {
                println!("Running batched dyn...");
                let dy = ty.into_dyn();
                let times_dyn_map = run_test(len, TRIES, check, seed, |a, b, o| dy.map(a, b, o));
                print_stats(times_dyn_map);
            }
        }
    }
}
