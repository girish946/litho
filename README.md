# Litho

A simple and lightweight library and CLI tool to write images to block devices.

## TUI

An interactive terminal UI is available via the `litho-tui` binary:

```bash
cargo run --bin litho-tui
```

[![TUI demo](demo.gif)](demo.cast)

Play the full recording locally:

```bash
asciinema play demo.cast
```

To regenerate the preview GIF after re-recording:

```bash
agg demo.cast demo.gif
```

## Command usage

- Cloning a device to an image file:

```bash
litho clone --help
Usage: litho clone [OPTIONS] --file <FILE> --device <DEVICE>

Options:
  -f, --file <FILE>              file to which device should be cloned
  -d, --device <DEVICE>          device
  -b, --block-size <BLOCK_SIZE>  block size
  -s, --silent <SILENT>          message to be published [possible values: true, false]
  -h, --help                     Print help
```

- Flashing an image file to a device:

```bash
litho flash --help
Usage: litho flash [OPTIONS] --file <FILE> --device <DEVICE>

Options:
  -f, --file <FILE>              file to be written to the device
  -d, --device <DEVICE>          device
  -b, --block-size <BLOCK_SIZE>  block size
  -s, --silent <SILENT>          message to be published [possible values: true, false]
  -h, --help                     Print help
```

## Building portable Linux binaries

**Problem:** If you compile on a bleeding-edge Linux distribution, the resulting binary will depend on a very new glibc (you may see `GLIBC_2.43 not found` or similar on older systems like Ubuntu 22.04/24.04, Debian 12, etc.).

### Best option: Fully static musl build (recommended for the `litho` CLI)

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl --bin litho
```

The binary will be at:
`target/x86_64-unknown-linux-musl/release/litho`

This is almost completely static and runs on virtually any x86_64 Linux.

Copy it to `lithographer/src-tauri/resources/litho` before building the GUI.

(The vendored OpenSSL in this `Cargo.toml` makes musl builds much easier.)

### Good alternative: Build inside an older container

```bash
docker run --rm -v "$PWD":/src -w /src rust:1.80-slim-bookworm \
  cargo build --release --bin litho
```

This will only require glibc ~2.36, which is compatible with most current distros.

## API usage

- Clone a device to an image file:

```rust
use litho::clone;
let image = "/home/user/image-file.img".to_string();
let device = "/dev/sda".to_string();
let block_size = 4096;

fn callback_fn(percentage: f64) {
    println!("{percentage}%");
}

litho::clone(image, device, block_size as usize, false, callback);
```

- FLASH an image file to a device:

```rust
use litho::flash
let image = "/home/user/image-file.img".to_string();
let device = "/dev/sda".to_string();
let block_size = 4096;

fn callback_fn(percentage: f64) {
    println!("{percentage}%");
}

litho::flash(image, device, block_size as usize, false, callback);
