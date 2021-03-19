use std::{
    fs::File,
    io::{prelude::*, stdout},
    path::PathBuf,
};

use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};

use crate::{
    cli::{GenerateOpts, SizeOverride},
    disson::algo::{OverlapCurve, PitchCurve},
    error::prelude::*,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateConfig {
    pub map: MapConfig,
    pub format: FormatConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MapConfig {
    pub width: u32,
    pub height: u32,
    pub pitch_curve: PitchCurve,
    pub overlap_curve: OverlapCurve,
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
                pitch_curve: PitchCurve::Erb,
                overlap_curve: OverlapCurve::ExpDiss,
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

        let mut cfg: GenerateConfig =
            ron::de::from_reader(file).context("failed to read config file")?;

        if let Some(size) = size {
            match size {
                SizeOverride::Width(w) => {
                    let MapConfig { width, height, .. } = &mut cfg.map;

                    let h = (f64::from(*w) * f64::from(*height) / f64::from(*width)).round();

                    if !h.is_normal() {
                        return Err(anyhow!("couldn't calculate new map height for override"));
                    }

                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    {
                        *width = *w;
                        *height = h as u32;
                    }
                },
                SizeOverride::Height(h) => {
                    let MapConfig { width, height, .. } = &mut cfg.map;

                    let w = (f64::from(*h) * f64::from(*width) / f64::from(*height)).round();

                    if !w.is_normal() {
                        return Err(anyhow!("couldn't calculate new map width for override"));
                    }

                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    {
                        *width = w as u32;
                        *height = *h;
                    }
                },
                SizeOverride::Exact(w, h) => {
                    cfg.map.width = *w;
                    cfg.map.height = *h;
                },
                SizeOverride::Percent(p) => {
                    if *p < 1e-7 {
                        return Err(anyhow!(
                            "invalid percentage for map size override, must be non-negative"
                        ));
                    }

                    let MapConfig { width, height, .. } = &mut cfg.map;

                    let w = (f64::from(*width) * *p).round();
                    let h = (f64::from(*height) * p).round();

                    if !(w.is_normal() && h.is_normal()) {
                        return Err(anyhow!("couldn't calculate new map size for override"));
                    }

                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    {
                        *width = w as u32;
                        *height = h as u32;
                    }
                },
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
