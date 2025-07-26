#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use log::{info, warn};
use uefi::CStr16;
use uefi::boot::{LoadImageSource, ScopedProtocol};
use uefi::prelude::*;
use uefi::proto::device_path::build::{self, DevicePathBuilder};
use uefi::proto::device_path::{DevicePath, DeviceSubType, DeviceType, LoadedImageDevicePath};
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::media::file::{File, FileAttribute, FileInfo, FileMode, FileType};
use uefi::proto::media::fs::SimpleFileSystem;

const KERNEL_PATH: &CStr16 = cstr16!("\\zebrafish-kernel");
const CMDLINE_PATH: &CStr16 = cstr16!("\\cmdline.txt");
const FALLBACK_CMDLINE: &CStr16 = cstr16!("initrd=\\zebrafish-initrd");

fn get_shell_app_device_path<'a>(
    storage: &'a mut Vec<u8>,
    path_name: &'a CStr16,
) -> &'a DevicePath {
    let loaded_image_device_path =
        boot::open_protocol_exclusive::<LoadedImageDevicePath>(boot::image_handle())
            .expect("failed to open LoadedImageDevicePath protocol");

    let mut builder = DevicePathBuilder::with_vec(storage);
    for node in loaded_image_device_path.node_iter() {
        if node.full_type() == (DeviceType::MEDIA, DeviceSubType::MEDIA_FILE_PATH) {
            break;
        }

        builder = builder.push(&node).unwrap();
    }
    builder = builder.push(&build::media::FilePath { path_name }).unwrap();
    builder.finalize().unwrap()
}

fn get_cmdline<'a>(
    fs: &'a mut ScopedProtocol<SimpleFileSystem>,
    buf16: &'a mut [u16],
) -> &'a CStr16 {
    // Open the volume
    let mut volume = fs.open_volume().expect("Failed to open volume");

    // Try to open \cmdline.txt
    let file_open_result = volume.open(CMDLINE_PATH, FileMode::Read, FileAttribute::empty());

    if let Ok(mut file) = file_open_result {
        let mut buf = [0; 16384];
        let mut info_buf = [0; 256]; // Sufficiently large buffer for FileInfo
        let file_info = file
            .get_info::<FileInfo>(&mut info_buf)
            .expect("Failed to get file info");
        let file_size = file_info.file_size() as usize;

        let read_size = if file_size >= buf.len() {
            buf.len() - 2 // Leave space for null terminator
        } else {
            file_size
        };

        let file_type = file.into_type().expect("Failed to get file type");
        if let FileType::Regular(mut regular_file) = file_type {
            let bytes_read = regular_file
                .read(&mut buf[..read_size])
                .expect("Failed to read cmdline.txt");

            if bytes_read == 0 {
                warn!("cmdline.txt is empty, using fallback command line");
                FALLBACK_CMDLINE
            } else {
                buf[bytes_read] = 0; // Null-terminate the buffer
                buf[bytes_read + 1] = 0; // Ensure the next byte is also null
                let utf8_str = core::str::from_utf8(&buf[..bytes_read])
                    .expect("Failed to convert cmdline.txt content to UTF-8 string");
                let cstr16_buf = CStr16::from_str_with_buf(utf8_str, buf16)
                    .expect("Failed to convert UTF-8 string to CStr16");
                info!("Successfully read cmdline.txt");
                cstr16_buf
            }
        } else {
            warn!("cmdline.txt is not a regular file, using fallback command line");
            FALLBACK_CMDLINE
        }
    } else {
        warn!("Failed to open cmdline.txt, using fallback command line");
        FALLBACK_CMDLINE
    }
}

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();

    info!("Zebrafish is booting...");

    // Get FileSystem Protocol
    let mut fs = boot::get_image_file_system(boot::image_handle())
        .expect("Failed to get FileSystemProtocol");

    let mut buf16 = [0u16; 16384]; // Allocate a buffer for CStr16
    let cmdline_str = get_cmdline(&mut fs, &mut buf16);

    info!("Loading kernel...");

    // Load the kernel (EFI stub Linux kernel)
    let mut storage = Vec::new();
    let kernel_path_dp = get_shell_app_device_path(&mut storage, KERNEL_PATH);
    let kernel_image_handle = boot::load_image(
        boot::image_handle(),
        LoadImageSource::FromDevicePath {
            device_path: kernel_path_dp,
            boot_policy: uefi::proto::BootPolicy::ExactMatch,
        },
    )
    .expect("Failed to load kernel image");

    let mut shell_loaded_image = boot::open_protocol_exclusive::<LoadedImage>(kernel_image_handle)
        .expect("failed to open LoadedImage protocol");

    unsafe {
        shell_loaded_image
            .set_load_options(cmdline_str.as_ptr().cast(), cmdline_str.num_bytes() as u32)
    };

    // Debug: Booting kernel
    info!("Booting kernel...");

    // Start the kernel
    boot::start_image(kernel_image_handle).expect("failed to launch the shell app");

    Status::SUCCESS
}
