# Litho

A simple and lightweight library and CLI tool to write images to block devices.

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
