use crate::flash::WriteConfig;
use clap::Parser;
use miette::Result;

pub mod buffer_nor_flash;
pub mod cli;
pub mod config;
pub mod elf;
pub mod flash;

fn main() -> Result<()> {
    // TODO: use this once -Z bindeps stabilizes
    // let firmware_bytes = elf::elf_to_bin(include_bytes!(env!(
    //     "CARGO_BIN_FILE_DPEDAL_FIRMWARE_dpedal_firmware"
    // )))?;
    let firmware_bytes = elf::elf_to_bin(include_bytes!(env!("FIRMWARE_PATH")))?;

    let cli = cli::Args::parse();
    let config = if cli.erase_config {
        WriteConfig::Clear
    } else {
        WriteConfig::Config(Box::new(config::load(cli.path)?))
    };
    flash::flash_device(&firmware_bytes, config)?;

    println!("Succesfully flashed!");
    Ok(())
}
