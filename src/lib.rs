use libc::O_DIRECT;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::os::unix::fs::OpenOptionsExt;

pub fn write_image(
    img_path: String,
    device_path: String,
    block_size: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let img_file = match File::open(&img_path) {
        Ok(file) => file,
        Err(e) => {
            println!("Image file not found");
            return Err(Box::new(e));
        }
    };
    let file_size = img_file.metadata().unwrap().len();

    // Write the image to the device
    let mut device_file = match OpenOptions::new()
        .write(true)
        .custom_flags(O_DIRECT)
        .custom_flags(libc::O_SYNC)
        .custom_flags(libc::O_DSYNC)
        .open(device_path.clone())
    {
        Ok(device_file) => device_file,
        Err(e) => {
            println!("Error opening device file");
            return Err(Box::new(e));
        }
    };

    let img_file = match File::open(&img_path) {
        Ok(img_file) => img_file,
        Err(e) => {
            println!("Error opening image file");
            return Err(Box::new(e));
        }
    };

    let mut reader = BufReader::new(img_file);
    let mut buffer = vec![0u8; block_size];
    println!("Writing image to the device...: {file_size}");
    let mut count: usize = 0;
    while let Ok(bytes_read) = reader.read(&mut buffer) {
        if bytes_read == 0 {
            break;
        }
        device_file.write_all(&buffer[..bytes_read])?;
        count = count + bytes_read;
        println!("written {count}/{file_size}");
    }

    Ok(())
}
