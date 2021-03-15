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
    ($($stmt:stmt);+) => {
        {
            let __start = Instant::now();
            let __ret = { $($stmt)+ };
            let __end = Instant::now();

            (__ret, __end - __start)
        }
    };

    ($($stmt:stmt;)+) => { time! { $($stmt);+ }.1 };

    () => { time! { (); } };
}

fn run_test_iter<F: StaticFunction>(
    len: usize,
    tries: usize,
    check: Option<fn(f64, f64) -> f64>,
    seed: <StdRng as SeedableRng>::Seed,
    f: F,
) -> Vec<Duration> {
    let mut times = Vec::with_capacity(tries);
    let mut cerr = stderr();
    let mut rng = StdRng::from_seed(seed);

    for run in 0..tries {
        write!(cerr, "\r\x1b[2K  Run {}...", run + 1).unwrap();
        cerr.flush().unwrap();

        let mut a = vec![0_f64; len];
        let mut b = vec![0_f64; len];

        rng.fill(&mut *a);
        rng.fill(&mut *b);

        assert_eq!(a.len(), b.len());

        let out;

        times.push(time! { out = f.collect(a.iter().cloned().zip(b.iter().copied())); });

        assert_eq!(a.len(), out.len());

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

fn run_test_slice(
    len: usize,
    tries: usize,
    check: Option<fn(f64, f64) -> f64>,
    seed: <StdRng as SeedableRng>::Seed,
    f: impl Fn(&[(f64, f64)]) -> Vec<f64>,
) -> Vec<Duration> {
    let mut times = Vec::with_capacity(tries);
    let mut cerr = stderr();
    let mut rng = StdRng::from_seed(seed);

    for run in 0..tries {
        write!(cerr, "\r\x1b[2K  Run {}...", run + 1).unwrap();
        cerr.flush().unwrap();

        let mut a = vec![0_f64; len];
        let mut b = vec![0_f64; len];

        rng.fill(&mut *a);
        rng.fill(&mut *b);

        assert_eq!(a.len(), b.len());

        let ab: Vec<(f64, f64)>;
        let out;

        times.push(time! {
            ab = a.into_iter().zip(b.into_iter()).collect();
            out = f(black_box(&ab));
        });

        assert_eq!(ab.len(), out.len());

        if let Some(check) = check {
            write!(cerr, " (checking)").unwrap();
            cerr.flush().unwrap();

            for (i, out) in out.iter().enumerate() {
                let (a, b) = ab[i];
                assert_eq!(check(a, b), *out, "mismatch at index {}", i);
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

fn print_stats(
    head: impl std::fmt::Display,
    len: usize,
    ty: FunctionEnum,
    samples: impl AsRef<[Duration]>,
) {
    let samples = samples.as_ref();
    let samples_sec: Vec<_> = samples.iter().map(|d| d.as_secs_f64()).collect();
    let avg = samples_sec.iter().sum::<f64>() / samples_sec.len() as f64;
    let var =
        samples_sec.iter().map(|x| (x - avg).powi(2)).sum::<f64>() / (samples_sec.len() - 1) as f64;

    eprintln!("  Mean time: {}", time_fmt(Duration::from_secs_f64(avg)));
    eprintln!("  Variance: {}", time_fmt(Duration::from_secs_f64(var)));

    let samples_nsec: Vec<_> = samples.iter().map(|d| d.as_nanos()).collect();
    // samples_nsec.sort_unstable();

    print!("{:?}", format!("{} ({} {:?})", head, len, ty));
    for samp in samples_nsec {
        print!(",{}", samp);
    }
    println!();
}

//// Functions to be benchmarked

fn expon(a: f64, b: f64) -> f64 {
    let x = (b - a).abs();
    x * (1.0 - x).exp()
}

fn linear(a: f64, b: f64) -> f64 {
    let x = (b - a).abs();
    (3.0 * x).min(1.0) * (2.0 - x).max(0.0).min(1.0)
}

fn expon_tup((a, b): (f64, f64)) -> f64 { expon(a, b) }

fn linear_tup((a, b): (f64, f64)) -> f64 { linear(a, b) }

fn map_slice<
    'a,
    F: FnOnce(std::iter::Copied<std::slice::Iter<'a, (f64, f64)>>) -> J,
    J: IntoIterator<Item = f64> + 'a,
>(
    f: F,
    ab: &'a [(f64, f64)],
) -> Vec<f64> {
    f(ab.iter().copied()).into_iter().collect()
}

fn eval_slice<F: Fn((f64, f64)) -> f64>(f: F, ab: &[(f64, f64)]) -> Vec<f64> {
    map_slice(move |i| i.map(f), ab)
}

trait StaticFunction: Copy {
    fn eval(self, a: f64, b: f64) -> f64;

    fn map<I: IntoIterator<Item = (f64, f64)>>(
        self,
        it: I,
    ) -> std::iter::Map<I::IntoIter, fn((f64, f64)) -> f64>;

    fn map_slice(self, ab: &[(f64, f64)]) -> Vec<f64>;

    fn collect<I: IntoIterator<Item = (f64, f64)>>(self, it: I) -> Vec<f64>;
}

trait DynFunction {
    fn eval(&self, a: f64, b: f64) -> f64;

    fn map_slice(&self, ab: &[(f64, f64)]) -> Vec<f64>;
}

#[derive(Debug, Clone, Copy)]
struct Expon;

impl StaticFunction for Expon {
    fn eval(self, a: f64, b: f64) -> f64 { expon(a, b) }

    fn map<I: IntoIterator<Item = (f64, f64)>>(
        self,
        it: I,
    ) -> std::iter::Map<I::IntoIter, fn((f64, f64)) -> f64> {
        it.into_iter().map(expon_tup)
    }

    fn map_slice(self, ab: &[(f64, f64)]) -> Vec<f64> { eval_slice(expon_tup, ab) }

    fn collect<I: IntoIterator<Item = (f64, f64)>>(self, it: I) -> Vec<f64> {
        self.map(it).collect()
    }
}

impl DynFunction for Expon {
    fn eval(&self, a: f64, b: f64) -> f64 { expon(a, b) }

    fn map_slice(&self, ab: &[(f64, f64)]) -> Vec<f64> { eval_slice(expon_tup, ab) }
}

#[derive(Debug, Clone, Copy)]
struct Linear;

impl StaticFunction for Linear {
    fn eval(self, a: f64, b: f64) -> f64 { linear(a, b) }

    fn map<I: IntoIterator<Item = (f64, f64)>>(
        self,
        it: I,
    ) -> std::iter::Map<I::IntoIter, fn((f64, f64)) -> f64> {
        it.into_iter().map(linear_tup)
    }

    fn map_slice(self, ab: &[(f64, f64)]) -> Vec<f64> { eval_slice(linear_tup, ab) }

    fn collect<I: IntoIterator<Item = (f64, f64)>>(self, it: I) -> Vec<f64> {
        self.map(it).collect()
    }
}

impl DynFunction for Linear {
    fn eval(&self, a: f64, b: f64) -> f64 { linear(a, b) }

    fn map_slice(&self, ab: &[(f64, f64)]) -> Vec<f64> { eval_slice(linear_tup, ab) }
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
}

impl StaticFunction for FunctionEnum {
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
        match self {
            Self::Expon => Expon.map(it),
            Self::Linear => Linear.map(it),
        }
    }

    fn map_slice(self, ab: &[(f64, f64)]) -> Vec<f64> {
        match self {
            Self::Expon => Expon.map_slice(ab),
            Self::Linear => Linear.map_slice(ab),
        }
    }

    fn collect<I: IntoIterator<Item = (f64, f64)>>(self, it: I) -> Vec<f64> {
        match self {
            Self::Expon => Expon.collect(it),
            Self::Linear => Linear.collect(it),
        }
    }
}

//// Test suite

#[allow(clippy::too_many_lines)]
fn main() {
    use std::array::IntoIter;
    const TRIES: usize = 1000;
    const CHECK: bool = false;

    let mut seed = [0_u8; 32];
    StdRng::from_entropy().fill_bytes(&mut seed);

    for len in IntoIter::new([10, 1_000, 100_000, 1_000_000]) {
        eprintln!("//// LENGTH: {}", len);

        for ty in IntoIter::new([FunctionEnum::Linear, FunctionEnum::Expon]) {
            let ty = black_box(ty);

            let check = if CHECK {
                Some(match ty {
                    FunctionEnum::Expon => expon,
                    FunctionEnum::Linear => linear,
                })
            } else {
                None
            };

            eprintln!("//// TYPE: {:?}", ty);

            {
                eprintln!("Running baseline (iterator version)...");
                let times_base = match ty {
                    FunctionEnum::Expon => run_test_slice(len, TRIES, check, seed, |ab| {
                        ab.iter().copied().map(expon_tup).collect()
                    }),
                    FunctionEnum::Linear => run_test_slice(len, TRIES, check, seed, |ab| {
                        ab.iter().copied().map(linear_tup).collect()
                    }),
                };
                print_stats("Baseline (iter)", len, ty, times_base);
            }

            {
                eprintln!("Running baseline (slice version)...");
                let times_base = match ty {
                    FunctionEnum::Expon => {
                        run_test_slice(len, TRIES, check, seed, |ab| eval_slice(expon_tup, ab))
                    },
                    FunctionEnum::Linear => {
                        run_test_slice(len, TRIES, check, seed, |ab| eval_slice(linear_tup, ab))
                    },
                };
                print_stats("Baseline (slice)", len, ty, times_base);
            }

            {
                eprintln!("Running batched static (slice version)...");
                let times_static = match ty {
                    FunctionEnum::Expon => {
                        run_test_slice(len, TRIES, check, seed, |ab| Expon.map_slice(ab))
                    },
                    FunctionEnum::Linear => {
                        run_test_slice(len, TRIES, check, seed, |ab| Linear.map_slice(ab))
                    },
                };
                print_stats("Static (single)", len, ty, times_static);
            }

            {
                eprintln!("Running batched static (collecting version)...");
                let times_enum_collect = match ty {
                    FunctionEnum::Expon => run_test_iter(len, TRIES, check, seed, Expon),
                    FunctionEnum::Linear => run_test_iter(len, TRIES, check, seed, Linear),
                };
                print_stats("Static (collect)", len, ty, times_enum_collect);
            }

            {
                eprintln!("Running non-batched enum...");
                let times_enum_eval = run_test_slice(len, TRIES, check, seed, |ab| {
                    eval_slice(|(a, b)| ty.eval(a, b), ab)
                });
                print_stats("Enum (single)", len, ty, times_enum_eval);
            }

            {
                eprintln!("Running batched enum (iterator version)...");
                let times_enum_map =
                    run_test_slice(len, TRIES, check, seed, |ab| map_slice(|i| ty.map(i), ab));
                print_stats("Enum (iter)", len, ty, times_enum_map);
            }

            {
                eprintln!("Running batched enum (slice version)...");
                let times_enum_map_slice =
                    run_test_slice(len, TRIES, check, seed, |ab| ty.map_slice(ab));
                print_stats("Enum (slice)", len, ty, times_enum_map_slice);
            }

            {
                eprintln!("Running batched enum (collecting version)...");
                let times_enum_collect = run_test_iter(len, TRIES, check, seed, ty);
                print_stats("Enum (collect)", len, ty, times_enum_collect);
            }

            {
                eprintln!("Running non-batched dyn...");
                let dy = ty.into_dyn();
                let times_dyn_eval = run_test_slice(len, TRIES, check, seed, |ab| {
                    eval_slice(|(a, b)| dy.eval(a, b), ab)
                });
                print_stats("Dyn (single)", len, ty, times_dyn_eval);
            }

            {
                eprintln!("Running batched dyn...");
                let dy = ty.into_dyn();
                let times_dyn_map = run_test_slice(len, TRIES, check, seed, |ab| dy.map_slice(ab));
                print_stats("Dyn (slice)", len, ty, times_dyn_map);
            }
        }
    }
}
