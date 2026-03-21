use dpedal_config::{CONFIG_AVAILABLE_SIZE, CONFIG_OFFSET, FIRMWARE_OFFSET, FIRMWARE_SIZE};
use miette::{Result, miette};
use picoboot_rs::{
    PICO_FLASH_START, PICO_PAGE_SIZE, PICO_SECTOR_SIZE, PICO_STACK_POINTER, PicobootConnection,
    TargetID,
};
use rusb::Context;

pub fn flash_device(firmware: &[u8], config: &[u8]) -> Result<()> {
    if firmware.len() > FIRMWARE_SIZE {
        return Err(miette!(
            "Firmware is too large to flash, is {:?} bytes but must be less than or equal to {:?} bytes.",
            firmware.len(),
            FIRMWARE_SIZE
        ));
    }
    if config.len() > CONFIG_AVAILABLE_SIZE {
        return Err(miette!(
            "Config is too large to flash, is {:?} bytes but must be less than or equal to {:?} bytes.",
            config.len(),
            CONFIG_AVAILABLE_SIZE
        ));
    }

    let ctx = Context::new().map_err(|e| miette!(e).context("could not initialize libusb"))?;
    // create connection object
    println!("Connecting to device");
    let mut conn =
        PicobootConnection::new(ctx, None).expect("failed to connect to PICOBOOT interface");

    conn.reset_interface().expect("failed to reset interface");
    conn.access_exclusive_eject()
        .expect("failed to claim access");
    conn.exit_xip().expect("failed to exit from xip mode");

    println!("writing {} KB of firmware", firmware.len() as f32 / 1000.0);
    flash_bytes_at_offset(&mut conn, firmware, FIRMWARE_OFFSET);
    println!("writing {} KB of config", config.len() as f32 / 1000.0);
    flash_bytes_at_offset(&mut conn, config, CONFIG_OFFSET);

    // reboot device to start firmware
    let delay = 500; // in milliseconds
    match conn.get_device_type() {
        TargetID::Rp2040 => {
            conn.reboot(0x0, PICO_STACK_POINTER, delay)
                .expect("failed to reboot device");
        }
        TargetID::Rp2350 => conn.reboot2_normal(delay).expect("failed to reboot device"),
    }

    Ok(())
}

fn flash_bytes_at_offset(conn: &mut PicobootConnection<Context>, data: &[u8], offset: usize) {
    let fw_pages = bin_pages(data);
    // erase space on flash
    for (i, _) in fw_pages.iter().enumerate() {
        if i.is_multiple_of(10) {
            print!("-");
        }
        let addr = offset as u32 + (i as u32) * PICO_PAGE_SIZE + PICO_FLASH_START;
        if addr.is_multiple_of(PICO_SECTOR_SIZE) {
            conn.flash_erase(addr, PICO_SECTOR_SIZE)
                .expect("failed to erase flash");
        }
    }

    for (i, page) in fw_pages.iter().enumerate() {
        if i.is_multiple_of(10) {
            print!(".");
        }
        let addr = offset as u32 + (i as u32) * PICO_PAGE_SIZE + PICO_FLASH_START;

        // write page to flash
        conn.flash_write(addr, page).expect("failed to write flash");

        // confirm flash write was successful
        let read = conn
            .flash_read(addr, PICO_PAGE_SIZE)
            .expect("failed to read flash");
        let matching = page.iter().zip(&read).all(|(&a, &b)| a == b);
        assert!(matching, "page does not match flash");
    }
    println!();
}

fn bin_pages(fw: &[u8]) -> Vec<Vec<u8>> {
    let mut fw_pages: Vec<Vec<u8>> = vec![];
    let len = fw.len();

    // splits the binary into sequential pages
    for i in (0..len).step_by(PICO_PAGE_SIZE as usize) {
        let size = std::cmp::min(len - i, PICO_PAGE_SIZE as usize);
        let mut page = fw[i..i + size].to_vec();
        page.resize(PICO_PAGE_SIZE as usize, 0);
        fw_pages.push(page);
    }

    fw_pages
}
