pub mod algo;
pub mod map;
mod waves;

use std::{
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use anyhow::anyhow;
use dispose::defer;
use log::{debug, info, warn};
use notify::{event::ModifyKind, EventKind, RecursiveMode, Watcher};
use tokio::{runtime, select, signal, sync::mpsc};

use crate::{
    cache,
    cache::prelude::*,
    cli::{CacheMode, GenerateOpts},
    config::GenerateConfig,
    error::prelude::*,
};

async fn generate_impl<C: for<'a> Cache<'a>>(
    cache: C,
    opts: &GenerateOpts,
    cancel: Arc<AtomicBool>,
) -> Result<()> {
    let cfg = GenerateConfig::read(opts).context("failed to get config")?;
    debug!("{:#?}", cfg);

    let (map_cfg, fmt_opts) = map::Config::for_generate(cfg.map);

    map::compute(cache, map_cfg, cancel).context("failed to generate new dissonance map")?;

    Ok(())
}

fn run_cancelable<F: FnOnce(Arc<AtomicBool>) -> FR, FR: Future<Output = Result<T>>, T>(
    f: F,
) -> Result<Option<T>> {
    let r = runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to setup Tokio runtime")?;

    let cancel = Arc::new(AtomicBool::new(false));

    let dfr = defer({
        let cancel = cancel.clone();
        move || cancel.store(true, Ordering::Relaxed)
    });

    r.block_on(async move {
        let ret = select! {
            res = f(cancel.clone()) => res.map(Some),
            res = signal::ctrl_c() => {
                res.context("interrupt handler failed")?;

                if atty::is(atty::Stream::Stdout) {
                    eprint!("\r");
                }

                info!("^C received, stopping...");

                Ok(None)
            }
        };

        std::mem::drop(dfr);
        ret
    })
}

pub fn generate(cache_mode: CacheMode, opts: GenerateOpts) -> Result<()> {
    let cache = cache::from_opts(cache_mode);

    run_cancelable(|cancel| generate_impl(cache, &opts, cancel))
        .map(|s| s.map_or_else(|| (), |()| ()))
}

pub fn watch(cache_mode: CacheMode, opts: GenerateOpts) -> Result<()> {
    let cache = Arc::new(cache::from_opts(cache_mode));

    run_cancelable(|cancel| async move {
        if opts.config.exists() {
            info!("Running initial pass...");

            // TODO: use tokio to watch for interrupts while this runs?
            generate_impl(cache.clone(), &opts, cancel.clone()).await?;
        } else {
            warn!("Config file doesn't exist yet, waiting for a new one...");
        }

        info!("Listening for changes...");

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

        while let Some(evt) = rx.recv().await {
            let evt = evt.context("filesystem watcher encountered an error")?;

            if let EventKind::Modify(ModifyKind::Data(_)) = evt.kind {
                info!("Config change detected; rerunning...");

                generate_impl(&cache, &opts, cancel.clone()).await?;
            }
        }

        Ok(())
    })
    .map(|s| s.map_or_else(|| (), |()| ()))
}
