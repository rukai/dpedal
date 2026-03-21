use clap::Parser;
use dpedal_config::CONFIG_AVAILABLE_SIZE;
use miette::Result;

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
    let config_bytes = if cli.erase_config {
        vec![0; CONFIG_AVAILABLE_SIZE]
    } else {
        let config = config::load(cli.path)?;
        config::encode_config(&config)?
    };
    flash::flash_device(&firmware_bytes, &config_bytes)?;

    println!("Succesfully flashed!");
    Ok(())
}
