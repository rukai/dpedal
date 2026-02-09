use std::{env, process::Command};

fn main() {
    for file in std::fs::read_dir("../dpedal_firmware")
        .unwrap()
        .chain(std::fs::read_dir("../dpedal_config").unwrap())
    {
        let path = file.unwrap().path();

        if path.file_name().unwrap().to_str().unwrap() != "target" {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }

    let profile = env::var("PROFILE").unwrap();

    println!("cargo:rerun-if-env-changed=PROFILE");

    let cargo_args = if profile == "release" {
        vec!["build", "--release"]
    } else {
        vec!["build"]
    };

    let firmware_dir = "../dpedal_firmware";

    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

    let status = Command::new(cargo)
        .current_dir(firmware_dir)
        .args(&cargo_args)
        // This is created by caller cargo but isnt overwritten by called cargo, so we have to kill it manually :/
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .status()
        .unwrap();

    if !status.success() {
        panic!("cargo build failed");
    }

    let firmware_path =
        format!("../../dpedal_firmware/target/thumbv6m-none-eabi/{profile}/dpedal_firmware");

    println!("cargo:rustc-env=FIRMWARE_PATH={firmware_path}");
}
