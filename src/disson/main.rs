#![warn(clippy::all, clippy::pedantic)]
#![deny(missing_debug_implementations)]
#![allow(clippy::module_name_repetitions)]

use cli::{GlobalOpts, Opts, Subcommand};
use log::{error, LevelFilter};

mod cache;
mod cli;
mod config;
mod disson;
mod error;
mod gui;
mod tile_renderer;

const VERBOSITY: [LevelFilter; 3] = [LevelFilter::Info, LevelFilter::Debug, LevelFilter::Trace];
#[cfg(debug_assertions)]
const DEFAULT_V: usize = 1;
#[cfg(not(debug_assertions))]
const DEFAULT_V: usize = 0;

fn main() {
    let Opts { opts: global, cmd } = cli::parse();
    let GlobalOpts {
        cache_mode,
        quiet,
        no_quiet,
        verbose,
    } = global;

    {
        let mut b = env_logger::builder();

        if !(no_quiet || verbose != 0 || atty::is(atty::Stream::Stderr)) || quiet {
            b.filter_level(LevelFilter::Warn);
        } else {
            b.filter_level(VERBOSITY[(DEFAULT_V + verbose).min(VERBOSITY.len() - 1)]);
        }

        b.init();
    }

    let result = match cmd {
        Subcommand::Clean => cache::clean(cache_mode),
        Subcommand::Gui => gui::run(cache_mode),
        Subcommand::Generate(g) => disson::generate(cache_mode, g),
        Subcommand::PrintDefaults => config::print_defaults(),
        Subcommand::Watch(g) => disson::watch(cache_mode, g),
    };

    match result {
        Ok(()) => (),
        Err(e) => {
            error!("Program exited with error: {:?}", e);
            std::process::exit(-1);
        },
    }
}
