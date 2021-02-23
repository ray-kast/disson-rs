use std::{
    fs::File,
    io::{prelude::*, stdout},
    path::PathBuf,
};

use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};

use crate::{cli::GenerateOpts, error::prelude::*};

#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateConfig {
    pub map: MapConfig,
    pub format: FormatConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MapConfig {
    pub width: usize,
    pub height: usize,
    pub pitch_curve: PitchCurveId,
    pub overlap_curve: OverlapCurveId,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PitchCurveId {
    Log,
    ErbRate,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum OverlapCurveId {
    ExponentialDissonance,
    TrapezoidDissonance,

    TriangleConsonance,
    TrapezoidConsonance,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FormatConfig {
    #[serde(rename = "type")]
    pub ty: FormatType,
    pub out: MapOutput,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum FormatType {
    Csv,
    Png,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MapOutput {
    Stdout,
    File(PathBuf),
}

impl Default for GenerateConfig {
    fn default() -> Self {
        Self {
            map: MapConfig {
                width: 1000,
                height: 1000,
                pitch_curve: PitchCurveId::ErbRate,
                overlap_curve: OverlapCurveId::ExponentialDissonance,
            },
            format: FormatConfig {
                ty: FormatType::Csv,
                out: MapOutput::Stdout,
            },
        }
    }
}

impl GenerateConfig {
    pub fn read(opts: &GenerateOpts) -> Result<Self> {
        let GenerateOpts {
            config,
            size,
            ty,
            out,
        } = opts;

        let file = File::open(config).context("failed to open config file")?;

        let mut cfg = ron::de::from_reader(file).context("failed to read config file")?;

        if let Some(size) = size {
            match size {
                _ => todo!(),
            }
        }

        Ok(cfg)
    }
}

pub fn print_defaults() -> Result<()> {
    let mut stream = stdout();

    ron::ser::to_writer_pretty(
        &mut stream,
        &GenerateConfig::default(),
        PrettyConfig::new().with_decimal_floats(true),
    )
    .context("failed to serialize default config")?;

    if atty::is(atty::Stream::Stdout) {
        writeln!(stream).context("failed to write trailing newline")?;
    }

    Ok(())
}
