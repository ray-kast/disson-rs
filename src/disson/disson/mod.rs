use std::{
    borrow::Borrow,
    fs::File,
    future::Future,
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use anyhow::anyhow;
use dispose::defer;
use futures::prelude::*;
use log::{debug, info, trace, warn};
use map::DissonMap;
use notify::{event::ModifyKind, EventKind, RecursiveMode, Watcher};
use tokio::{runtime, select, signal, sync::mpsc};

use crate::{
    cache,
    cache::prelude::*,
    cli::{CacheMode, GenerateOpts},
    config::{GenerateConfig, MapFormat, MapOutput},
    error::cancel::prelude::*,
};

pub mod algo;
pub mod map;
mod wave;

fn write_xsv<W: io::Write>(
    map: &DissonMap,
    delim: u8,
    out: W,
    cancel: &AtomicBool,
) -> CancelResult<()> {
    let mut writer = csv::WriterBuilder::new().delimiter(delim).from_writer(out);

    trace!("Outputting map in delimited format...");

    writer
        .write_field("x/y")
        .context("failed to write first xSV field")?;
    writer
        .serialize((0..map.size.x as usize).collect::<Vec<_>>())
        .context("failed to write xSV column headers")?;

    for (i, chunk) in map.data.chunks(map.size.x as usize).enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(Cancelled);
        }

        writer
            .write_field(i.to_string())
            .context("failed to write xSV row index")?;
        writer
            .serialize(chunk)
            .context("failed to write xSV data")?;

        if cancel.load(Ordering::Relaxed) {
            return Err(Cancelled);
        }

        writer.flush().context("failed to flush xSV data")?;
    }

    Ok(())
}

fn generate_impl<C: for<'a> Cache<'a>>(
    cache: C,
    opts: impl Borrow<GenerateOpts>,
    cancel: impl Borrow<AtomicBool>,
) -> CancelResult<()> {
    let opts = opts.borrow();
    let cancel = cancel.borrow();

    trace!("Reading config...");

    let cfg = GenerateConfig::read(opts).context("failed to get config")?;

    trace!("Computing map...");

    let map_cfg = map::Config::for_generate(&cfg.map);
    let map = map::compute(cache, map_cfg, cancel).context("failed to generate dissonance map")?;

    match opts.ty()? {
        MapFormat::Xsv(ref d) => match opts.out {
            MapOutput::Stdout => write_xsv(&map, *d, io::stderr(), cancel)?,
            MapOutput::File(ref p) => write_xsv(
                &map,
                *d,
                File::create(p).context("failed to open output file")?,
                cancel,
            )?,
        },
        MapFormat::Png => todo!(),
    }

    Ok(())
}

fn generate_async<C: for<'a> Cache<'a> + 'static>(
    cache: C,
    opts: impl Borrow<GenerateOpts> + Send + 'static,
    cancel: impl Borrow<AtomicBool> + Send + 'static,
) -> impl Future<Output = CancelResult<()>> {
    tokio::task::spawn_blocking(|| generate_impl(cache, opts, cancel)).map(Result::unwrap)
}

fn run_cancelable<
    F: FnOnce(Arc<AtomicBool>) -> FR + Send,
    FR: Future<Output = CancelResult<T>> + Send,
    T: Send,
>(
    f: F,
) -> Result<Option<T>> {
    let r = runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to setup Tokio runtime")?;

    let cancel = Arc::new(AtomicBool::new(false));

    match r.block_on(async move {
        let dfr = defer(|| {
            trace!("Setting cancelled flag");
            cancel.store(true, Ordering::SeqCst)
        });

        let ret = select! {
            r = signal::ctrl_c() => {
                r.context("interrupt handler failed")?;

                if atty::is(atty::Stream::Stdout) {
                    eprint!("\r");
                }

                info!("^C received, stopping...");

                Err(Cancelled)
            },
            r = f(cancel.clone()) => r,
        };

        std::mem::drop(dfr);
        ret
    }) {
        Ok(r) => Ok(Some(r)),
        Err(e) => e.into_result().map(|()| {
            debug!("Operation cancelled.");

            None
        }),
    }
}

pub fn generate(cache_mode: CacheMode, opts: GenerateOpts) -> Result<()> {
    let cache = cache::from_opts(cache_mode);

    run_cancelable(move |cancel| generate_async(cache, opts, cancel))
        .map(|s| s.map_or_else(|| (), |()| ()))
}

pub fn watch(cache_mode: CacheMode, opts: GenerateOpts) -> Result<()> {
    // TODO: can this be scoped to drop the Arc?
    let cache = Arc::new(cache::from_opts(cache_mode));
    let opts = Arc::new(opts);

    run_cancelable(move |cancel| async move {
        if opts.config.exists() {
            info!("Running initial pass...");

            generate_async(cache.clone(), opts.clone(), cancel.clone()).await?;
        } else {
            warn!("Config file doesn't exist yet, waiting for a new one...");
        }

        info!("Listening for changes...");

        let (tx, mut rx) = mpsc::unbounded_channel();

        let mut watcher = notify::immediate_watcher(move |evt| tx.send(evt).unwrap()).context(
            "failed to open filesystem
    watcher",
        )?;

        watcher
            .watch(
                opts.config
                    .parent()
                    .ok_or_else(|| anyhow!("invalid config path {:?}", opts.config))?,
                RecursiveMode::NonRecursive,
            )
            .with_context(|| format!("failed to watch file {:?}", opts.config))?;

        while let Some(evt) = rx.recv().await {
            let evt = evt.context(
                "filesystem watcher encountered an
    error",
            )?;

            if let EventKind::Modify(ModifyKind::Data(_)) = evt.kind {
                info!("Config change detected; rerunning...");

                generate_async(cache.clone(), opts.clone(), cancel.clone()).await?;
            }
        }

        Ok(())
    })
    .map(|s| s.map_or_else(|| (), |()| ()))
}
