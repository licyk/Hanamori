use anyhow::{Context, Result};

/// Runs a future to completion on a fresh runtime.
///
/// Used by the CLI, which is synchronous apart from the download and extraction work.
pub fn block_on<F: std::future::Future>(future: F) -> Result<F::Output> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("cannot start an async runtime")?;

    Ok(runtime.block_on(future))
}
