use std::{path::PathBuf, str::FromStr};

use lazy_static::lazy_static;
use regex::{Regex, RegexBuilder};
use structopt::StructOpt;
use thiserror::Error;

use crate::error::prelude::*;

#[derive(Debug, StructOpt)]
pub struct Opts {
    #[structopt(flatten)]
    pub opts: GlobalOpts,
    #[structopt(subcommand)]
    pub cmd: Subcommand,
}

#[derive(Debug, StructOpt)]
pub struct GlobalOpts {
    /// The cache directory to use, or "-" to disable caching
    #[structopt(name = "cache-dir", short, long, default_value = "")]
    pub cache_mode: CacheMode,

    /// Only print warnings and errors to the console (enabled by default if no
    /// console is attached)
    #[structopt(short, long)]
    pub quiet: bool,

    /// Always print info messages, even if no console is detected
    #[structopt(long, conflicts_with("quiet"))]
    pub no_quiet: bool,

    /// Output extra information to the console
    #[structopt(
        short,
        long,
        parse(from_occurrences),
        conflicts_with("quiet"),
        conflicts_with("no_quiet")
    )]
    pub verbose: usize,
}

#[derive(Debug, StructOpt)]
pub enum Subcommand {
    /// Empty the cache folder
    Clean,
    /// Generate a dissonance map from the given config
    Generate(GenerateOpts),
    /// Open the GUI to interactively configure and generate maps
    Gui,
    /// Print the default configuration file to the console
    PrintDefaults,
    /// Generate a dissonance map from the given config, and watch it for
    /// changes
    Watch(GenerateOpts),
}

#[derive(Debug, StructOpt)]
pub struct GenerateOpts {
    /// The configuration file to read options from
    #[structopt(parse(from_os_str))]
    pub config: PathBuf,

    /// Override the output size
    ///
    /// Valid formats are <n>w and <n>h, which set width or height to n while
    /// keeping the configured aspect ratio; <x>%, which scales the configured
    /// output dimensions by x%; or <w>x<h>, which sets the dimensions to
    /// exactly w by h.
    #[structopt(short, long)]
    pub size: Option<SizeOverride>,

    /// The format to output the result in
    #[structopt(name = "type", short, long, requires("out"))]
    pub ty: Option<MapFormat>,

    #[structopt(short, long, default_value = "-")]
    pub out: MapOutput,
}

impl GenerateOpts {
    pub fn ty(&self) -> Result<MapFormat> {
        self.ty.map_or_else(
            || {
                Ok(match self.out {
                    MapOutput::Stdout => MapFormat::TSV,
                    MapOutput::File(ref p) => match p
                        .extension()
                        .map(|s| {
                            s.to_str()
                                .ok_or_else(|| anyhow!("couldn't read output file extension"))
                        })
                        .transpose()?
                    {
                        Some("png") => MapFormat::Png,
                        Some("csv") => MapFormat::CSV,
                        Some("tsv") | Some("txt") | None => MapFormat::TSV,
                        Some(e) => {
                            return Err(anyhow!(
                                "couldn't guess output format from file extension {:?}",
                                e
                            ))
                        },
                    },
                })
            },
            Ok,
        )
    }
}

#[derive(Debug, Error)]
pub enum FromStrErr {
    #[error("value {0:?} did not match any of {}", .1.join(", "))]
    OneOf(String, &'static [&'static str]),
    #[error("error reading {0:?}: {1}")]
    Custom(String, &'static str),
    #[error("error reading number in {0:?}: {1}")]
    ParseInt(String, std::num::ParseIntError),
    #[error("error reading number in {0:?}: {1}")]
    ParseFloat(String, std::num::ParseFloatError),
}

#[derive(Debug)]
pub enum CacheMode {
    Off,
    File(Option<PathBuf>),
}

#[derive(Debug, Clone, Copy)]
pub enum MapFormat {
    Xsv(u8),
    Png,
}

#[derive(Debug, Clone)]
pub enum MapOutput {
    Stdout,
    File(PathBuf),
}

#[derive(Debug)]
pub enum SizeOverride {
    Width(u32),
    Height(u32),
    Exact(u32, u32),
    Percent(f64),
}

impl FromStr for CacheMode {
    type Err = FromStrErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "" => Self::File(None),
            "-" => Self::Off,
            s => Self::File(Some(s.into())),
        })
    }
}

impl MapFormat {
    const CSV: Self = Self::Xsv(b',');
    const TSV: Self = Self::Xsv(b'\t');
}

impl FromStr for MapFormat {
    type Err = FromStrErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_ref() {
            "csv" => Self::CSV,
            "tsv" => Self::TSV,
            "png" => Self::Png,
            _ => return Err(FromStrErr::OneOf(s.into(), &["csv", "tsv", "png"])),
        })
    }
}

impl FromStr for MapOutput {
    type Err = FromStrErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "-" => Self::Stdout,
            s => Self::File(s.into()),
        })
    }
}

impl FromStr for SizeOverride {
    type Err = FromStrErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        lazy_static! {
            static ref WIDTH_HEIGHT_REGEX: Regex = RegexBuilder::new(r"^(\d+)([wh])$")
                .case_insensitive(true)
                .build()
                .unwrap();
            static ref PERCENT_REGEX: Regex = RegexBuilder::new(r"^(\d+(?:\.\d+))%$")
                .case_insensitive(true)
                .build()
                .unwrap();
            static ref EXACT_REGEX: Regex = RegexBuilder::new(r"^(\d+)x(\d+)$")
                .case_insensitive(true)
                .build()
                .unwrap();
        }

        Ok(if let Some(caps) = WIDTH_HEIGHT_REGEX.captures(s) {
            let len = caps[1]
                .parse()
                .map_err(|e| FromStrErr::ParseInt(caps[1].into(), e))?;

            match &caps[2] {
                "w" => SizeOverride::Width(len),
                "h" => SizeOverride::Height(len),
                _ => unreachable!(),
            }
        } else if let Some(caps) = PERCENT_REGEX.captures(s) {
            let pct = caps[1]
                .parse()
                .map_err(|e| FromStrErr::ParseFloat(caps[1].into(), e))?;

            SizeOverride::Percent(pct)
        } else if let Some(caps) = EXACT_REGEX.captures(s) {
            let w = caps[1]
                .parse()
                .map_err(|e| FromStrErr::ParseInt(caps[1].into(), e))?;
            let h = caps[2]
                .parse()
                .map_err(|e| FromStrErr::ParseInt(caps[2].into(), e))?;

            SizeOverride::Exact(w, h)
        } else {
            return Err(FromStrErr::Custom(
                s.into(),
                "valid formats are <n>w, <n>h, <x>%, or <w>x<h>",
            ));
        })
    }
}

pub fn parse() -> Opts { Opts::from_args() }
