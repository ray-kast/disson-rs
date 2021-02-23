#![warn(clippy::all, clippy::pedantic)]
#![deny(missing_debug_implementations)]
#![allow(clippy::module_name_repetitions)]

mod cache;
mod cli;
mod config;
mod disson;
mod error;
mod gui;

use cli::{Opts, Subcommand};

fn main() {
    let Opts { opts: global, cmd } = cli::parse();

    let result = match cmd {
        Subcommand::Clean => cache::clean(global),
        Subcommand::Gui => gui::run(global),
        Subcommand::Generate(g) => disson::generate(global, g),
        Subcommand::PrintDefaults => config::print_defaults(),
        Subcommand::Watch(g) => disson::watch(global, g),
    };

    match result {
        Ok(()) => (),
        Err(e) => {
            eprintln!("ERROR: {:?}", e);
            std::process::exit(-1);
        },
    }
}
