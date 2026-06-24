//! Flash/clone dispatch: simulated by default, real block I/O when `real-io` is enabled.
//!
//! - **Default (`simulated-io`)** — safe for development and `cargo test`; no block writes.
//! - **Release (`real-io`)** — `cargo build --no-default-features --features real-io`.

use crate::progress::OperationProgress;
use anyhow::Result;

#[cfg(all(feature = "real-io", feature = "simulated-io"))]
compile_error!("Features `real-io` and `simulated-io` are mutually exclusive. Build real I/O with: cargo build --no-default-features --features real-io");

#[cfg(all(test, feature = "real-io"))]
compile_error!(
    "Do not run tests with `real-io` enabled (risk of accidental disk writes). Use default features: cargo test"
);

#[cfg(not(feature = "real-io"))]
use crate::cli_simulate;

/// True when flash/clone use the simulator instead of `liblitho::flash` / `clone`.
pub const USES_SIMULATED_IO: bool = cfg!(not(feature = "real-io"));

pub fn flash_io<F>(
    image: &str,
    device: &str,
    block_size: usize,
    silent: bool,
    verify: bool,
    progress: Option<F>,
) -> Result<()>
where
    F: FnMut(OperationProgress),
{
    #[cfg(feature = "real-io")]
    {
        crate::flash(
            image.to_string(),
            device.to_string(),
            block_size,
            silent,
            verify,
            progress,
        )
    }

    #[cfg(not(feature = "real-io"))]
    {
        cli_simulate::simulate_flash(image, device, block_size, silent, verify, progress)
    }
}

pub fn clone_io<F>(
    device: &str,
    file: &str,
    block_size: usize,
    silent: bool,
    progress: Option<F>,
) -> Result<()>
where
    F: FnMut(OperationProgress),
{
    #[cfg(feature = "real-io")]
    {
        crate::clone(
            device.to_string(),
            file.to_string(),
            block_size,
            silent,
            progress,
        )
    }

    #[cfg(not(feature = "real-io"))]
    {
        cli_simulate::simulate_clone(device, file, block_size, silent, progress)
    }
}

pub fn in_progress_suffix() -> &'static str {
    if USES_SIMULATED_IO {
        " (simulation — disk writes disabled)"
    } else {
        ""
    }
}

pub fn complete_suffix() -> &'static str {
    if USES_SIMULATED_IO {
        " (simulation)"
    } else {
        ""
    }
}