use std::io::{self, Write};

use crate::buffer_nor_flash::BufferNorFlash;
use dpedal_config::{
    CONFIG_AVAILABLE_SIZE, CONFIG_OFFSET, Config, ConfigKey, FIRMWARE_OFFSET, FIRMWARE_SIZE,
    PROFILE_SERIALIZED_SIZE,
};
use miette::{Result, miette};
use picoboot_rs::{
    PICO_FLASH_START, PICO_PAGE_SIZE, PICO_SECTOR_SIZE, PICO_STACK_POINTER, PicobootConnection,
    TargetID,
};
use rusb::Context;
use sequential_storage::{
    cache::NoCache,
    map::{MapConfig, MapStorage},
};

pub enum WriteConfig {
    Config(Box<Config>),
    Clear,
}

pub fn flash_device(firmware: &[u8], config: WriteConfig) -> Result<()> {
    if firmware.len() > FIRMWARE_SIZE {
        return Err(miette!(
            "Firmware is too large to flash, is {:?} bytes but must be less than or equal to {:?} bytes.",
            firmware.len(),
            FIRMWARE_SIZE
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
    erase_flash(&mut conn, FIRMWARE_OFFSET, firmware.len());
    write_flash(&mut conn, firmware, FIRMWARE_OFFSET);
    println!();

    match config {
        WriteConfig::Config(config) => {
            flash_config(&mut conn, config)?;
        }
        WriteConfig::Clear => {
            println!("erasing config region");
            erase_flash(&mut conn, CONFIG_OFFSET, CONFIG_AVAILABLE_SIZE);
            println!();
        }
    }

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

fn flash_config(conn: &mut PicobootConnection<Context>, config: Box<Config>) -> Result<()> {
    let mut nor_flash = BufferNorFlash::new(CONFIG_AVAILABLE_SIZE);
    {
        let mut storage = MapStorage::new(
            &mut nor_flash,
            MapConfig::new(0..CONFIG_AVAILABLE_SIZE as u32),
            NoCache::new(),
        );
        let mut data_buffer = vec![0u8; PROFILE_SERIALIZED_SIZE + 32];

        // Write Meta
        futures::executor::block_on(
            storage.store_item::<&[u8]>(
                &mut data_buffer,
                &ConfigKey::Meta.key(),
                &postcard::to_stdvec(&config.meta)
                    .map_err(|e| miette!(e))?
                    .as_slice(),
            ),
        )
        .map_err(|e| miette!("Failed to write meta to flash: {:?}", e))?;

        // Write each Profile
        for (i, profile) in config.profiles.iter().enumerate() {
            futures::executor::block_on(
                storage.store_item::<&[u8]>(
                    &mut data_buffer,
                    &ConfigKey::Profile(i as u8).key(),
                    &postcard::to_stdvec(profile)
                        .map_err(|e| miette!(e))?
                        .as_slice(),
                ),
            )
            .map_err(|e| miette!("Failed to write profile {} to flash: {:?}", i, e))?;
        }

        // Write ProfileCount
        futures::executor::block_on(storage.store_item::<u8>(
            &mut data_buffer,
            &ConfigKey::ProfileCount.key(),
            &(config.profiles.len() as u8),
        ))
        .map_err(|e| miette!("Failed to write profile count to flash: {:?}", e))?;
    }
    let buffer = nor_flash.into_buffer();

    if buffer.len() > CONFIG_AVAILABLE_SIZE {
        return Err(miette!(
            "Config is too large to flash, is {:?} bytes but must be less than or equal to {:?} bytes.",
            buffer.len(),
            CONFIG_AVAILABLE_SIZE
        ));
    }
    println!("writing {} KB of config", buffer.len() as f32 / 1000.0);

    // Erase all config flash but only write the number of config bytes that we actually have
    // This is done to ensure that old bits of config arent picked up by sequential-storage.
    // In theory this could happen as the items are left with valid headers/CDCs
    // TODO: It is however possible that with a better understanding of how sequential-storage works,
    // we could remove this logic, or reduce the amount that we erase, to make flashing process faster.
    erase_flash(conn, CONFIG_OFFSET, CONFIG_AVAILABLE_SIZE);
    write_flash(conn, &buffer, CONFIG_OFFSET);
    println!();
    Ok(())
}

fn erase_flash(conn: &mut PicobootConnection<Context>, offset: usize, size: usize) {
    let num_sectors = size.div_ceil(PICO_SECTOR_SIZE as usize);
    for i in 0..num_sectors {
        if i.is_multiple_of(10) {
            print!("-");
            io::stdout().flush().unwrap();
        }
        let addr = offset as u32 + (i as u32) * PICO_SECTOR_SIZE + PICO_FLASH_START;
        conn.flash_erase(addr, PICO_SECTOR_SIZE)
            .expect("failed to erase flash");
    }
}

fn write_flash(conn: &mut PicobootConnection<Context>, data: &[u8], offset: usize) {
    for (i, page) in bin_pages(data).iter().enumerate() {
        if i.is_multiple_of(10) {
            print!(".");
            io::stdout().flush().unwrap();
        }
        let addr = offset as u32 + (i as u32) * PICO_PAGE_SIZE + PICO_FLASH_START;

        // write page to flash
        conn.flash_write(addr, page).expect("failed to write flash");

        // confirm flash write was successful
        let read = conn
            .flash_read(addr, PICO_PAGE_SIZE)
            .expect("failed to read flash");
        assert!(
            page.iter().zip(&read).all(|(&a, &b)| a == b),
            "page does not match flash"
        );
    }
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
