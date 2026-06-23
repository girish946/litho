# Litho

A Rust library and CLI for flashing disk images to block devices and cloning block devices to image files. Includes an interactive terminal UI (`litho-tui`) for guided flash and clone operations on Linux.

**Primary platform:** Linux (full CLI, library, and TUI support). macOS and Windows platform layers exist but device enumeration and end-to-end workflows are not complete — see [Platform support](#platform-support).

## Features

- **Flash** — write a raw image (`.img`, `.iso`, or `.img.xz`) to a block device with optional SHA-256 verification
- **Clone** — read an entire block device into an image file
- **Query** — list storage devices from `/sys/block` (Linux)
- **Progress API** — structured `OperationProgress` callbacks (phase, bytes, percentage, message)
- **TUI** — responsive terminal UI with device/file pickers, privilege elevation via `pkexec`, and file logging

## Requirements

- Linux for production use (flash/clone require root)
- Rust 1.70+ (2021 edition)
- For `litho-tui` privilege elevation: `pkexec` (polkit) and a running polkit authentication agent
- Terminal at least **60×24** characters for the TUI

## Building

```bash
cargo build --release
```

Binaries:

| Binary | Path |
|--------|------|
| CLI | `target/release/litho` |
| TUI | `target/release/litho-tui` |

Run from the project directory:

```bash
cargo run --bin litho -- --help
cargo run --bin litho-tui
```

## `litho` CLI

Flash and clone require **root** (e.g. `sudo`). Set log verbosity with `RUST_LOG`:

```bash
RUST_LOG=info sudo litho flash --file image.img --device /dev/sdX
```

### Flash

Write an image file to a block device. `.xz` images are decompressed on the fly.

```bash
sudo litho flash --file /path/to/image.img --device /dev/sdX
sudo litho flash -f image.img.xz -d /dev/sdX -b 4096
sudo litho flash -f image.img -d /dev/sdX -s true   # suppress progress logs
```

| Option | Description |
|--------|-------------|
| `-f, --file` | Image file to write (required) |
| `-d, --device` | Target block device (required) |
| `-b, --block-size` | I/O buffer size in bytes (default: `4096`) |
| `-s, --silent` | Suppress progress output (`true` / `false`, default: `false`) |

Global option (all subcommands):

| Option | Description |
|--------|-------------|
| `--json-progress` | Emit one JSON line per `OperationProgress` event on stdout |

### Clone

Read a block device into an image file. **Argument order:** `--device` first in the API; the CLI accepts both flags in any order.

```bash
sudo litho clone --device /dev/sdX --file /path/to/backup.img
sudo litho clone -d /dev/sdX -f backup.img -b 1048576
```

| Option | Description |
|--------|-------------|
| `-d, --device` | Source block device (required) |
| `-f, --file` | Output image file (required) |
| `-b, --block-size` | I/O buffer size in bytes (default: `4096`) |
| `-s, --silent` | Suppress progress output |

### Query

List detected block devices (JSON per line via logging):

```bash
RUST_LOG=info litho query
RUST_LOG=info litho query --device /dev/sdb
```

## `litho-tui` (interactive)

```bash
cargo run --bin litho-tui
# or
./target/release/litho-tui
```

[![TUI demo](demo.gif)](demo.cast)

Replay the full terminal recording:

```bash
asciinema play demo.cast
```

Regenerate the preview GIF after re-recording:

```bash
agg demo.cast demo.gif
# or
./scripts/record-demo.sh record
./scripts/record-demo.sh gif
```

### Launch options

```bash
litho-tui --help
```

| Option | Description |
|--------|-------------|
| `-m, --mode` | `flash` or `clone` (also accepts `backup` as alias for clone) |
| `-d, --device` | Pre-select block device (e.g. `/dev/sdb`) |
| `-i, --image` / `-f, --file` | Pre-fill image path (flash source or clone output) |
| `--start` | Start the operation immediately when already privileged (see below) |
| `--log-file` | Log file path (default: `~/.cache/litho/litho-tui.log`) |
| `--log-level` | `error`, `warn`, `info`, `debug`, `trace` (default: `info`) |

Debug logging to stderr (before the TUI starts): `--log-file=-` or `LITHO_LOG_STDERR=1`.

Log files rotate to `.log.old` when they exceed 5 MiB.

### Keyboard shortcuts

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Move focus between sections |
| `1` / `←` | Flash mode |
| `2` / `→` | Clone mode |
| `d` / `Enter` (device focused) | Open device picker |
| `r` (device focused) | Refresh device list |
| `f` / `Enter` (file focused) | Open file picker |
| `Enter` (start focused) | Start operation |
| `c` / `Esc` (while running) | Cancel operation |
| `q` | Quit |

### Privilege elevation

Flash and clone require root. When an unprivileged user starts an operation:

1. A confirmation dialog appears in the TUI.
2. On confirm, the terminal is restored and the process **`exec`s into `pkexec`** with pre-filled `--mode`, `--device`, and `--image` only.
3. **`--start` is never passed by pkexec** — after elevation you must press **Start** again (or pass `--start` yourself when already root).
4. On success, the elevated session shows **Privileged** in the footer.

Run directly as root to skip elevation:

```bash
sudo litho-tui --mode flash --device /dev/sdb --image ./image.img --start
```

### TUI logging

Operational logs are written to a file so the TUI screen stays clean. Default path:

```
~/.cache/litho/litho-tui.log
```

(`$XDG_CACHE_HOME/litho/litho-tui.log` when set.)

Terminal initialization failures and elevation errors are recorded there. Example:

```bash
litho-tui --log-level debug --log-file /tmp/litho-tui.log
```

### Simulation mode

The TUI runs a **simulated** progress loop for flash/clone — it does **not** call `liblitho::flash` or `clone`. Use the `litho` CLI or library API for real disk I/O. The status line indicates simulation explicitly.

### Device list notes

- Devices are discovered via `/sys/block` on Linux.
- NVMe drives often lack a separate `vendor` sysfs file; you may see a harmless log warning and an empty vendor field — the model string (e.g. `Samsung SSD 980 1TB`) is still shown when available.
- Selecting a non-removable (fixed) disk triggers an extra confirmation dialog.

## Library API

Add the crate to your project (package name `liblitho`):

```toml
[dependencies]
liblitho = { path = "../litho" }
log = "0.4"
```

### Clone

```rust
use liblitho::progress::{OperationPhase, OperationProgress};
use liblitho::clone;

fn on_progress(p: OperationProgress) {
    if let Some(pct) = p.percentage {
        eprintln!("{:?}: {:.1}%", p.phase, pct);
    }
}

clone(
    "/dev/sdb".to_string(),      // device (source)
    "/tmp/backup.img".to_string(), // output file
    4096,                        // block size
    false,                       // silent
    Some(on_progress),           // progress callback (None to disable)
)?;
```

### Flash

```rust
use liblitho::flash;
use liblitho::progress::OperationPhase;

flash(
    "/path/to/image.img".to_string(), // image file
    "/dev/sdb".to_string(),           // device
    4096,
    false,
    Some(|p| {
        if p.phase == OperationPhase::Verifying {
            println!("Verifying…");
        }
    }),
)?;
```

### Progress types

```rust
use liblitho::progress::{OperationPhase, OperationProgress};

// Phases: Preparing, Decompressing, Writing, Verifying, Complete, Failed
let p = OperationProgress::new(OperationPhase::Writing)
    .with_bytes(1024, Some(4096))
    .with_message("Writing…");
```

### Device enumeration

```rust
use liblitho::devices::get_storage_devices;

for dev in get_storage_devices()? {
    println!("{} — {} {}", dev.device_name, dev.vendor_name, dev.model_name);
}
```

## Platform support

| Component | Linux | macOS | Windows |
|-----------|-------|-------|---------|
| Build `litho` CLI | ✅ | ⚠️ | ⚠️ |
| `flash` / `clone` | ✅ (root) | ⚠️ partial I/O | ⚠️ partial I/O |
| Device listing (`query`) | ✅ `/sys/block` | ❌ Linux-only code path | ❌ |
| `litho-tui` | ✅ | ❌ | ❌ |

See [`../platform-support.md`](../platform-support.md) for the cross-platform roadmap.

## Building portable Linux binaries

**Problem:** Building on a bleeding-edge distro links against a very new glibc (`GLIBC_2.43 not found` on older Ubuntu/Debian releases).

### Recommended: static musl build (`litho` CLI)

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl --bin litho
```

Output: `target/x86_64-unknown-linux-musl/release/litho`

Vendored OpenSSL in `Cargo.toml` helps musl builds succeed. Copy the binary to `lithographer/src-tauri/resources/litho` before building the Lithographer GUI.

### Alternative: older glibc via container

```bash
docker run --rm -v "$PWD":/src -w /src rust:1.80-slim-bookworm \
  cargo build --release --bin litho
```

`litho-tui` depends on the terminal stack and is typically built for the host environment rather than distributed as a static portable binary.

## Safety

- Flashing or cloning the **wrong device can destroy data**. Always verify the target device path.
- Prefer removable media for flash targets when possible.
- Ensure target partitions are unmounted before writing (the CLI does not unmount for you).

## Related projects

- [Lithographer](https://github.com/girish946/lithographer) — Tauri GUI built on top of `litho`

## License

MIT — see `Cargo.toml`.