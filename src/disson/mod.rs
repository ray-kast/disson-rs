mod algo;
mod map;
mod waves;

use map::{CacheKey, DissonMap};
use notify::{event::ModifyKind, EventKind, RecursiveMode, Watcher};
use tokio::{runtime, select, signal, sync::mpsc};

use crate::{
    cache,
    cache::Cache,
    cli::{GenerateOpts, GlobalOpts},
    config::GenerateConfig,
    error::prelude::*,
};

fn generate_impl(
    cache: impl AsRef<dyn Cache<CacheKey, DissonMap>>,
    opts: &GenerateOpts,
) -> Result<()> {
    let cfg = GenerateConfig::read(opts).context("failed to get config")?;
    let (map_cfg, fmt_opts) = map::Config::for_generate(cfg.map);

    let map = map::compute::<algo::EdoPitch, algo::ExpDiss>(cache.as_ref(), map_cfg)
        .context("failed to generate new dissonance map")?;

    Ok(())
}

pub fn generate(global: GlobalOpts, opts: GenerateOpts) -> Result<()> {
    let GlobalOpts { cache_mode } = global;
    let cache = cache::from_opts(cache_mode);

    generate_impl(cache, &opts)
}

pub fn watch(global: GlobalOpts, opts: GenerateOpts) -> Result<()> {
    let GlobalOpts { cache_mode } = global;
    let cache = cache::from_opts(cache_mode);

    // TODO: handle Ctrl+C

    if opts.config.exists() {
        eprintln!("Running initial pass...");

        generate_impl(&cache, &opts)?;
    } else {
        eprintln!("WARNING: config file doesn't exist yet, waiting for a new one...");
    }

    eprintln!("Listening for changes...");

    let (tx, mut rx) = mpsc::unbounded_channel();

    let mut watcher = notify::immediate_watcher(move |evt| tx.send(evt).unwrap())
        .context("failed to open filesystem watcher")?;

    watcher
        .watch(
            opts.config
                .parent()
                .ok_or_else(|| anyhow!("invalid config path {:?}", opts.config))?,
            RecursiveMode::NonRecursive,
        )
        .with_context(|| format!("failed to watch file {:?}", opts.config))?;

    runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to setup Tokio runtime")?
        .block_on(async {
            while let Some(evt) = select!(
                opt_evt = rx.recv() => opt_evt,
                res = signal::ctrl_c() => {
                    res.context("interrupt handler failed")?;

                    if atty::is(atty::Stream::Stdout) {
                        eprint!("\r");
                    }

                    eprintln!("^C received, stopping...");

                    None
                }
            ) {
                let evt = evt.context("filesystem watcher encountered an error")?;

                if let EventKind::Modify(ModifyKind::Data(_)) = evt.kind {
                    eprintln!("Config change detected; rerunning...");

                    generate_impl(&cache, &opts)?;
                }
            }

            Ok(())
        })
}
